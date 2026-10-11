# @heyo/api

The extension API of [Heyo](../../README.md), a launcher for Linux and
Windows that runs Raycast extensions.

`@heyo/api` is `@raycast/api` and more. Every export of `@raycast/api` is
here under the same name, so an extension moves over by changing its
imports; on top are the things Heyo can do that Raycast's API has no name
for: other applications' windows, the screen and the text on it, dictation,
the clipboard history, the file index, the system, and showing what a
command has in progress while it is away.

The package is not published yet (`"private": true` keeps it from being
published by accident). The specification is `docs/extension-api.md`.

## Using it

```ts
import { isHeyo, List, Windows } from "@heyo/api";

if (isHeyo && (await Windows.supported()) !== "none") {
  const windows = await Windows.list();
}
```

Say in the extension's `package.json` what it may come to ask for:

```json
{
  "heyo": {
    "apiVersion": 1,
    "permissions": ["windows", "screen.capture"],
    "platforms": ["linux", "windows"]
  }
}
```

A permission that is not listed is refused without the user being asked.
`platforms` may be left out; with it, the extension's commands are offered
only on the platforms it names.

`@raycast/utils` and other libraries that import `@raycast/api` need no
change: inside Heyo both names are one module.

## In Heyo and outside it

Inside Heyo the module is Heyo's own; this package's `index.js` is never
loaded. Outside Heyo (in Raycast) `index.js` passes `@raycast/api` through,
which is why that is a peer dependency, and Heyo's own part answers so that
an extension can ask before it uses anything:

- `isHeyo` is `false`, `has()` answers `false`;
- every `supported()` resolves to `"none"`;
- everything else rejects with `new HeyoError("unsupported")`
  (`Dictation.listen()` returns an object whose `result` rejects so);
- `AI.ask` and `Clipboard.paste` are Raycast's: `system`, the `messages`
  so far and the fence around `material` are put before the prompt,
  `onText` adds up what `on("data")` gives, `restoreClipboard` does
  nothing, and `images` are refused.

## What Heyo adds

| Module | Permission | What it does |
|---|---|---|
| `Windows` | `windows` | `list`, `active`, `activate`, `close`, `arrange`, `setBounds`, `monitors` |
| `Screen` | `screen.capture`, `screen.text` | `capture` an area the user selects, the screen, a window or a rectangle; `recognizeText` |
| `Dictation` | `microphone` | `listen`: the text of what is said comes to the extension; `installed`: whether the speech engine has been downloaded |
| `Memory`, `ClipboardHistory` | `memory.read`, `clipboard.history` | `Memory.search` (by words, and with `byMeaning` by what they mean), `Memory.get`; `ClipboardHistory.copy` puts an entry back on the clipboard |
| `Files` | `files.search` | `search`: paths, by name |
| `System` | `system` | `run` an action (lock, sleep, volume, …), `appearance`, `notify`, `audioDevices`, `useAudioDevice` |
| `Clipboard.paste(…, { restoreClipboard })` | `input.paste` | puts back what was on the clipboard |
| `AI.provider()`, `AI.ask(…, { system, messages, images, material, onText })` | `ai` | which model answers; a system prompt; the conversation so far; pictures; text to work on; the answer as it grows |
| `LiveActivity` | `activity` | `start` something in progress that Heyo shows while the command is away (timers and timed progress counted by Heyo); the handle it gives can `update` and `end` it; `list`, `get`, `presentation`; events through `onEvent` or a command marked `"heyo": { "handles": "activity" }` |

Also `environment.heyoVersion`, `apiVersion`, `platform`, `session` and
`offline`, and `has("Module.function")`.

Every module of Heyo's has `supported()`, which answers `"full"`,
`"limited"` or `"none"` for the session Heyo runs in (X11, GNOME on
Wayland, Windows, …). Every failure is a `HeyoError` with a `code`:
`unsupported`, `denied`, `cancelled`, `unavailable`, `invalid` or `failed`.

## Permissions

The user is asked in Heyo's palette the first time an extension uses
`windows`, `screen.capture` (except for an area the user selects),
`microphone`, `clipboard.history`, `memory.read` or `system`; Allow is kept
and can be taken back in Heyo's settings, on the extension's page. A
command in the menu bar or the background cannot ask: until the user has
allowed it from a command opened in Heyo, its calls fail with `denied`.
Restarting, shutting down and logging out are confirmed every time.

`screen.text`, `files.search`, `input.paste`, `ai` and `activity` are not
asked about: listing them is enough. For `activity` the user has a switch
per extension in Heyo's settings instead, and only a command the user ran
(or the handler of a button they chose) can start an activity; one run on
a schedule can update what it has.

A call that needs a permission the manifest does not list fails with
`denied`, and says which: `LiveActivity.start needs the "activity"
permission; add it to "heyo.permissions" in package.json.` `heyo ext
check` finds these before the user does.

Permissions are about what Heyo hands over or does for an extension. They
do not confine it: an extension is a program that runs as the user, and can
read files and start other programs without asking Heyo for anything.

## Not here yet

`Windows.watch`, `Dictation.transcribe`, `Memory.add` and `Memory.remove`,
search providers, `Clipboard.watch`, `Hotkeys`, `Overlay` and `Browser`
are in the specification and not in this version. `BrowserExtension` of
Raycast's API is there by name and refuses.
