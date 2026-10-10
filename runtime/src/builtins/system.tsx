//! Desktop commands built into Heyo: switching windows, system actions,
//! audio devices, processes, file search and browser bookmarks/history.
//!
//! Written against `@heyo/api` (`../heyo`), as an extension would be. What
//! is still asked of the host directly is Heyo's own to do: re-reading its
//! index of file names, and the list of processes (`host.rs`).

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { useEffect, useMemo, useRef, useState } from "react";

import { Color } from "../api";
import { Action, ActionPanel, List } from "../api/components";
import { Icon } from "../api/icon";
import { closeMainWindow, confirmAlert, getApplications, open, showHUD, showInFinder, showToast, Toast, trash, type Application } from "../api/system";
import type { ProcessInfo } from "../../../shared/bindings/ProcessInfo";
import { cacheRoot } from "../context";
import { Files, System, type AudioDevice, type FoundFiles, type SystemAction } from "../heyo/system";
import { Windows, type Window } from "../heyo/windows";
import { request } from "../protocol";

function fail(title: string) {
  return (e: unknown) => showToast({ style: Toast.Style.Failure, title, message: e instanceof Error ? e.message : String(e) });
}

// --- Switch Windows ----------------------------------------------------------

/// The icon of the application a window class belongs to, e.g. "firefox".
function iconFor(windowClass: string, apps: Application[]) {
  const wanted = windowClass.toLowerCase();
  const app =
    apps.find((a) => (a.bundleId ?? "").toLowerCase().replace(/\.desktop$/, "").split(".").pop() === wanted) ??
    apps.find((a) => a.name.toLowerCase() === wanted) ??
    apps.find((a) => (a.bundleId ?? "").toLowerCase().includes(wanted));
  return app?.icon ?? Icon.Window;
}

