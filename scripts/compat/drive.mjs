//! Launches one command of one extension in the real runtime bundle, with
//! the stand-in host on the other end of its pipe, and reports what
//! happened. One process per launch: what a command leaves behind (a
//! late rejection, a timer, a toast) is then its own and nobody else's.

import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import { createHost } from "./host.mjs";
import { createObserver, hasContent, readStack } from "./observe.mjs";
import { dummyArguments, dummyPreferences } from "./preferences.mjs";
import { bwrapArgs, environment, layout, writeAccount } from "./sandbox.mjs";

/// Hands over each line a stream carries. A line ends at a line feed and
/// nowhere else: `readline` also ends one at U+2028 and U+2029, which a
/// render with text in it may hold.
function lines(stream, each) {
  stream.setEncoding("utf8");
  let held = "";
  stream.on("data", (text) => {
    const parts = (held + text).split("\n");
    held = parts.pop();
    parts.forEach(each);
  });
}

/// What is typed into an empty list that searches as you type.
export const SEARCH_TEXT = "test";

/// The resident memory, in MiB, of a process and everything under it.
function treeRss(pid) {
  let total = 0;
  const visit = (p) => {
    try {
      const status = fs.readFileSync(`/proc/${p}/status`, "utf8");
      total += Number(/VmRSS:\s+(\d+)/.exec(status)?.[1] ?? 0) / 1024;
      for (const child of fs.readFileSync(`/proc/${p}/task/${p}/children`, "utf8").split(/\s+/)) if (child) visit(child);
    } catch {
      // Gone since it was listed.
    }
  };
  visit(pid);
  return Math.round(total);
}

