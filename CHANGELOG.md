# Changelog

What changed in each version of Heyo, newest first, for the people who use
it. A release's notes are its section here (`scripts/package/notes.sh`).

## 0.0.1 (2026-10-13)

The first version, and a first try of the packages on other people's
computers. Heyo is developed and used every day on GNOME on X11; that is
where everything below works. [The README](README.md#where-it-runs) says
how far each other desktop is.

### What is in it

- **Search** over applications, commands, quicklinks and snippets, which
  learns from what you open; favorites, aliases and a hotkey for any row.
- **Calculator**: arithmetic, units, currencies and time zones, answered
  as you type.
- **Clipboard History**: text, links, colors, files and images, searched
  by words, by when, by the application they came from or by kind. Kept
  on your computer; what a password manager marks as secret is not
  recorded.
- **Dictation**: hold a key and speak, and the words appear where you are
  typing while you are still speaking. Recognized on your computer
  (Mandarin, Cantonese, English, Japanese, Korean; Chinese with English in
  it without changing a setting). An AI model can clean up or translate
  what you said, if you set one up.
- **Screenshots**: select an area or a window, annotate, blur, then copy,
  save or pin it on screen, or copy the text in it.
- **Windows**: switch between them; halves, quarters, thirds, maximize,
  center, next display.
- **Quicklinks and snippets** with placeholders; snippets can expand as
  you type.
- **AI**, with Claude or a model on your own computer: ask from the
  search, or act on the text selected in any application.
- **Raycast extensions**: browse the Raycast Store and install with
  `Enter`; extensions run unchanged in Node.js. Not all of them work:
  [docs/compatibility.md](docs/compatibility.md) has the measured results
  for the 100 most downloaded.
- **Timers and other things in progress** stay at the top of the palette.
- Emoji and symbols, file search, browser bookmarks and history, audio
  devices, processes and the system commands.

### Packages

- Linux, x86_64 and arm64: `.deb`, `.rpm` and AppImage. Heyo needs
  WebKitGTK 4.1; extensions, dictation and text recognition also need
  Node.js 22 or newer.
- Windows, x86_64 and arm64: an installer. Not signed, so Windows warns
  before running it.

### Known limits

- **These packages have not been installed by anyone yet.** They were
  built by CI for the first time for this version.
- **Windows is behind**: the palette, applications, a window layout,
  pasting and the clipboard history ran in a virtual machine; screen
  capture, the selected text, the hold-to-talk key and the installer have
  only been compiled, and dictation does not work there.
- **GNOME on Wayland** works through a Shell extension Heyo carries, tried
  in a container only. **Other Wayland desktops** are not supported; under
  Sway the dictation pill, pinned captures and the screenshot overlay
  work, and little else.
- **macOS**: no.
- The interface is in English only.
