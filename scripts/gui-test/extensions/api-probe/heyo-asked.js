// The question itself. "memory.read" and "clipboard.history" are listed and
// have NOT been allowed beforehand: the palette must ask for each, naming
// this extension. The script answers Deny to the first and Allow to the
// second; asked again at once, each is as it was answered, with no further
// question.
const { ClipboardHistory, Memory } = require("@heyo/api");
const { probe } = require("./probe.js");

// Allowed is not refused: the entry named may be gone ("invalid"), which
// is an answer about the entry, not about the extension's leave.
const allowed = () => ClipboardHistory.copy("clipboard:1").then(
  () => 1,
  (e) => (e.code === "invalid" ? 1 : Promise.reject(e)),
);

exports.default = () =>
  probe("asked", {
    denied: async () => (await Memory.search({ limit: 1 })).items.length,
    deniedAgain: async () => (await Memory.search({ limit: 1 })).items.length,
    allowed,
    allowedAgain: allowed,
  });
