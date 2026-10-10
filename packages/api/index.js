"use strict";
// `@heyo/api` outside Heyo.
//
// Inside Heyo this file is never loaded: Heyo provides the module itself.
// Anywhere else (in Raycast) an extension written against `@heyo/api` gets
// Raycast's API under the same names, and Heyo's own part in a shape it can
// ask before using: `isHeyo` is false, every `supported()` answers "none",
// and everything else of Heyo's fails with a `HeyoError` whose code is
// "unsupported".

const raycast = require("@raycast/api");

class HeyoError extends Error {
  constructor(code, message) {
    super(message || "This failed.");
    this.name = "HeyoError";
    this.code = code;
  }
}

const notHere = (name) => new HeyoError("unsupported", `${name} is only available in Heyo.`);
const refuses = (name) => () => Promise.reject(notHere(name));

/** A module of Heyo's: it says it is not supported, and its functions refuse. */
function absent(name, functions) {
  const module = { supported: () => Promise.resolve("none") };
  for (const each of functions) module[each] = refuses(`${name}.${each}`);
  return module;
}

const Windows = absent("Windows", ["list", "active", "activate", "close", "arrange", "setBounds", "monitors"]);
const Screen = absent("Screen", ["capture", "recognizeText"]);
const Memory = absent("Memory", ["search", "get"]);
const ClipboardHistory = absent("ClipboardHistory", ["search", "copy"]);
const Files = absent("Files", ["search"]);
const System = absent("System", ["run", "appearance", "notify", "audioDevices", "useAudioDevice"]);

const Dictation = absent("Dictation", ["installed"]);
// Not a promise but something to stop: the refusal is its result.
Dictation.listen = () => {
  const result = Promise.reject(notHere("Dictation.listen"));
  // Refused whether or not anyone waits for it.
  result.catch(() => {});
  return { stop() {}, result };
};

const AI = {
  ...raycast.AI,
  provider: refuses("AI.provider"),
  // Raycast's `ask`. Its model is given a system prompt the only way there
  // is, before the prompt; it cannot be handed image files.
  ask(prompt, options = {}) {
    const { system, images, ...rest } = options;
    if (images && images.length) {
      const refused = Promise.reject(notHere("AI.ask with images"));
      refused.catch(() => {});
      return Object.assign(refused, { on() {} });
    }
    return raycast.AI.ask(system ? `${system}\n\n${prompt}` : prompt, rest);
  },
};

const Clipboard = {
  ...raycast.Clipboard,
  // Raycast leaves the pasted text on the clipboard.
  paste: (content) => raycast.Clipboard.paste(content),
};

const platforms = { win32: "windows", darwin: "macos" };
const ofHeyo = {
  heyoVersion: "",
  apiVersion: 0,
  platform: platforms[process.platform] || "linux",
  session: "unknown",
  offline: false,
};
// Raycast's own, whose fields follow the running command, with Heyo's beside them.
const environment = new Proxy(raycast.environment || {}, {
  get: (target, key) => (Object.hasOwn(ofHeyo, key) ? ofHeyo[key] : target[key]),
  has: (target, key) => Object.hasOwn(ofHeyo, key) || key in target,
  ownKeys: (target) => [...new Set([...Reflect.ownKeys(target), ...Object.keys(ofHeyo)])],
  getOwnPropertyDescriptor: (target, key) =>
    Object.hasOwn(ofHeyo, key) ? { value: ofHeyo[key], enumerable: true, configurable: true, writable: false } : Reflect.getOwnPropertyDescriptor(target, key),
});

const heyo = {
  isHeyo: false,
  has: () => false,
  HeyoError,
  environment,
  AI,
  Clipboard,
  Windows,
  Screen,
  Dictation,
  Memory,
  ClipboardHistory,
  Files,
  System,
};

// Everything of `@raycast/api` under its own name, as it is at the moment
// it is used; Heyo's own on top.
for (const name of Object.keys(raycast)) {
  if (!Object.hasOwn(heyo, name)) Object.defineProperty(exports, name, { enumerable: true, get: () => raycast[name] });
}
Object.assign(exports, heyo);
Object.defineProperty(exports, "__esModule", { value: true });
