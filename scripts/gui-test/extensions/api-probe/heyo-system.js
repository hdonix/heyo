// `Files`, `System`, `Clipboard.paste` with the clipboard put back, and
// `Dictation.listen` ("files.search" and "input.paste" need no asking;
// "system" and "microphone" are allowed beforehand). Nothing here ends the
// session: restart, shut-down and log-out are left alone on purpose.
const { Clipboard, Dictation, Files, System } = require("@heyo/api");
const { probe, sleep } = require("./probe.js");

exports.default = () =>
  probe("system", {
    files: async () => {
      // The index is built when first asked; give it a moment.
      await Files.search("probe");
      await sleep(3000);
      const { files, scanning } = await Files.search("api-probe", { limit: 3 });
      return { count: files.length, first: files[0], scanning };
    },
    appearance: () => System.appearance(),
    notified: () => System.notify("API probe", "A notification from an extension"),
    mute: () => System.run("toggle-mute"),
    unmute: () => System.run("toggle-mute"),
    pasted: async () => {
      await Clipboard.copy("before the paste");
      await sleep(900);
      await Clipboard.paste("pasted by the probe", { restoreClipboard: true });
      await sleep(900);
      return Clipboard.readText();
    },
    // With HEYO_DICTATION_WAV set (see dictate.sh) this hears the file;
    // without the speech engine it is "unavailable".
    heard: async () => {
      const segments = [];
      const listening = Dictation.listen({ indicator: true, onSegment: (text) => segments.push(text) });
      // A second one while the first is going is refused.
      await sleep(1500);
      const second = await Dictation.listen().result.catch((e) => e.code);
      await sleep(1500);
      listening.stop();
      return { text: await listening.result, segments, second };
    },
  });