/// Runs `command` (an entry of `manifest.commands`) and resolves with the
/// record of the run. Never rejects for anything the extension does.
///
///  - `extensionDir`: the extension, in a directory it may write to.
///  - `scratch`: an empty directory for this launch alone.
///  - `sandbox`: "bwrap" or "none".
export function launchCommand({ runtimeFile, extensionDir, manifest, command, scratch, network = true, sandbox = "none", timeoutMs = 15_000, quietMs = 1_200, memoryMb = 1024, nodePath = process.execPath }) {
  const dirs = layout(scratch);
  for (const dir of Object.values(dirs)) fs.mkdirSync(dir, { recursive: true });
  fs.mkdirSync(path.join(dirs.home, "Documents"), { recursive: true });
  fs.mkdirSync(path.join(dirs.home, "Downloads"), { recursive: true });
  fs.mkdirSync(path.join(dirs.home, "Desktop"), { recursive: true });
  fs.writeFileSync(path.join(dirs.home, "Documents", "notes.txt"), "A note.\n");

  const { stored, required } = dummyPreferences(manifest, command.name, dirs.home);
  if (required.length > 0) {
    fs.mkdirSync(path.join(dirs.heyoConfig, "preferences"), { recursive: true });
    fs.writeFileSync(path.join(dirs.heyoConfig, "preferences", `${manifest.name}.json`), JSON.stringify(stored));
  }
  const args = dummyArguments(command);
  const background = command.mode === "menu-bar";
  const env = environment(dirs, { extensionsDir: path.dirname(extensionDir), background });
  const node = [nodePath, `--max-old-space-size=${Math.max(128, Math.floor(memoryMb * 0.75))}`, runtimeFile];

  let child;
  if (sandbox === "bwrap") {
    const readOnly = [path.dirname(runtimeFile)];
    if (!nodePath.startsWith("/usr/")) readOnly.push(path.dirname(nodePath));
    child = spawn(
      "bwrap",
      bwrapArgs({ command: node, readOnly, writable: [scratch, extensionDir], network, chdir: dirs.home, passwd: writeAccount(scratch, dirs.home) }),
      { env, stdio: ["pipe", "pipe", "pipe"] },
    );
  } else {
    // Its own process group, so that what it starts can be stopped with it.
    child = spawn(node[0], node.slice(1), { env, cwd: dirs.home, stdio: ["pipe", "pipe", "pipe"], detached: true });
  }

  const host = createHost({ home: dirs.home });
  const observer = createObserver();
  const { state } = observer;
  const started = Date.now();
  let stderr = "";
  let ready = false;
  let oauth;
  let typed = false;
  let peakRss = 0;
  let over = false;
  let ended;

  const say = (message) => {
    if (child.stdin.writable) child.stdin.write(JSON.stringify(message) + "\n");
  };

  return new Promise((resolve) => {
    let quiet;
    const timers = [];

    const finish = (why, exit) => {
      if (ended) return;
      ended = why;
      clearTimeout(quiet);
      timers.forEach(clearTimeout);
      clearInterval(watch);
      try {
        if (sandbox === "bwrap") child.kill("SIGKILL");
        else process.kill(-child.pid, "SIGKILL");
      } catch {
        // Already gone.
      }
      const failures = state.toasts.filter((toast) => toast.style === "FAILURE");
      // What the runtime itself reports for an error nothing caught
      // (`reportError` in session.ts): logged, then shown as "Error".
      const uncaught = failures.find((toast) => toast.title === "Error" && toast.message && stderr.includes(toast.message.split("\n")[0].slice(0, 120)));
      // An error while rendering is shown in the view's place instead.
      const screen = state.view?.errorScreen;
      const thrown = uncaught?.message ?? state.doneError ?? screen?.split("\n")[0].replace(/^(?:Uncaught )?\w*Error: /, "");
      const where = { extensionDir, runtimeFile };
      const tidy = (text) => text.split(extensionDir).join("<extension>").split(scratch).join("<scratch>").split(path.dirname(runtimeFile)).join("<heyo>");
      const stack = thrown ? readStack(uncaught || !screen ? stderr : screen, thrown, where) : undefined;
      const error = thrown ? { message: thrown, ...(stack ? { ...stack, top: tidy(stack.top) } : {}) } : undefined;

      let outcome;
      if (why === "exit" || why === "never-ready" || why === "memory") outcome = "crashed";
      else if (oauth) outcome = "oauth";
      else if (background) outcome = state.menu ? "menu" : state.done ? "no-menu" : "timeout";
      else if (state.view) outcome = "view";
      // Going back to the root search is how a command with nothing to
      // show takes its leave.
      else if (state.closed || state.poppedToRoot) outcome = "finished";
      else outcome = "timeout";

      resolve({
        network,
        sandbox,
        outcome,
        /// Why the launch was stopped: "settled", "closed", "deadline", …
        stopped: why,
        ms: Date.now() - started,
        view: state.view ? { ...state.view, rootId: undefined, errorScreen: screen ? true : undefined, excerpt: screen ? undefined : state.view.excerpt, content: hasContent(state.view) } : undefined,
        menu: state.menu ?? undefined,
        typed: typed ? SEARCH_TEXT : undefined,
        poppedToRoot: state.poppedToRoot || undefined,
        toasts: state.toasts.slice(0, 8),
        oauth,
        error: error && { ...error, message: tidy(error.message).slice(0, 600) },
        exit,
        renders: state.renders,
        host: host.used,
        effects: host.effects,
        unknownRequests: host.unknown.length ? [...new Set(host.unknown)] : undefined,
        peakRssMb: peakRss,
        arguments: args,
        preferences: required.length ? required : undefined,
        stderr: tidy(stderr).slice(-1500) || undefined,
      });
    };

    // Looked at whenever the runtime has been silent for a moment.
    const settle = () => {
      const view = state.view;
      if (oauth) return finish("oauth");
      if (background) return state.done ? finish("done") : undefined;
      if (state.closed) return finish("closed");
      if (!view && state.poppedToRoot) return finish("popped");
      if (!view) {
        // A view command that failed before it showed anything.
        if (state.toasts.some((toast) => toast.style === "FAILURE") && command.mode === "view" && state.renders > 0) finish("failed");
        return;
      }
      if (view.isLoading) return;
      // An empty list that searches as you type is waiting to be typed in.
      if (!typed && !hasContent(view) && view.searchable && (view.root === "List" || view.root === "Grid")) {
        typed = true;
        say({ type: "dispatch", session: state.session, node: view.rootId, prop: "onSearchTextChange", args: [SEARCH_TEXT] });
        return bump();
      }
      finish("settled");
    };
    const bump = () => {
      clearTimeout(quiet);
      quiet = setTimeout(settle, quietMs);
    };

    const watch = setInterval(() => {
      peakRss = Math.max(peakRss, treeRss(child.pid));
      if (peakRss > memoryMb && !over) {
        over = true;
        finish("memory", { memory: `over ${memoryMb} MiB` });
      }
    }, 500);

    timers.push(setTimeout(() => !ready && finish("never-ready"), 10_000));
    timers.push(setTimeout(() => finish("deadline"), timeoutMs + 2_000));

    lines(child.stdout, (line) => {
      let message;
      try {
        message = JSON.parse(line);
      } catch {
        return;
      }
      if (!message || typeof message !== "object") return;
      if (message.type === "ready" && !ready) {
        ready = true;
        // The deadline counts from when the command starts, not the runtime.
        timers.push(setTimeout(() => finish("deadline"), timeoutMs));
        if (background) say({ type: "background-run", id: 1, extension: extensionDir, command: command.name, launchType: "userInitiated", action: null });
        else say({ type: "launch", extension: extensionDir, command: command.name, arguments: args });
        return bump();
      }
      if (message.type === "request") {
        say({ type: "reply", id: message.id, ...host.answer(message.method, message.params) });
        return bump();
      }
      observer.message(message);
      if (message.type === "toast" && /^Sign in to /.test(message.toast.title ?? "") && message.toast.style === "ANIMATED") {
        oauth = { provider: message.toast.title.replace(/^Sign in to /, "") };
      }
      bump();
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
      if (stderr.length > 200_000) stderr = stderr.slice(-100_000);
    });
    child.stdin.on("error", () => {});
    child.on("error", (e) => finish("exit", { error: String(e) }));
    // A moment after it is gone: what it wrote last has been read by then.
    child.on("exit", (code, signal) => setTimeout(() => finish("exit", { code, signal }), 200));
  });
}
