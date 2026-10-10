// What Heyo says of itself, and how well this session supports each module.
// None of it needs a permission.
const heyo = require("@heyo/api");
const { probe } = require("./probe.js");

exports.default = () =>
  probe("info", {
    isHeyo: () => heyo.isHeyo,
    sameModule: () => heyo === require("@raycast/api"),
    environment: () => {
      const { heyoVersion, apiVersion, platform, session, offline, extensionName, commandName } = heyo.environment;
      return { heyoVersion, apiVersion, platform, session, offline, extensionName, commandName };
    },
    has: () => ({
      "Windows.list": heyo.has("Windows.list"),
      "Screen.recognizeText": heyo.has("Screen.recognizeText"),
      "Windows.watch": heyo.has("Windows.watch"),
      "Memory.add": heyo.has("Memory.add"),
    }),
    supported: async () => ({
      Windows: await heyo.Windows.supported(),
      Screen: await heyo.Screen.supported(),
      Dictation: await heyo.Dictation.supported(),
      Memory: await heyo.Memory.supported(),
      ClipboardHistory: await heyo.ClipboardHistory.supported(),
      Files: await heyo.Files.supported(),
      System: await heyo.System.supported(),
    }),
    dictationInstalled: () => heyo.Dictation.installed(),
    // `ai` is listed and needs no asking.
    provider: () => heyo.AI.provider(),
  });
