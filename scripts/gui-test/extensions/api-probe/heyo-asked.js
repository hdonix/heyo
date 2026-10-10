// The question itself. "memory.read" and "clipboard.history" are listed and
// have NOT been allowed beforehand: the palette must ask for each, naming
// this extension. The script answers Deny to the first and Allow to the
// second; asked again at once, each is as it was answered, with no further
// question.
const { ClipboardHistory, Memory } = require("@heyo/api");
const { probe } = require("./probe.js");

exports.default = () =>
  probe("asked", {
    denied: async () => (await Memory.search({ limit: 1 })).items.length,
    deniedAgain: async () => (await Memory.search({ limit: 1 })).items.length,
    allowed: async () => (await ClipboardHistory.search(undefined, { limit: 1 })).length,
    allowedAgain: async () => (await ClipboardHistory.search(undefined, { limit: 1 })).length,
  });
