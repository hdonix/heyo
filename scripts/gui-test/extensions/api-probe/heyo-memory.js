// `ClipboardHistory` and `Memory` (permissions "clipboard.history" and
// "memory.read", allowed beforehand). The script copies two things first.
const { ClipboardHistory, Memory } = require("@heyo/api");
const { probe } = require("./probe.js");

const brief = (item) => ({ id: item.id, source: item.source, kind: item.kind, text: item.text, pinned: item.pinned, at: typeof item.at });

exports.default = () =>
  probe("memory", {
    recent: async () => (await ClipboardHistory.search()).map(brief),
    found: async () => (await ClipboardHistory.search("probe", { limit: 1 })).map(brief),
    links: async () => (await ClipboardHistory.search(undefined, { kind: "link" })).map(brief),
    memory: async () => {
      const { items, understood, relaxed } = await Memory.search({ text: "probe", limit: 5 });
      return { items: items.map(brief), understood, relaxed };
    },
    byKind: async () => (await Memory.search({ kinds: ["link"] })).items.map(brief),
    // Nothing was copied before the epoch's second day.
    longAgo: async () => (await Memory.search({ until: new Date(86400000) })).items.length,
    otherSource: async () => (await Memory.search({ sources: ["file"] })).items.length,
    one: async (out) => brief(await Memory.get(out.recent[0].id)),
    none: () => Memory.get("clipboard:99999999"),
    badId: () => Memory.get("nonsense"),
    copied: (out) => ClipboardHistory.copy(out.recent[out.recent.length - 1].id),
  });
