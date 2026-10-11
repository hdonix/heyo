# To do: matching the commonly used features of Raycast

Arranged by Raycast's feature areas. `[x]` is done, `[ ]` is to do.
Priorities: **P0** used often and feasible on Linux, **P1** commonly used, **P2** nice to have, or limited by the platform.

## Root search and launching

- [x] Fuzzy search of applications (`.desktop`) and commands
- [x] Single instance + a global hotkey (a GNOME custom shortcut, which can be recorded in the settings)
- [x] Hides on losing focus, and keeps the current command when opened again
- [x] Ordering by how often things are used (frecency): the applications and commands used most come first, and an empty query shows "Suggestions"
- [x] Favorites: `Ctrl+Shift+F` pins to the top of the root search, and the order can be changed
- [x] Aliases: `Ctrl+K → Set Alias`, such as `cb` → Clipboard History
- [x] A global hotkey of its own for every application / command / quicklink / snippet (`Ctrl+Shift+H` in the root search, or the Hotkey column of Settings → Extensions)
- [x] Opening an application that is already running switches to its window instead of opening another; pressing again cycles through that application's windows
- [ ] **P2** A "New Window" action for applications in the root search
- [x] Fallback commands: below the results, "Search Google", "Search Files" and the quicklinks that have `{query}`
- [ ] **P2** Fallback commands can be ordered and switched on and off in the settings
- [x] **P1** Command arguments are typed straight into the root search bar (Raycast's inline arguments), not in a form that pops up
- [ ] **P2** Searching folders/recent documents

## Built-in tools

- [x] Clipboard history (text/links/colors/files/images, pinning, search, filtering by kind, skipping passwords)
- [x] Calculator: `2^10`, `15% of 80`, unit conversion `5 km in mi`, currencies `100 usd in cny`, time zones `3pm pst in beijing`
- [x] Quicklinks: templates with `{query}`; with an alias set, typing `gh heyo` opens it directly
- [x] Snippets: placeholders such as `{clipboard}`, `{date}` and `{argument}`; the keyword pastes directly from the root search
- [x] System commands: lock screen, sleep, restart, shut down, log out, empty the trash, toggle dark mode, adjust the volume, keep awake
- [x] Switch Windows: search for an open window and switch to it; windows can be closed
- [x] Window management: halves, quarters, thirds, maximize, center, move to the next display (X11, GNOME Wayland, Windows; on Windows a window cannot be moved to the next display)
- [x] Emoji and symbol picker: search in Chinese and English, recently used
- [x] File search: on opening, the file names of the home directory are read into memory (about 0.3 seconds), and every keystroke is a fuzzy match; hidden directories, node_modules and the like are skipped
- [x] Kill Process: search by name, command or listening port
- [x] Switching the audio input/output device
- [x] Searching browser bookmarks and history (Firefox, Chrome, Chromium, Brave, Edge)
- [ ] **P2** Searching file contents (ripgrep)
- [ ] **P2** Searching the tabs open in the browser (needs a browser extension to go with it)
- [ ] **P2** Floating Notes
- [ ] **P2** Calendar (reading local calendars / CalDAV)
- [ ] **P2** Color picker, translation, dictionary
- [ ] **P2** Focus mode (Focus)

## Quicklinks and snippets (later)

- [x] **P1** Snippet keywords expand automatically (typing the keyword in any application replaces it; needs listening to the keyboard)
- [ ] **P2** Choosing what a quicklink opens with (Open With), custom icons

## Clipboard history (later)

The research and the phases of "local memory search" are in `reports/Heyo 本地记忆搜索调研规划.md`.

- [x] **Memory search, phase 0** The history moved to SQLite (`history.db`; the old `history.json` is migrated automatically and then deleted); an index of single Chinese characters + an index of three-character substrings; rules parse the time/source/kind, falling back to plain text when nothing is found that way; the limit relaxed from 1000 entries to 50,000 (mainly relying on the retention period); recording "what was searched for, which entry was chosen"; evaluation with `heyo --clipboard-eval`
- [x] **Memory search: evaluation tools** `heyo --clipboard-eval --export/--generate` (generation is only sent to a model on this machine), a synthetic corpus of 130 entries and 168 cases
- [x] **The implementation of memory search phase 1** Finding by meaning (multilingual-e5-small, off by default, downloaded in the settings); on the synthetic set hits in the top 5 went from 42% to 93%, with a query p95 of about 7 milliseconds
- [ ] **P1 Memory search: real evaluation** The real history has only 33 entries and no record of choices, so the criterion (in four weeks of real use, ≥10% of choices reach the top five only by meaning) cannot be answered yet; across languages it takes about 16 or more entries per language to have an effect. The original item: build the real history up to more than 100 entries (the choices recorded in daily use + queries synthesized by kind by a local model), which is the precondition for entering phase 1
- [x] **P1 Memory search, phase 1** Semantic retrieval of the clipboard: multilingual-e5-small (int8) runs in the onnxruntime-node that OCR uses, with a background vector queue and brute-force retrieval, fused with the keyword results; whether to go on is judged by the four criteria in the report
- [ ] **P2 Memory search, phases 2–4** Browser history, OCR of screenshots taken on purpose, and file names go into the same search
- [ ] **P2** Search by pinyin/initials; encrypting the database (with the key in the Secret Service)

- [x] Pasting as plain text (the Paste as Plain Text command, which can be given a shortcut; the history itself only stores plain text)
- [ ] **P2** Keeping the window after pasting
- [x] **P1** Ignoring given applications (such as sensitive applications other than password managers)
- [x] **P1** Showing the icon of the source application
- [ ] **P2** OCR of images, editing an entry before pasting it
- [ ] **P2** Listening for changes under Wayland (for now it falls back to polling the text every second)

## Selected text and screenshots

- [x] Selected-text actions: translate, polish, correct, shorten, explain, summarize (Claude, streamed), search; the result can replace the original text
- [x] Screenshots: selecting an area, rectangle/arrow/pen annotations, copy, save, pin on screen, take the text (PaddleOCR, locally)
- [x] AI actions support a local model (an OpenAI-compatible interface: vLLM, LM Studio, Ollama; shared with the polishing of speech); `scripts/local-model.sh` starts vLLM + Qwen with Docker
- [x] Screenshots: text annotations, blurring, moving and resizing the selection; annotations can be dragged
- [ ] **P2** Screenshots: double-clicking a text annotation to edit it again; changing the color of an annotation, deleting a single one
- [x] Custom selected-text actions (with a prompt you write yourself)
- [x] **P1** Screenshots: snapping to windows automatically
- [ ] **P2** Scrolling screenshots, screen recording / GIF, selecting across several displays at once

## Voice input

- [x] A shortcut starts/ends recording, recognition is local (SenseVoice), the words appear in the floating indicator as you speak, and each sentence is pasted after a pause
- [x] Optional polishing by Claude (tidy up / translate into English)
- [x] Hold to talk, release to end (a short press is instead press once to start and press again to end; X11 only)
- [x] Hold-to-talk on a single key (right Alt, left Alt, right Ctrl and so on): listens to the raw keys and does not take the key over; short presses and key combinations do not trigger it
- [x] **P1** A custom vocabulary (names, terms), to improve the accuracy of proper nouns
- [ ] **P2** More polishing modes and custom prompts; choosing the style automatically by the current application
- [ ] **P2** An optional higher-accuracy model, or recognition in the cloud
- [x] **Typing as you speak** (2026-10-11): on X11 the sentence in progress goes into the application as it is recognized and is corrected where it stands (`src-tauri/src/dictation/onscreen.rs`, `platform/x11/writing.rs`, `x11/offer.rs`); polished text replaces it at the end; correcting stops for good at any key or click of the user's or when focus moves; terminals and the applications snippet expansion leaves alone still get finished sentences. Setting `dictationLive`, on by default. End to end: `scripts/gui-test/dictate-live.sh`
- [x] **Dictated words through the input method** (2026-10-12): for the length of a recording Heyo is an IBus engine (`platform/ibus/`, `platform/composing.rs`, `dictation/composed.rs`): the sentence in progress is composed in the application and committed when finished, with nothing borrowed from the clipboard and nothing erased; the engine the user had is written down before the switch and given back on every way out and at the next start. Where the focused window takes no input method (no IBus, GTK4 without `ibus-gtk4`, Heyo's own windows, password fields) the words go in by pasting as before
- [ ] **P1 The input method way, left to do** Try it on GNOME Wayland in the container and enable it there; give the engine back on SIGTERM (between a kill and the next start the user types without their input method); Fcitx 5; in a terminal with polishing on, show the end of a long composition instead of its clipped beginning; suggest `ibus-gtk4` where GTK4 applications fall back; decide whether rewriting by XTEST stays now that it serves so few cases (IBus older than 1.5.27, as on Ubuntu 22.04, is refused and falls back too)
- [ ] **P1 Typing as you speak, to be tried by hand** A real microphone; a GNOME custom shortcut as the dictation hotkey; fcitx; Firefox and Chromium; killing Heyo in the middle of an update; GNOME on Wayland and Windows keep finished sentences (the user's own keys cannot be heard there)
- [ ] **P2 Seen while building it and not changed** Two recordings one after the other are not separated by a space; a shortcut for `heyo --dictate` that Heyo did not set counts as typing, so polished text is left out when finishing by it; Alt as the hold-to-talk key opens the menu bar in applications that act on a lone Alt

## AI

- [x] Quick AI: type a question in the root search and press Tab to hand it to the AI to answer (Claude or a local model); follow-up questions are possible
- [x] **P1** The AI Chat window
- [ ] **P2** AI Commands: running prompts such as "rewrite, translate, summarize" on the selected text
- [ ] **P2** `useAI` for extensions (`AI.ask` already works); mapping the `creativity` of `AI.ask` to temperature

## Extensions and the Raycast API

- [x] Searching, installing and uninstalling extensions from the Raycast Store
- [x] List / Grid / Detail / Form / ActionPanel / navigation / Toast / Alert
- [x] Clipboard, LocalStorage, Cache, preferences, command arguments, `@raycast/utils`
- [x] Enabling/disabling commands and editing extension preferences in the settings
- [x] Extension updates: Manage Extensions detects new versions in the store, and updates one or all
- [x] Background/scheduled commands (`interval`, `LaunchType.Background`): after being run once they run automatically at the interval
- [x] Menu bar commands (MenuBarExtra) → the system tray: title, icon, sections, submenus, click actions
- [x] **P1** Faster menu bar clicks: the command is not unloaded once the menu is rendered, and a click calls the action function that is already there (today it has to load again and fetch its data again, and the delay equals one network request). Precondition: the global state of "the current command" in the background process becomes a context each command carries for itself (such as AsyncLocalStorage), otherwise several resident commands would mix up preferences and storage directories
- [ ] **P2** Menu bar: icons of menu items, Raycast's built-in icons as tray icons, `alternate` items, a way to remove an item from the tray
- [x] `AI.ask` works for extensions (through the model chosen in the settings)
- [x] OAuth (`OAuth.PKCEClient`): registered as the handler of `raycast://` links to catch the authorization code; tokens are stored per extension (mode 600)
- [ ] **P1** Verifying OAuth extensions one by one against real services (GitHub, Linear and so on)
- [ ] **P2** Other `raycast://` deep links (opening an extension's command)
- [x] **P1** Developer mode: importing a local extension, `ray develop` hot reload, showing logs
- [x] **P1** `launchCommand` across extensions
- [x] `WindowManagement`, `getFrontmostApplication` (the host reports window positions and can place windows exactly; measured on X11, only compiled on GNOME Wayland and Windows)
- [x] Aliases of the old API (`CopyToClipboardAction`, `FormTextField`, `getLocalStorageItem` and others, 37 in all), `AI.Model`, `Color.Brown`; the old property names `enableFiltering`, `accessoryTitle`, `accessoryIcon`
- [x] Conformance tests: against the types of the real `@raycast/api`, missing names and component properties that are never read have to be registered (`runtime/src/api/conformance.gaps.json`, `props.gaps.json`)
- [x] **P1** `storeValue` of forms (9 components: remembering the last value)
- [x] **P1** The options of `Form.FilePicker` (multiple selection, choosing directories, choosing files, showing hidden files)
- [x] **P1** `Action.CreateSnippet` / `CreateQuicklink` connected to Heyo's own snippets and quicklinks; `Action.PickDate`; `Action.ToggleQuickLook` and `quickLook`
- [ ] **P2** `environment.appearance` following the real theme (hard-coded to dark for now); `environment.launchContext`; `updateCommandMetadata`; `getSelectedFinderItems`; `Form.enableDrafts`, `TextArea.enableMarkdown`; the `application` field of windows
- [ ] **P2** Deepening the conformance tests to function signatures; the 16 left among the gaps at the level of names (`render()`, unstable interfaces, the newer OAuth and the `Cache` constants)
- [ ] **P2** `BrowserExtension` (needs publishing a browser extension of our own); Raycast's AI extensions (`Tool`)
- [ ] **P2** Script Commands: shell scripts with metadata comments as commands
- [ ] **P2** Deeplink: `heyo://extensions/<author>/<ext>/<command>`
- [ ] **P2** Filtering out extensions that clearly support only macOS, or marking them in the store

## Appearance and settings

- [x] A settings window in the style of Raycast (General / Extensions / Advanced / About)
- [x] Opening at login, the path of Node.js
- [x] Light theme / following the system: `src/styles/` is split by module, `tokens.css` defines the light and dark color schemes in one place with `light-dark()`, and the settings offer System/Light/Dark
- [x] Visual polish: the styles moved to SCSS (scales maps + mixins), a 4px spacing grid, a type scale with line heights and letter spacing, uniform icon tiles, a sliding selection highlight, skeleton screens and empty states, a drop shadow for the palette (the window leaves a transparent margin), motion for appearing/switching/popping up, five interaction states
- [ ] **P2** A frosted glass background (depends on each platform's compositor); the styles of the shortcut table and of the dictation and screenshot parts of the settings have not been rewritten with mixins yet; font rendering on Windows has not been looked at
- [x] Compact mode (only the search bar is shown, expanding once something is typed); the settings window draws its own border
- [ ] **P2** A favorites bar in compact mode; scaling and minimizing of the settings window
- [ ] **P2** Font size, remembering the window position, showing on the current screen with several displays
- [ ] **P2** Importing/exporting settings

## Code review (2026-10-07, from five angles)

Fixed in the first batch: a crash after Alt+F4, the store install accepting `..` as an extension name, the settings window overwriting favorites and aliases changed in the palette, a silent reset when the settings file is corrupt, a no-view command closing another command by mistake when it ends, tray icons that could not be removed, Enter/Esc/Tab being taken as commands while an input method is composing, the keyboard going dead after Ctrl+K carried something out, popovers left behind after hiding.

- [x] **Security** CSP; a Markdown allowlist (no `<style>`, forms or media), sanitized only at the last step; links only let through http/https/mailto; `runtime_send` only forwards allowlisted messages; the API key is no longer sent back to any window; the asset protocol blocks the keyring, tokens and the clipboard history
- [x] **Privacy** The clipboard history skips Heyo's own pipeline writes (the pasting and restoring of voice input), copies that extensions mark as `concealed`, and copies made in password manager windows; directories 0700 / files 0600; AI results are shown as the original text (no HTML rendered, no images loaded), and control characters are removed before pasting; copies of browser databases go into a private cache directory; leftover screenshot files are deleted as soon as they have been used
- [x] **P1 Security** Restricting the commands that can be called per window (today every window can call every command); a warning when the local model's address is not this machine; downloading pinned versions on demand and verifying them; confirming multi-line text before it is pasted into a terminal
- [ ] **P2 Privacy** Making the clipboard ignore list a setting, a "Pause Recording" command; quicklink icons no longer requested from Google
- [x] **P0 The background runtime** A separate context or a separate worker for each command; a timeout for background no-view commands; unloading only after a menu item's action has finished; background commands cannot hide/show the palette; `launchCommand` validates before closing and supports the background type; the OAuth callback is also sent to the background process; the leak from modules being loaded repeatedly
- [x] **P1 Input** Search fields and form values controlled by extensions become local-first (today every character waits for the runtime to send it back, which drops characters and interrupts input methods); the search text and the selected item are restored on returning from a child view; the selected item is kept when the root search refreshes
- [x] **Groundwork** Splitting `lib.rs` (1082 → 260 lines); desktop-related code gathered into `platform/`, with the X11 implementation in `platform/x11/`, with session detection and a table of abilities
- [x] **Groundwork (the 8 items of the architecture review of 2026-10-10)** Settings are declared only once; the platform backend is chosen only once (the `Desktop` trait); the messages of the three layers are defined only once (`wire.rs` → `shared/bindings/`); paths in one place (`paths.rs`); the line protocol of child processes (`helper.rs`); system commands belong to the host; the selection logic of lists (`selection.ts`); one catalog of built-in commands, one path that opens by id (`open.rs`), tests of the ordering. 84 Rust tests, 11 tests of the front end and the runtime (vitest)
- [x] **The 8 items of the second architecture review (2026-10-11)** Answering extension requests does not need a running application (`ext/answer.rs`, 28 tests go through the entry point); the members of `@heyo/api` are declared only once (`runtime/src/heyo/members.ts`, from which the lists and tables are all derived); the commands the interface calls on the host have one typed table (`commands/bindings.rs` → `shared/bindings/commands.ts`, `src/host.ts`, with the tests using the same fake host); pasting by borrowing the clipboard is in only one place (`pasting.rs`; reading and writing the clipboard went into `Desktop`); a row of the root search is spelled out in one place (`src/rows.ts`); the consequences of a settings change are one testable list (`settings::consequences`); the test seam between the runtime and the host moved to the transport layer (`transport.ts`, `testhost.ts`); the palette window is only operated on in `palette.rs` (item 8, which introduces no seam). 372 Rust tests, 544 of the front end and the runtime
- [ ] **What the second review did not list as candidates** "Installing with progress" is written three times in `Settings.tsx`, and the page logic has only 4 tests; the invariants of sessions and confirmations in `App.tsx` are only in the listener closures; the geometry and the drag state machine of `Screenshot.tsx` have no tests; installing in `builtins/store.tsx` deletes and replaces the extension's directory, and has no tests; the recording state machine of `dictation.rs`; the constraints on the startup order in `lib.rs`; the extension manifest is parsed in three ways
- [ ] **P2 Seen while taking the README's screenshots (2026-10-11, not looked into)** The detail preview of image entries in the clipboard history is empty (the row thumbnails are fine; possibly the data directory went through a symbolic link in the sandbox); after opening a view command with `heyo --open` while the palette is hidden, the first keystroke is lost (reproduced all three times); at 2x scaling the settings window is occasionally 74 pixels taller than set; under GNOME at 2x scaling the palette is about 250 pixels off the first time it is called up, and centered after that
- [ ] **P2 Seen in this round and not changed** Giving the clipboard back is blind (what the user copied during a long dictation is overwritten by the old content), and only text is given back; the payload type of the `settingsChanged` event does not distinguish the two paths it leaves out; an activity with no action and no buttons that can be ended is ended directly by Enter; `ExtReplies` is written by hand on the runtime's side, and the host's replies are not generated types yet; 7 reply types of `host.*` are still hand-written assertions
- [x] Crash hook, Heyo's own log file, "Copy Diagnostics"
- [ ] **P1 Groundwork** Version checks of `scripts/install.sh` and of the binary/helper scripts; tests of the calculator and of placeholders; switching the audio device and ending processes still call `pactl`/`ps`/`ss` directly in the runtime
- [x] **Wayland (GNOME), step one** `gnome-extension/heyo@heyo.app` + `platform/gnome/`: listing/switching/closing/arranging windows, pasting, the selected text, watching and reading and writing the clipboard, capturing the screen, bringing its own window to the front; it answers only the Heyo process; verified in a GNOME 50 session in Docker
- [x] **Wayland (GNOME), step two** Hold-to-talk (the extension registers a single modifier key as a shortcut to report the press, learns of the release by polling the modifier state, and uses the idle time to judge whether another key was pressed in between); the settings detect the extension and install it with one click (the extension's files are embedded in the binary)
- [x] **Windows can start** Linux-only code is set apart with conditional compilation, and the list of applications comes from the Start menu's shortcuts; cross-compiled from Linux (llvm-mingw), it starts in a Windows 11 ARM virtual machine in Docker and can open applications (`scripts/windows-test/`)
- [x] **The Windows backend, first batch** (`platform/win/`) A global shortcut (Alt+Space by default, stored in `hotkeys.json`), listing/switching/arranging windows, pasting, watching the clipboard; the `node.exe` beside `heyo.exe` is used directly; data in `%APPDATA%\heyo`, cache and logs in `%LOCALAPPDATA%\heyo`; measured in the virtual machine: called up by the shortcut, the left-half layout, pasting an emoji into a terminal, the clipboard history recording the source application
- [x] **The Windows backend, second batch (only compiled, not run)** Screen capture, the selected text (UI Automation, falling back to Ctrl+C), hold-to-talk (a low-level keyboard hook), parsing .lnk and application icons, Store applications, layouts across displays, starting at login, the installer; the checklist is in `scripts/windows-test/CHECKLIST.md`
- [ ] **P1 What is left for Windows** (everything in the previous item needs measuring in the virtual machine) Recording for dictation (`heyo.exe --record` through WASAPI is suggested), whether the speech and recognition engines have win-arm64 versions, a native backdrop for the screenshot overlay, snippet expansion, audio devices and the process list. The original list: the Windows implementations of the system commands, opening files and the trash are written but have not been run in the virtual machine; screenshots and OCR, the selected text, hold-to-talk and dictation (model download, microphone), application icons (taken from .lnk), switching windows by application (the program name a .lnk points to), UWP/Store applications, the installer package and starting at login, several displays, what Heyo itself pastes should not be recorded in the history; compiler warnings on Windows
- [x] **Wayland (GNOME), step three** The extension has a protocol version, and the settings distinguish "missing/outdated/pending/active"; the interfaces of GNOME 45–50 were checked by reading; capture and layouts under scaling and several displays (110 checks of four layouts in the container); Heyo's windows stay out of the Dash/switcher/overview; hold-to-talk is not lost after the screen locks; `docs/wayland.md` describes the approaches for KDE, wlroots and portals
- [ ] **P1 What is left for Wayland** Not done yet: measuring on a real machine with GNOME 46; submitting to extensions.gnome.org (the url in `metadata.json` is missing, and a person has to upload it); when Heyo starts before the Shell enables the extension it falls back to the generic adapter until restarted; KDE/other desktops are not implemented. The original list: a notice after the extension is upgraded (today a version mismatch shows as not installed, and installing again fixes it); measuring in a Wayland session of GNOME 46 (Ubuntu 24.04); screenshot coordinates on high-DPI screens/several displays; keeping Heyo's windows out of the Dock and the overview; submitting to extensions.gnome.org; KDE/other desktops
- [x] Blocking calls on the main thread made asynchronous; several instances of `Cache` overwriting each other; 59 long-sequence emoji shown as broken images
- [ ] **P2** The `appPicker` preference; list virtualization; atomicity of the shortcut settings

## Extension API (`@heyo/api`)

The specification is in `docs/extension-api.md` (draft v0.2): a superset of `@raycast/api`, migrating by changing one import, plus Heyo's own abilities.

- [ ] **Undecided** The 9 questions of section 12 of the specification: whether process isolation goes into v1, window permission for extensions without a manifest, how consent is given for root search providers, the package name, `Memory.add`, "Only This Once", re-exported types or our own, whether to support `Tool`, `BrowserExtension`
- [x] **P1 Phase A** `ext.*` requests, errors with a typed code, requests carrying the extension's name
- [x] **P1 Phase B** Permissions: the manifest, deciding, the prompt, storage, the settings page
- [x] **P1 Phase C** The `@heyo/api` package (both package names point to the same implementation); `heyo build` / `migrate` / `ext check`
- [x] **P1 Phase D** The v1 modules: Windows, Screen, Dictation, Memory, Files, System, AI
- [x] **P1 Phase E** The built-in commands move to the same API, and the corresponding `host.*` requests are deleted
- [ ] **P2 Phases F–G** The event channel and window events; root search result providers
- [ ] **P2 Phase H** One process per extension
- [ ] **Shelved** Verifying migrated extensions in Raycast on macOS (this ARM machine cannot run macOS; it takes a real Mac or a Mac in the cloud)

## Distribution

- [x] The packages can be built: deb, rpm, AppImage, the Windows NSIS installer; `scripts/install.sh`; version checks of the program and the helper scripts; the updater is wired in but not compiled by default (`packaging/README.md`)
- [ ] **P0 What is left for distribution (a person has to do it)** The licence (the repository has none, so neither do the packages); installing and running every kind of package; x86_64 builds; the signing key and the release address for updates; Windows code signing; the AUR and Flatpak manifests have never been built (the conclusion for Flatpak is that it is not feasible as things stand)
- [x] **P1** A compatibility dashboard for Raycast extensions: runs the popular extensions automatically and publishes which ones work
- [ ] **P2** Importing from Raycast, Ulauncher, Albert; first-run onboarding
- [ ] **P2** A trademark search for "Heyo"

## Performance

- [x] The window shadow became an image (under software rendering the blur was recomputed on every redraw); the selection highlight no longer jumps back at the edges of a list; measured by `scripts/gui-test/scroll.sh`
- [ ] **P2** The frame rate of key navigation on a real machine has not been measured (in the sandbox each step takes 11–13 milliseconds to draw)

## The interface engine

- [x] **Flutter tried, and shelved** (2026-10-11): rendering is three to four times faster and input methods work, but the whole interface layer would have to be rewritten, the HTML and SVG in extensions cannot be drawn, the stable channel has no multi-window, and Windows cannot be cross-compiled. The figures and the reasons are in `docs/flutter-evaluation.md`, the code on the branch `flutter-ui`
- [x] **Rendering performance: the cause found** (2026-10-11, `research_notes/gpu/webkitgtk.md`): NVIDIA's GBM backend needs `nvidia-drm modeset=1`, and the DGX Spark's image turns it off with the `nvidia-drm-options-modeset0` package (`/etc/modprobe.d/zz-nvidia-drm-override.conf`), so WebKit falls back to Mesa's software path and fails. The present CPU path turns off all accelerated compositing (no `backdrop-filter` and no 3D). A new setting, Advanced → Rendering: Automatic / CPU / GPU shared memory / GPU DMA-BUF
- [ ] **P1 Rendering performance: two ways out** (a) remove that package or the override file, enable `nvidia-drm.modeset=1`, restart and cold boot once (it works on machines of the same model according to the forum; the user has to decide); (b) in the meantime use "GPU shared memory": the GPU draws, but every frame is a fixed 36 ms or so (the UI's FrameDone handshake is serialized, plus a synchronous readback; 2.52 has no switch to change it), better than the CPU's 65 ms at 2× scaling and worse than its 16 ms at 1×. The alternative engines (Verso is archived, the CEF runtime is a 400 MB alpha, Electron is 124 MB) are none of them worth it
- [ ] **P1 Display scaling**: the development machine's desktop became 2× on 2026-10-11 (`Xft.dpi` 192), and on the CPU path every frame is 65 ms; to be confirmed whether that was intended
- [x] `environment.appearance` follows the setting (through an environment variable at startup, through a message when it changes; with "follow the system" chosen the system theme is read once at startup and when the setting is changed, and a change of the system theme in between is not followed)

## Left over from this round (after implementing P0/P1 in parallel on 2026-10-10)

- [ ] **P1 Measuring** A great many features have only unit tests and have not been seen in the interface: resident menu bar commands in the background, local-first input, restoring on return, `storeValue`, the file picker, the date picker, Quick Look, the AI conversation (never tried with a real model), the clipboard ignore list and pausing, the dictation vocabulary, the confirmation for pasting into a terminal, diagnostics, the interface of finding by meaning, dictation and asking with images through the extension API, the list of permissions in the settings
- [ ] **P1 Manual checks of snippet expansion** A real keyboard, Electron/Qt/Firefox, terminals, input methods, dead keys; not supported on GNOME Wayland and Windows
- [x] **Extension API, phase E** Switching windows, the system commands, audio devices, file search, and reading and copying from the clipboard history moved to `@heyo/api`, with 15 `host.*` requests deleted; Ask AI and AI Chat were not moved (`AI.ask` does not support a conversation history)
- [x] **Revising the specification (the conclusions of phase E)** Done as v0.3; `AI.ask` supports `messages`, `system`, `images` and `material`, and Ask AI / AI Chat / the selected-text actions all moved to it. The original item: the fields of `MemoryItem`, finding by meaning in `Memory.search`, removing `ClipboardHistory.search`, `AI.ask` supporting earlier messages, the order of `Windows.list()`, a limit of 200 or of 300, whether `System` includes audio and processes
- [x] **P2 Deviations between the extension API and the specification** A refusal is only remembered for 60 seconds; `offline` is always false; `apiVersion` does not block installing; uninstalling from Manage Extensions does not clear permissions; permissions are not shown at install time
- [ ] **P1 To decide** `storeValue` of a form's password field is stored in plain text now (readable only by the user); the AI's answer goes on being generated after the palette closes
- [ ] **P2** In the background process, the "current command" that class component lifecycles and ref callbacks read may be another command; read-modify-write of `Cache` across processes has no lock; an extension with only menu bar commands cannot be asked about launching across extensions
- [ ] **P2** After `heyo dev` rebuilds, menu bar and background commands are not restarted; AI tools (`tools/*.ts`) are not built
- [ ] **P2** When pasting from Heyo's own commands, the source application of the clipboard entry is sometimes recorded as Heyo and sometimes as the target window (depending on which is in front when the watcher handles it)
- [x] **P2** Tested with an audio file, the dictation helper lost the first word of a recording ("Front center" → "Center."): the voice detector was never reset between recordings, so from the second one on the lead-in was lost
- [x] **P2** Node's `readline` breaks lines at U+2028/U+2029: the runtime's pipes, the dictation helper process and the compatibility run all changed to breaking only at newlines (`streams` in `runtime/src/transport.ts`)
- [x] **Live activities, step one** (2026-10-11): the table of activities in the host (sequence protection, limits, two-step degradation, ordering), `ext.activity.*` and the permission `activity`, `LiveActivity` in `@heyo/api`, the "In Progress" section in the palette, the built-in Timer command, notifications as the only presenter. Deviations: a new `elapsed` event; `handles` is written on the command entry; the switch for each extension is stored in the settings
- [ ] **Live activities (a Dynamic Island)** The design `docs/live-activities.md` v0.3 (research + the conclusions of discussion: the status bar as the main ground, also carrying menu bar extensions; the collapsed state takes data while the expanded state can have a custom interface; the "In Progress" section in the palette; quiet by default). The phase 0 prototype is to be done once you have settled the questions of section 12; the "In Progress" section depends on no decision and can be done first
- [ ] The dictation window was "late both to appear and to disappear" three times in the sandbox, and did not reproduce in 16 tries after that; the cause has not been established

- [ ] The first render of a large list is big (about 1900 entries with actions ≈ 4.5 MB); the action panel could be loaded on demand
