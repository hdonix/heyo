// `Screen` without a selection (permission "screen.capture", allowed
// beforehand): the monitor, a rectangle, a window; then the text in one of
// them ("screen.text" needs no asking; "unavailable" until the engine is
// downloaded).
const fs = require("node:fs");
const { Screen, Windows } = require("@heyo/api");
const { probe } = require("./probe.js");

// The file is there and is a PNG; the command's cache holds it.
const seen = (capture) => ({
  ...capture,
  png: fs.readFileSync(capture.path).subarray(1, 4).toString() === "PNG",
  bytes: fs.statSync(capture.path).size,
  mode: (fs.statSync(capture.path).mode & 0o777).toString(8),
});

exports.default = () =>
  probe("screen", {
    screen: async () => seen(await Screen.capture("screen")),
    rect: async () => seen(await Screen.capture({ rect: { x: 100, y: 100, width: 320, height: 200 } })),
    window: async () => seen(await Screen.capture({ window: (await Windows.active()).id })),
    offScreen: () => Screen.capture({ rect: { x: 90000, y: 0, width: 10, height: 10 } }),
    noSuchWindow: () => Screen.capture({ window: 1 }),
    text: (out) => Screen.recognizeText(out.window.path),
    textOfNothing: () => Screen.recognizeText("/nonexistent.png"),
  });
