//! Reads what the runtime says while a command runs (the messages of
//! `shared/wire.ts`) and keeps what a person would have seen: the view on
//! top, the toasts, whether the command ended. Nothing here starts or
//! stops anything; `drive.mjs` feeds it.

const FIELDS = /^Form\.(TextField|PasswordField|TextArea|Checkbox|DatePicker|Dropdown|TagPicker|FilePicker)$/;

const clip = (value, limit = 80) => {
  if (value && typeof value === "object") value = value.value ?? value.text ?? "";
  const text = String(value ?? "").replace(/\s+/g, " ").trim();
  return text.length > limit ? `${text.slice(0, limit)}…` : text;
};

/// What is in a view: the nodes of the view on top, as the webview would
/// hold them after every `render` so far.
export function summarizeView(view, nodes) {
  // A command without a view still has an (empty) place in the navigation.
  if (!view || !nodes.get(view.root?.[0])) return null;
  const counts = {};
  const titles = [];
  let empty;
  const seen = new Set();
  const visit = (id) => {
    const node = nodes.get(id);
    if (!node || seen.has(id)) return;
    seen.add(id);
    counts[node.type] = (counts[node.type] ?? 0) + 1;
    if ((node.type === "List.Item" || node.type === "Grid.Item") && titles.length < 5) titles.push(clip(node.props.title));
    if (node.type === "List.EmptyView" || node.type === "Grid.EmptyView") empty = clip(node.props.title);
    node.children.forEach(visit);
  };
  view.root.forEach(visit);
  const root = nodes.get(view.root[0]);
  const sum = (test) => Object.entries(counts).reduce((n, [type, count]) => (test(type) ? n + count : n), 0);
  const markdown = typeof root?.props.markdown === "string" ? root.props.markdown : "";
  return {
    root: root?.type ?? null,
    rootId: root?.id,
    depth: view.depth,
    isLoading: root?.props.isLoading === true,
    items: sum((type) => type === "List.Item" || type === "Grid.Item"),
    sections: sum((type) => type === "List.Section" || type === "Grid.Section"),
    fields: sum((type) => FIELDS.test(type)),
    actions: sum((type) => type === "Action" || type.startsWith("Action.")),
    markdown: markdown.length,
    excerpt: markdown ? clip(markdown, 160) : undefined,
    // What the runtime puts in place of a view that threw while rendering
    // (`ErrorBoundary` in api/navigation.tsx): the error and its stack.
    errorScreen: root?.type === "Detail" ? /^## Error\n\n```\n([\s\S]*)\n```$/.exec(markdown)?.[1] : undefined,
    emptyView: empty,
    titles,
    searchable: root?.props.onSearchTextChange?.$fn === 1,
    navigationTitle: typeof root?.props.navigationTitle === "string" ? clip(root.props.navigationTitle) : undefined,
  };
}

/// Whether a view shows anything of the command's own.
export function hasContent(view) {
  if (!view || view.errorScreen) return false;
  if (view.root === "Detail") return view.markdown > 0;
  if (view.root === "Form") return view.fields > 0;
  return view.items > 0;
}

function countMenu(items) {
  let count = 0;
  for (const item of items ?? []) {
    if (item.kind === "item") count++;
    count += countMenu(item.children);
  }
  return count;
}

export function createObserver() {
  const state = {
    launched: false,
    session: undefined,
    /// The command ended by itself.
    closed: false,
    /// A menu bar or background run reported back.
    done: false,
    doneError: undefined,
    renders: 0,
    view: null,
    menu: undefined,
    /// Every toast shown, as it last was.
    toasts: [],
    poppedToRoot: false,
  };
  let nodes = new Map();
  let top = null;
  const toastsById = new Map();

  function message(m) {
    switch (m.type) {
      case "launched":
        state.launched = true;
        state.session = m.session;
        break;
      case "render": {
        state.renders++;
        if (!m.view) {
          nodes = new Map();
          top = null;
          state.view = null;
          break;
        }
        if (m.view.full) nodes = new Map();
        for (const id of m.view.removed ?? []) nodes.delete(id);
        for (const node of m.view.nodes ?? []) nodes.set(node.id, node);
        top = m.view;
        state.view = summarizeView(top, nodes);
        break;
      }
      case "toast": {
        const toast = { style: m.toast.style, title: clip(m.toast.title, 200), message: m.toast.message ? clip(m.toast.message, 400) : undefined };
        if (!toastsById.has(m.toast.id)) state.toasts.push(toast);
        else state.toasts[state.toasts.indexOf(toastsById.get(m.toast.id))] = toast;
        toastsById.set(m.toast.id, toast);
        break;
      }
      case "closed":
        state.closed = true;
        break;
      case "pop-to-root":
        state.poppedToRoot = true;
        break;
      case "menu":
        state.menu = m.menu ? { title: m.menu.title ? clip(m.menu.title) : undefined, items: countMenu(m.menu.items) } : null;
        break;
      case "background-done":
        state.done = true;
        state.doneError = m.error ?? undefined;
        break;
    }
  }

  return { state, message };
}

/// Where an error came from, by the first lines of its stack as Node
/// prints it. `extensionDir` and `runtimeFile` are the paths the frames
/// are told apart by.
export function readStack(stderr, text, { extensionDir, runtimeFile }) {
  const lines = stderr.split("\n");
  // The error as `console.error` wrote it: its message, then its frames.
  const first = text ? text.split("\n")[0].trim() : "";
  let at = first ? lines.findIndex((line) => line.includes(first)) : -1;
  if (at === -1) at = lines.findIndex((line) => /^\s+at /.test(line)) - 1;
  if (at < 0) return undefined;
  const frames = [];
  for (let i = at + 1; i < lines.length && frames.length < 30; i++) {
    if (/^\s+at /.test(lines[i])) frames.push(lines[i].trim());
    else if (frames.length > 0) break;
  }
  if (frames.length === 0) return undefined;
  const place = (frame) => {
    if (extensionDir && frame.includes(extensionDir)) return "extension";
    if (runtimeFile && frame.includes(runtimeFile)) return "runtime";
    if (/\(node:|at node:/.test(frame)) return "node";
    return "other";
  };
  const short = (frame) => (extensionDir ? frame.split(`${extensionDir}/`).join("") : frame).replace(/^at /, "");
  const inExtension = frames.find((frame) => place(frame) === "extension");
  return {
    /// Whose code the error was raised in.
    raisedIn: place(frames[0]),
    top: short(frames[0]),
    /// The first frame inside the extension, if any.
    frame: inExtension ? short(inExtension) : undefined,
  };
}
