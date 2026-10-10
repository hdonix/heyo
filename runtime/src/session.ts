//! Runs one command at a time and streams its UI to the webview.

import Module from "node:module";
import path from "node:path";
import { createElement as h, useEffect, useState, type ComponentType } from "react";

import { Navigator, type Navigation } from "./api/navigation";
import { showToast, Toast } from "./api/system";
import { Detail } from "./api/components";
import { builtinExtension, builtins } from "./builtins";
import { ArgumentsForm, PreferencesForm } from "./builtins/setup";
import { asCommand, current, ended, foreground, missingPreferences, ownsPalette, readManifest, runAs, scoped, type CommandManifest, type Current, type Extension } from "./context";
import { pendingFixup } from "./compat";
import { send } from "./protocol";
import type { WireNode } from "../../shared/wire";
import { encodeProps, mount, visibleChildren, type Container, type HostNode } from "./renderer";

export interface LaunchOptions {
  arguments?: Record<string, string>;
  launchContext?: unknown;
  fallbackText?: string;
}

interface Session {
  id: number;
  context: Current;
  /// What it was launched with, for launching it again (`relaunch`).
  options: LaunchOptions;
  navigation?: Navigation;
  unmount?: () => void;
}

let session: Session | null = null;
let nextSession = 0;

export function activeSession() {
  return session;
}

export function reportError(error: unknown) {
  console.error(error);
  const message = error instanceof Error ? error.message : String(error);
  showToast({ style: Toast.Style.Failure, title: "Error", message });
}

/// Makes `modules` what extensions get when they ask for those names: built
/// extensions leave them external and expect the host to provide them.
export function provide(modules: Record<string, unknown>) {
  const load = (Module as any)._load;
  (Module as any)._load = function (request: string, ...rest: unknown[]) {
    if (Object.hasOwn(modules, request)) return modules[request];
    return load.call(this, request, ...rest);
  };
}

/// Evaluates a command's file anew, so its module-level state starts fresh
/// on every launch. The module is kept out of Node's module cache and has no
/// parent: nothing but the command holds on to it, and it is freed with it.
/// (`delete require.cache[file]` and `require` left every evaluation
/// reachable from the requiring module's `children` for the life of the
/// process.)
export function loadModule(file: string): any {
  const module = new Module(file) as Module & { load(file: string): void };
  module.load(file);
  return module.exports;
}

export function loadCommand(extension: Extension, name: string): unknown {
  if (extension === builtinExtension) return (builtins as Record<string, unknown>)[name];
  const module = loadModule(path.join(extension.dir, `${name}.js`));
  return module.default ?? module;
}

function ViewCommand(props: { extension: Extension; command: CommandManifest; context: Current; launchProps: object }) {
  // As the command: what its module starts while loading is its own.
  const [Command] = useState(() => {
    const load = () => loadCommand(props.extension, props.command.name) as ComponentType<object>;
    return asCommand(props.context, runAs(props.context, load));
  });
  return h(Command, props.launchProps);
}

function NoViewCommand(props: { extension: Extension; command: CommandManifest; context: Current; launchProps: object; onDone(): void }) {
  useEffect(() => {
    runAs(props.context, async () => {
      const run = loadCommand(props.extension, props.command.name) as (props: object) => unknown;
      await run(props.launchProps);
    })
      .catch(reportError)
      .finally(props.onDone);
  }, []);
  return null;
}

/// Asks for missing required preferences and for arguments, then runs.
function Launch(props: { extension: Extension; command: CommandManifest; context: Current; options: LaunchOptions; onDone(): void }) {
  const { extension, command, context, options } = props;
  const needsArguments = !!command.arguments?.length && !options.arguments;
  const afterFixup = () => (missingPreferences(extension, command.name).length ? "preferences" : needsArguments ? "arguments" : "run");
  // Something the extension needs put right before it can run on Linux.
  const [fixup] = useState(() => (extension === builtinExtension ? undefined : pendingFixup(extension)));
  const [phase, setPhase] = useState(() => (fixup ? "fixup" : afterFixup()));
  const [fixupError, setFixupError] = useState<string>();
  const [args, setArgs] = useState(options.arguments ?? {});

  useEffect(() => {
    fixup
      ?.apply()
      .then(() => setPhase(afterFixup()))
      .catch((e) => setFixupError(e instanceof Error ? e.message : String(e)));
  }, []);

  if (phase === "fixup") {
    return h(Detail, {
      isLoading: !fixupError,
      markdown: fixupError ? `**Could not prepare ${extension.manifest.title} for Linux**\n\n${fixupError}` : fixup?.title,
    });
  }

  if (phase === "preferences") {
    return h(PreferencesForm, {
      extension,
      command: command.name,
      onSaved: () => setPhase(needsArguments ? "arguments" : "run"),
    });
  }
  if (phase === "arguments") {
    return h(ArgumentsForm, {
      command,
      onSubmit: (values: Record<string, string>) => {
        setArgs(values);
        setPhase("run");
      },
    });
  }
  const launchProps = {
    arguments: args,
    launchType: "userInitiated",
    launchContext: options.launchContext,
    fallbackText: options.fallbackText,
  };
  return command.mode === "no-view"
    ? h(NoViewCommand, { extension, command, context, launchProps, onDone: props.onDone })
    : h(ViewCommand, { extension, command, context, launchProps });
}

