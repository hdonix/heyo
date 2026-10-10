//! A stand-in for the Heyo host: answers the runtime's `host.*` requests
//! and what commands ask through `@heyo/api` (`ext.*`; both declared in
//! `src-tauri/src/wire.rs`, answered for real in `host.rs` and `ext.rs`)
//! with small, plausible, made-up data, and DOES nothing. A request that
//! would open, paste, trash, end or change something is written down and
//! answered as done.

/// The requests that would do something to the user's computer.
export const EFFECTS = new Set([
  "host.open",
  "host.trash",
  "host.paste",
  "host.clipboard.write",
  "ext.system.run",
  "ext.windows.activate",
  "ext.windows.close",
  "ext.windows.arrange",
  "ext.windows.setBounds",
  "host.launchCommand",
  "ext.system.useAudioDevice",
  "host.processes.end",
  "ext.clipboardHistory.copy",
  "host.clipboardHistory.chose",
  "host.clipboardHistory.pin",
  "host.clipboardHistory.delete",
  "host.clipboardHistory.clear",
  "host.clipboardHistory.pause",
]);

/// Applications as the host describes them (`host.rs`, `application`).
const application = (id, name) => ({ name, localizedName: name, path: `/usr/share/applications/${id}.desktop`, bundleId: `${id}.desktop`, icon: null });

export const APPLICATIONS = [
  application("firefox", "Firefox"),
  application("org.gnome.Nautilus", "Files"),
  application("org.gnome.Terminal", "Terminal"),
  application("org.gnome.TextEditor", "Text Editor"),
  application("code", "Visual Studio Code"),
];

const MONITORS = [{ x: 0, y: 0, width: 1920, height: 1080 }];
const WINDOWS = [
  { id: 4194311, title: "Example Domain — Mozilla Firefox", class: "firefox", active: true, bounds: { x: 0, y: 32, width: 1280, height: 1048 } },
  { id: 6291463, title: "notes.txt — Text Editor", class: "org.gnome.TextEditor", active: false, bounds: { x: 1280, y: 32, width: 640, height: 1048 } },
];

const short = (value, limit = 200) => {
  const text = typeof value === "string" ? value : JSON.stringify(value ?? null);
  return text.length > limit ? `${text.slice(0, limit)}…` : text;
};

/// A host for one launch. `home` is the made-up user's home directory;
/// the files it speaks of are there.
export function createHost({ home = "/home/user" } = {}) {
  let clipboard = "Heyo compatibility test";
  /// How often each method was asked.
  const used = {};
  /// What the command asked to have done, in order.
  const effects = [];
  /// Requests this stand-in has no answer for.
  const unknown = [];

  const answers = {
    "host.hide": () => null,
    "host.show": () => null,
    "host.reload": () => null,
    "host.hud": () => null,
    "host.applications": () => APPLICATIONS,
    "ext.frontmostApplication": () => APPLICATIONS[0],
    "host.defaultApplication": ({ target } = {}) => (/^https?:/.test(String(target)) ? "firefox.desktop" : "org.gnome.TextEditor.desktop"),
    "host.clipboard.read": () => clipboard,
    "host.clipboard.write": ({ text } = {}) => {
      // Kept, so that a command reading back what it copied sees it.
      clipboard = String(text ?? "");
      return null;
    },
    "host.selection": () => "selected text",
    // No model is configured: the commands that need one say so.
    "host.ai": () => ({ provider: "claude", apiKey: "", localUrl: "", localModel: "" }),
    "host.files.refresh": () => null,
    "ext.files.search": () => ({
      files: [
        { path: `${home}/Documents`, isDirectory: true },
        { path: `${home}/Documents/notes.txt`, isDirectory: false },
      ],
      scanning: false,
      total: 2,
    }),
    "ext.windows.list": () => WINDOWS,
    "ext.windows.active": () => WINDOWS[0],
    "ext.windows.monitors": () => MONITORS,
    "ext.system.audioDevices": () => [
      { name: "alsa_output.pci-0000_00_1f.3.analog-stereo", description: "Built-in Audio Analog Stereo", kind: "output", current: true },
      { name: "alsa_input.pci-0000_00_1f.3.analog-stereo", description: "Built-in Audio Analog Stereo", kind: "input", current: true },
    ],
    "host.processes.list": () => [
      { pid: 2101, cpu: 3.5, memory: 412_000_000, name: "firefox", command: "/usr/lib/firefox/firefox", ports: [] },
      { pid: 2733, cpu: 0.1, memory: 48_000_000, name: "node", command: "node server.js", ports: [3000] },
    ],
    "ext.memory.search": () => ({ items: [], understood: { terms: [] }, relaxed: false }),
    "host.clipboardHistory.status": () => ({ recording: true, paused: false }),
    // The alert a command raises before doing something: declined, so the
    // launch stays the least a command does.
    "ui.confirm": () => false,
  };

  /// The reply to a request: `{ result }`, or `{ error }` as the host
  /// refuses a request it does not know.
  function answer(method, params) {
    used[method] = (used[method] ?? 0) + 1;
    if (EFFECTS.has(method) && effects.length < 40) effects.push({ method, params: short(params) });
    if (method === "host.hud" && effects.length < 40) effects.push({ method, params: short(params?.text) });
    const known = answers[method];
    if (known) return { result: known(params ?? undefined) };
    if (EFFECTS.has(method)) return { result: null };
    unknown.push(method);
    return { error: `unknown request ${method}` };
  }

  return { answer, used, effects, unknown };
}
