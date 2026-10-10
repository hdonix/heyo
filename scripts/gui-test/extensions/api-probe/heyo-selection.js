// `Screen.capture("selection")`: the overlay comes up without tools, the
// area dragged over comes back as a PNG, and nothing lands on the clipboard.
// The script drags; run again and press Escape instead for "cancelled".
const fs = require("node:fs");
const { Clipboard, Screen } = require("@heyo/api");
const { probe } = require("./probe.js");

exports.default = () =>
  probe("selection", {
    clipboardBefore: () => Clipboard.readText(),
    capture: async () => {
      const capture = await Screen.capture("selection");
      return { ...capture, png: fs.readFileSync(capture.path).subarray(1, 4).toString() === "PNG" };
    },
    clipboardAfter: () => Clipboard.readText(),
  });