export function SwitchWindows() {
  const [windows, setWindows] = useState<Window[]>();
  const [apps, setApps] = useState<Application[]>([]);

  const reload = () =>
    Windows.list()
      // Most recently used first, which begins with the window the user came
      // from: that one goes last, so the first row is the one to switch back to.
      .then((all) => setWindows([...all.filter((w) => !w.active), ...all.filter((w) => w.active)]))
      .catch((e) => {
        setWindows([]);
        fail("Could not list windows")(e);
      });
  useEffect(() => {
    reload();
    getApplications().then(setApps);
  }, []);

  return (
    <List isLoading={windows === undefined} searchBarPlaceholder="Search open windows…">
      {windows && <List.EmptyView icon={Icon.Window} title="No open windows" />}
      {(windows ?? []).map((w) => (
        <List.Item
          key={w.id}
          title={w.title}
          subtitle={w.class}
          icon={iconFor(w.class, apps)}
          keywords={[w.class]}
          accessories={w.active ? [{ tag: "Current" }] : undefined}
          actions={
            <ActionPanel>
              <Action title="Switch to Window" icon={Icon.Window} onAction={() => Windows.activate(w.id).catch(fail("Could not switch"))} />
              <Action
                title="Close Window"
                icon={Icon.XMarkCircle}
                style="destructive"
                shortcut={{ modifiers: ["ctrl"], key: "x" }}
                onAction={async () => {
                  await Windows.close(w.id).catch(fail("Could not close the window"));
                  setTimeout(reload, 300);
                }}
              />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}

// --- System commands (no view) -----------------------------------------------

async function confirmed(title: string, message: string, action: string) {
  return confirmAlert({ title, message, primaryAction: { title: action, style: "DESTRUCTIVE" } });
}

/// Has it done, and shows what there is to say of it ("Volume 40%"). What
/// ends the session is confirmed here, in this command's own words: Heyo
/// does not ask its own commands a second time, as it would an extension.
async function system(action: SystemAction) {
  const said = await System.run(action);
  if (said) await showHUD(said);
}

export const systemCommands: Record<SystemAction, () => Promise<void>> = {
  "lock-screen": async () => {
    await closeMainWindow();
    await system("lock-screen");
  },
  sleep: async () => {
    await closeMainWindow();
    await system("sleep");
  },
  restart: async () => {
    if (await confirmed("Restart the computer?", "Unsaved work in open applications may be lost.", "Restart")) await system("restart");
  },
  "shut-down": async () => {
    if (await confirmed("Shut down the computer?", "Unsaved work in open applications may be lost.", "Shut Down")) await system("shut-down");
  },
  "log-out": async () => {
    if (await confirmed("Log out?", "All your applications will be closed.", "Log Out")) await system("log-out");
  },
  "empty-trash": async () => {
    if (await confirmed("Empty the trash?", "The files in it are deleted permanently.", "Empty Trash")) await system("empty-trash");
  },
  "toggle-dark-mode": () => system("toggle-dark-mode"),
  "volume-up": () => system("volume-up"),
  "volume-down": () => system("volume-down"),
  "toggle-mute": () => system("toggle-mute"),
};

// --- Audio devices -----------------------------------------------------------

export function AudioDevices() {
  const [devices, setDevices] = useState<AudioDevice[]>();

  const reload = () =>
    System.audioDevices()
      .then(setDevices)
      .catch((e) => {
        setDevices([]);
        fail("Could not list audio devices")(e);
      });
  useEffect(() => {
    reload();
  }, []);

  const use = async (device: AudioDevice) => {
    try {
      await System.useAudioDevice(device.kind, device.name);
    } catch (e) {
      return fail("Could not switch device")(e);
    }
    await reload();
    await showToast({ title: `${device.kind === "output" ? "Output" : "Input"}: ${device.description}` });
  };

  const section = (kind: AudioDevice["kind"], title: string, icon: string) => (
    <List.Section title={title}>
      {(devices ?? [])
        .filter((d) => d.kind === kind)
        .map((d) => (
          <List.Item
            key={d.name}
            title={d.description}
            icon={icon}
            keywords={[d.name]}
            accessories={d.current ? [{ tag: { value: "Current", color: Color.Green } }] : undefined}
            actions={
              <ActionPanel>
                <Action title={`Use as ${title}`} icon={icon} onAction={() => use(d)} />
              </ActionPanel>
            }
          />
        ))}
    </List.Section>
  );

  return (
    <List isLoading={devices === undefined} searchBarPlaceholder="Search audio devices…">
      <List.EmptyView icon={Icon.SpeakerOn} title="No audio devices found" />
      {section("output", "Output", Icon.SpeakerOn)}
      {section("input", "Input", Icon.Microphone)}
    </List>
  );
}

// --- Processes ---------------------------------------------------------------

const megabytes = (bytes: number) => (bytes >= 1 << 30 ? `${(bytes / (1 << 30)).toFixed(1)} GB` : `${Math.round(bytes / (1 << 20))} MB`);

export function KillProcess() {
  const [list, setList] = useState<ProcessInfo[]>();
  const reload = () =>
    request<ProcessInfo[]>("host.processes.list")
      // Ending the runtime would end this command with it.
      .then((all) => setList(all.filter((p) => p.pid !== process.pid)))
      .catch((e) => (setList([]), fail("Could not list processes")(e)));
  useEffect(() => {
    reload();
  }, []);

  const end = async (p: ProcessInfo, force: boolean) => {
    try {
      await request("host.processes.end", { pid: p.pid, force });
      await showToast({ title: `${force ? "Killed" : "Quit"} ${p.name}`, message: `PID ${p.pid}` });
      setTimeout(reload, 400);
    } catch (e) {
      fail(`Could not stop ${p.name}`)(e);
    }
  };

  return (
    <List isLoading={list === undefined} searchBarPlaceholder="Search by name, command or port (e.g. 3000)…">
      {(list ?? []).map((p) => (
        <List.Item
          key={p.pid}
          title={p.name}
          subtitle={p.command.length > 70 ? `${p.command.slice(0, 70)}…` : p.command}
          icon={Icon.Terminal}
          keywords={[String(p.pid), p.command, ...p.ports.map((port) => [String(port), `:${port}`]).flat()]}
          accessories={[
            ...p.ports.map((port) => ({ tag: { value: `:${port}`, color: Color.Blue } })),
            { text: `${p.cpu.toFixed(1)}%` },
            { text: megabytes(p.memory) },
          ]}
          actions={
            <ActionPanel>
              <Action title="Quit Process" icon={Icon.XMarkCircle} onAction={() => end(p, false)} />
              <Action title="Force Kill" icon={Icon.Bolt} style="destructive" onAction={() => end(p, true)} />
              <Action.CopyToClipboard title="Copy PID" content={String(p.pid)} shortcut={{ modifiers: ["cmd", "shift"], key: "c" }} />
              <Action title="Refresh" icon={Icon.ArrowClockwise} shortcut={{ modifiers: ["cmd"], key: "r" }} onAction={reload} />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}

// --- File search -------------------------------------------------------------

export function SearchFiles() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<FoundFiles>({ files: [], scanning: true, total: 0 });
  const latest = useRef(0);

  // The host keeps the file names in memory; opening the command re-reads
  // them. When that happens is Heyo's to say, not an extension's.
  useEffect(() => {
    request("host.files.refresh").catch(fail("Could not read your files"));
  }, []);

  useEffect(() => {
    const id = ++latest.current;
    let timer: NodeJS.Timeout | undefined;
    const search = () =>
      Files.search(query)
        .then((r) => {
          if (id !== latest.current) return;
          setResults(r);
          // Ask again until the scan in progress has finished.
          if (r.scanning) timer = setTimeout(search, 300);
        })
        .catch(fail("Could not search files"));
    search();
    return () => clearTimeout(timer);
  }, [query]);

  const home = os.homedir();
  const empty = !query.trim()
    ? { title: "Type to search your files", description: results.total ? `${results.total.toLocaleString()} files and folders under your home folder. Hidden folders and node_modules are skipped.` : "Reading file names…" }
    : results.scanning && !results.total
      ? { title: "Reading file names…", description: undefined }
      : { title: "No files found", description: undefined };
  return (
    <List isLoading={results.scanning} filtering={false} onSearchTextChange={setQuery} searchBarPlaceholder="Search files by name…">
      <List.EmptyView icon={Icon.MagnifyingGlass} title={empty.title} description={empty.description} />
      {results.files.map(({ path: file, isDirectory }) => (
        <List.Item
          key={file}
          title={path.basename(file)}
          subtitle={path.dirname(file).replace(home, "~")}
          icon={isDirectory ? Icon.Folder : Icon.Document}
          actions={
            <ActionPanel>
              <Action.Open title="Open" target={file} />
              <Action.ShowInFinder path={file} shortcut={{ modifiers: ["cmd"], key: "return" }} />
              <Action.CopyToClipboard title="Copy Path" content={file} shortcut={{ modifiers: ["cmd", "shift"], key: "c" }} />
              <Action
                title="Move to Trash"
                icon={Icon.Trash}
                style="destructive"
                shortcut={{ modifiers: ["ctrl"], key: "x" }}
                onAction={async () => {
                  await trash(file).catch(fail("Could not move to trash"));
                  setResults((r) => ({ ...r, files: r.files.filter((f) => f.path !== file) }));
                }}
              />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}

// --- Browser bookmarks and history -------------------------------------------

interface Page {
  title: string;
  url: string;
  browser: string;
  /// Folder for bookmarks, visit count for history.
  detail?: string;
}

/// Profile directories of installed browsers.
function profiles() {
  const home = os.homedir();
  const firefoxRoots = [".mozilla/firefox", "snap/firefox/common/.mozilla/firefox", ".var/app/org.mozilla.firefox/.mozilla/firefox"];
  const chromeRoots: [string, string][] = [
    ["Chrome", ".config/google-chrome"],
    ["Chromium", ".config/chromium"],
    ["Chromium", "snap/chromium/common/chromium"],
    ["Brave", ".config/BraveSoftware/Brave-Browser"],
    ["Edge", ".config/microsoft-edge"],
    ["Vivaldi", ".config/vivaldi"],
  ];
  const dirs = (root: string) => {
    try {
      return fs.readdirSync(root, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => path.join(root, d.name));
    } catch {
      return [];
    }
  };
  // The snap and classic Firefox locations can be the same profile.
  const seen = new Set<string>();
  const unique = (dir: string) => {
    const real = fs.realpathSync(dir);
    return !seen.has(real) && seen.add(real);
  };
  return {
    firefox: firefoxRoots.flatMap((r) => dirs(path.join(home, r))).filter((d) => fs.existsSync(path.join(d, "places.sqlite")) && unique(d)),
    chrome: chromeRoots.flatMap(([browser, root]) =>
      dirs(path.join(home, root))
        .filter((d) => fs.existsSync(path.join(d, "Bookmarks")) || fs.existsSync(path.join(d, "History")))
        .filter(unique)
        .map((dir) => ({ browser, dir })),
    ),
  };
}

/// Runs a query on a copy of a browser database (the original is locked
/// while the browser runs).
function query<T>(database: string, sql: string): T[] {
  // One unreadable profile must not hide every other browser's results.
  try {
    return queryCopy<T>(database, sql);
  } catch (e) {
    console.error(`heyo: could not read ${database}:`, e);
    return [];
  }
}

function queryCopy<T>(database: string, sql: string): T[] {
  const { DatabaseSync } = require("node:sqlite") as typeof import("node:sqlite");
  // The copy holds browsing history: a private directory in the user's cache,
  // not a predictable name in /tmp that other accounts could read.
  const cache = cacheRoot;
  fs.mkdirSync(cache, { recursive: true, mode: 0o700 });
  const directory = fs.mkdtempSync(path.join(cache, "browser-"));
  const copy = path.join(directory, path.basename(database));
  try {
    fs.copyFileSync(database, copy);
    // Recent changes live in the write-ahead log.
    if (fs.existsSync(`${database}-wal`)) fs.copyFileSync(`${database}-wal`, `${copy}-wal`);
    const db = new DatabaseSync(copy, { readOnly: false });
    try {
      return db.prepare(sql).all() as T[];
    } finally {
      db.close();
    }
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

function bookmarks(): Page[] {
  const { firefox, chrome } = profiles();
  const pages: Page[] = [];
  for (const dir of firefox) {
    const rows = query<{ title: string | null; url: string; folder: string | null }>(
      path.join(dir, "places.sqlite"),
      `SELECT b.title, p.url, f.title AS folder FROM moz_bookmarks b
       JOIN moz_places p ON p.id = b.fk LEFT JOIN moz_bookmarks f ON f.id = b.parent
       WHERE b.type = 1 AND p.url NOT LIKE 'place:%' ORDER BY b.dateAdded DESC`,
    );
    pages.push(...rows.map((r) => ({ title: r.title || r.url, url: r.url, browser: "Firefox", detail: r.folder ?? undefined })));
  }
  for (const { browser, dir } of chrome) {
    const file = path.join(dir, "Bookmarks");
    if (!fs.existsSync(file)) continue;
    const walk = (node: any, folder: string) => {
      if (node.type === "url") pages.push({ title: node.name || node.url, url: node.url, browser, detail: folder });
      for (const child of node.children ?? []) walk(child, node.name || folder);
    };
    for (const root of Object.values(JSON.parse(fs.readFileSync(file, "utf8")).roots ?? {})) walk(root, "");
  }
  return pages;
}

function history(): Page[] {
  const { firefox, chrome } = profiles();
  const pages: Page[] = [];
  for (const dir of firefox) {
    const rows = query<{ title: string | null; url: string; visit_count: number }>(
      path.join(dir, "places.sqlite"),
      `SELECT title, url, visit_count FROM moz_places WHERE visit_count > 0 AND hidden = 0
       ORDER BY last_visit_date DESC LIMIT 3000`,
    );
    pages.push(...rows.map((r) => ({ title: r.title || r.url, url: r.url, browser: "Firefox", detail: `${r.visit_count}×` })));
  }
  for (const { browser, dir } of chrome) {
    const file = path.join(dir, "History");
    if (!fs.existsSync(file)) continue;
    const rows = query<{ title: string; url: string; visit_count: number }>(
      file,
      "SELECT title, url, visit_count FROM urls WHERE hidden = 0 ORDER BY last_visit_time DESC LIMIT 3000",
    );
    pages.push(...rows.map((r) => ({ title: r.title || r.url, url: r.url, browser, detail: `${r.visit_count}×` })));
  }
  return pages;
}

function host(url: string) {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function PageList(props: { load(): Page[]; placeholder: string; empty: string }) {
  const [pages, setPages] = useState<Page[]>();
  const [query, setQuery] = useState("");
  const started = useRef(false);

  useEffect(() => {
    if (started.current) return;
    started.current = true;
    // Reading the databases blocks; let the empty list render first.
    setImmediate(() => {
      try {
        // The same page shows up once per profile, folder or query string.
        const seen = new Set<string>();
        const key = (p: Page) => `${p.title}\n${host(p.url)}`;
        setPages(props.load().filter((p) => !seen.has(key(p)) && seen.add(key(p))));
      } catch (e) {
        setPages([]);
        fail("Could not read browser data")(e);
      }
    });
  }, []);

  // Thousands of pages: filter here and send only the first matches.
  const visible = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    const all = pages ?? [];
    const matched = words.length ? all.filter((p) => words.every((w) => p.title.toLowerCase().includes(w) || p.url.toLowerCase().includes(w))) : all;
    return matched.slice(0, 100);
  }, [pages, query]);

  return (
    <List isLoading={pages === undefined} filtering={false} onSearchTextChange={setQuery} searchBarPlaceholder={props.placeholder}>
      <List.EmptyView icon={Icon.Globe} title={pages?.length ? "No matches" : props.empty} />
      {visible.map((p, i) => (
        <List.Item
          key={`${p.url}-${i}`}
          title={p.title}
          subtitle={host(p.url)}
          icon={Icon.Globe}
          accessories={[...(p.detail ? [{ text: p.detail }] : []), { tag: p.browser }]}
          actions={
            <ActionPanel>
              <Action
                title="Open in Browser"
                icon={Icon.Globe}
                onAction={async () => {
                  await open(p.url);
                  await closeMainWindow();
                }}
              />
              <Action.CopyToClipboard title="Copy Link" content={p.url} shortcut={{ modifiers: ["cmd", "shift"], key: "c" }} />
            </ActionPanel>
          }
        />
      ))}
    </List>
  );
}

export function SearchBookmarks() {
  return <PageList load={bookmarks} placeholder="Search browser bookmarks…" empty="No bookmarks found" />;
}

export function SearchHistory() {
  return <PageList load={history} placeholder="Search browser history…" empty="No browsing history found" />;
}

export { showInFinder };
