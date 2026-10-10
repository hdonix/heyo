// What an extension sees of the window in front, and what moving it does.
const fs = require("node:fs");
const { WindowManagement, getFrontmostApplication } = require("@raycast/api");

exports.default = async () => {
  const out = {};
  const attempt = async (name, run) => {
    try {
      out[name] = await run();
    } catch (e) {
      out[name] = { error: String(e && e.message ? e.message : e) };
    }
  };
  await attempt("application", () => getFrontmostApplication());
  await attempt("active", () => WindowManagement.getActiveWindow());
  await attempt("desktops", () => WindowManagement.getDesktops());
  await attempt("onDesktop", async () => (await WindowManagement.getWindowsOnActiveDesktop()).length);
  if (out.active && out.active.id) {
    await attempt("moved", async () => {
      await WindowManagement.setWindowBounds({ id: out.active.id, bounds: { position: { x: 300, y: 200 }, size: { width: 640, height: 480 } } });
      await new Promise((resolve) => setTimeout(resolve, 600));
      return (await WindowManagement.getActiveWindow()).bounds;
    });
  }
  fs.writeFileSync(process.env.HEYO_PROBE_OUT, JSON.stringify(out, null, 1));
};
