// An extension that lists no permissions. Heyo's own part of the API is
// refused to it without a question, each refusal saying what to add to the
// manifest. Raycast's window functions are not refused: the palette asks
// (the script answers Allow), as it would for any extension from the store.
const fs = require("node:fs");
const path = require("node:path");
const { Files, Windows, Screen, AI, getFrontmostApplication, WindowManagement } = require("@heyo/api");

exports.default = async () => {
  const out = {};
  const attempt = async (name, run) => {
    try {
      out[name] = await run();
    } catch (e) {
      out[name] = { error: e && e.code ? e.code : "thrown", message: String(e && e.message ? e.message : e) };
    }
  };
  await attempt("files", () => Files.search("probe"));
  await attempt("windows", () => Windows.list());
  await attempt("selection", () => Screen.capture("selection"));
  await attempt("provider", () => AI.provider());
  // Free for anyone to ask.
  await attempt("supported", () => Windows.supported());
  // Through Raycast's API: asked about, once.
  await attempt("frontmost", async () => (await getFrontmostApplication()).name);
  await attempt("activeWindow", async () => (await WindowManagement.getActiveWindow()).id);
  // Being allowed that does not make Heyo's own `Windows` the extension's.
  await attempt("windowsAfter", () => Windows.list());
  fs.writeFileSync(path.join(path.dirname(process.env.HEYO_PROBE_OUT), "probe-bare.json"), JSON.stringify(out, null, 1));
};