export function launch(extension: Extension, name: string, options: LaunchOptions = {}) {
  // Before closing anything: a launch that cannot be leaves things as they are.
  const command = extension.manifest.commands.find((c) => c.name === name);
  if (!command) throw new Error(`Unknown command ${extension.manifest.name}/${name}`);
  if (command.mode === "menu-bar") throw new Error("Menu bar commands run in the menu bar, not in the palette");
  close();

  const id = ++nextSession;
  const context: Current = { extension, command: name, launchType: "userInitiated", user: true };
  const self: Session = { id, context, options };
  session = self;
  foreground.value = context;

  let flushQueued = false;
  // What the webview already has: per node, its encoded props and child ids.
  let sentKey: number | undefined;
  let sent = new Map<number, string>();

  // Sends the top view. After the first render of a view only changed
  // nodes are sent, so selection changes in large lists stay cheap.
  const flush = () => {
    flushQueued = false;
    if (session !== self) return;
    const views = container.children.filter((c) => c.type === "View");
    const top = views.at(-1);
    if (!top) {
      sentKey = undefined;
      sent = new Map();
      return send({ type: "render", session: id, view: null });
    }
    const full = top.props.viewKey !== sentKey;
    const next = new Map<number, string>();
    const nodes: WireNode[] = [];
    const visit = (node: HostNode) => {
      const props = encodeProps(node);
      const children = visibleChildren(node);
      const ids = children.map((c) => c.id);
      const signature = JSON.stringify([props, ids]);
      next.set(node.id, signature);
      if (full || sent.get(node.id) !== signature) nodes.push({ id: node.id, type: node.type, props, children: ids });
      children.forEach(visit);
    };
    const roots = visibleChildren(top);
    roots.forEach(visit);
    const removed = full ? [] : [...sent.keys()].filter((n) => !next.has(n));
    sent = next;
    sentKey = top.props.viewKey;
    send({
      type: "render",
      session: id,
      view: { key: top.props.viewKey, depth: views.length, full, root: roots.map((c) => c.id), nodes, removed },
    });
  };

  const container: Container = {
    children: [],
    onCommit() {
      if (flushQueued) return;
      flushQueued = true;
      // Coalesce the commits of one tick into a single message.
      setImmediate(flush);
    },
  };

  send({
    type: "launched",
    session: id,
    fallbackText: options.fallbackText,
    mode: command.mode,
    title: command.title,
    extension: {
      name: extension.manifest.name,
      title: extension.manifest.title,
      dir: extension.dir,
      // Built-in commands use Raycast icon names; extensions use asset files.
      icon:
        extension === builtinExtension
          ? command.icon
          : extension.manifest.icon && path.join(extension.dir, "assets", extension.manifest.icon),
    },
  });

  // A command that finishes late must not close the one opened after it.
  const closeSelf = () => {
    if (session === self) close(true);
  };
  const root = h(Navigator, {
    root: h(Launch, { extension, command, context, options, onDone: closeSelf }),
    onPopRoot: closeSelf,
    expose: (navigation: Navigation) => {
      self.navigation = navigation;
    },
  });
  self.unmount = runAs(context, () => mount(scoped(context, root), container, reportError));
}

/// Starts the command in the palette again from its files, if it is one of
/// the extension in `dir`: `heyo dev` built that extension anew.
export function relaunch(dir: string) {
  const running = session;
  if (!running || running.context.extension.dir !== dir) return;
  try {
    launch({ dir, manifest: readManifest(dir) }, running.context.command, running.options);
  } catch (e) {
    // The command is no longer in the manifest, or the extension is gone.
    close(true);
    reportError(e);
  }
}

/// Unmounts the running command. `notify`: the command ended by itself and
/// the webview is told; otherwise the user left it, and whatever it still
/// does no longer has the palette.
export function close(notify = false) {
  const s = session;
  if (!s) return;
  session = null;
  if (!notify && foreground.value === s.context) foreground.value = null;
  s.unmount?.();
  ended(s.context);
  if (notify) send({ type: "closed", session: s.id });
}

export function openPreferences() {
  // Not into the views of a command that came after the caller.
  if (!ownsPalette()) return;
  const c = current.value;
  const navigation = session?.navigation;
  if (!c || !navigation) return;
  navigation.push(
    h(PreferencesForm, { extension: c.extension, command: c.command, onSaved: () => navigation.pop() }),
  );
}
