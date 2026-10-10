# Which Raycast extensions run on Heyo

The 100 most-downloaded extensions of the Raycast Store, each command launched once in Heyo's extension runtime on Linux and watched. This is what was seen, not a promise. The page with every command and its evidence is [index.html](index.html); the data is [results.json](results.json).

- Heyo 0.1.0, commit `ce87ea2`, runtime `bd41df8be768`
- Run on 2026-10-10, linux arm64, Node.js v24.14.0
- Store listing of 2026-10-10: 3394 extensions. Ordering: download_count, descending, over the whole public listing (the Store's own order is not by downloads and it takes no sort parameter).
- Each command: its own process, 15 s to settle, 1024 MiB; isolation: bubblewrap; network: allowed, and each command launched a second time without.

## Extensions (100)

| Verdict | Extensions | Share |
|---|---:|---:|
| Launches cleanly | 15 | 15% |
| Needs configuration | 19 | 19% |
| Partly | 49 | 49% |
| Inconclusive | 8 | 8% |
| Fails | 9 | 9% |

For comparison, what the authors told the Store: 28 of the 100 are listed for macOS only, 67 for Windows too. None is listed for Linux; the Store has no such listing.

## Commands (753)

| Verdict | Commands | Share |
|---|---:|---:|
| Launches cleanly | 114 | 15% |
| Needs configuration | 270 | 36% |
| Partly | 67 | 9% |
| Inconclusive | 135 | 18% |
| Fails | 167 | 22% |

## Extensions, with the network cut

| Verdict | Extensions | Share |
|---|---:|---:|
| Launches cleanly | 8 | 8% |
| Needs configuration | 19 | 19% |
| Partly | 49 | 49% |
| Inconclusive | 14 | 14% |
| Fails | 10 | 10% |

## What the verdicts mean

- **Launches cleanly** — The command showed something of its own (a list with items, a text, a form), or ran to its end, with no error and no failure message — and the static check found nothing in its code that cannot work on Heyo. Only the first screen was looked at; no action was pressed.
- **Needs configuration** — It stops at something only its user can give: a required preference (an API key, a server address), an account, a sign-in in the browser, an AI model. Required preferences were filled with made-up values; what happens with real ones was not tested.
- **Partly** — It launches, but its code holds parts that cannot work here (AppleScript, macOS programs, a missing API) which the launch did not reach; or it showed its content and a failure message besides.
- **Inconclusive** — Nothing that says either way: an empty view, a view still loading or nothing at all when time was up, a failure message of the extension's own, or a program or another application's files that the test machine does not have.
- **Fails** — An error was thrown or the runtime process ended; or it failed for a reason that is Heyo's or the platform's: macOS-only code, a missing API, an error inside the runtime.

## What stands in the way

| Reason | Extensions | Commands |
|---|---:|---:|
| Written for macOS (AppleScript, macOS programs or paths) | 45 | 211 |
| Showed an empty view | 20 | 34 |
| Needs a sign-in in the browser (OAuth) | 14 | 132 |
| Has required preferences; launched with made-up values | 12 | 84 |
| Showed a failure message of its own, which may be right for an empty machine | 11 | 32 |
| Showed a message that something it needs is not there | 8 | 40 |
| Needs an account, a key or a token | 6 | 52 |
| Turned away the made-up arguments it was launched with | 6 | 12 |
| Hands over to another application through that application's own kind of link | 3 | 23 |
| Needs a program that is not installed on the test machine | 3 | 10 |
| Asks for an AI model; none is set up in the test | 2 | 2 |
| Says it works only on Windows (the test ran on Linux) | 1 | 3 |
| An error of the extension's own (thrown in its code, or shown as a failure toast) | 1 | 2 |
| Still loading when time was up | 1 | 1 |
| Uses part of the Raycast API that Heyo does not have | 1 | 1 |

As exactly as the evidence names it (only what something could be done about; “only this” counts the extensions with nothing else in their way):

| Blocker | Extensions | Only this | Commands | Which |
|---|---:|---:|---:|---|
| macOS only: in code the launch did not reach | 25 | 4 | 47 | spotify-player, visual-studio-code, chatgpt, timers, raycast-system-monitor, cleanshotx, +19 |
| macOS only: probably (a failure in code with macOS-only parts) | 19 | 4 | 61 | visual-studio-code, arc, apple-notes, warp, music, remove-paywall, +13 |
| macOS only: AppleScript | 14 | 1 | 71 | slack, arc, apple-notes, coffee, music, sips, +8 |
| macOS only: says it is for macOS | 5 | 0 | 8 | screenocr, toothpick, cursor-recent-projects, spotify-controls, easydict |
| macOS only: a macOS program it ships or downloads | 2 | 0 | 12 | apple-reminders, audio-device |
| macOS only: sw_vers | 2 | 0 | 4 | raycast-system-monitor, music |
| macOS only: caffeinate | 1 | 0 | 3 | coffee |
| Extension's own error: The database does not exist | 1 | 0 | 2 | messages |
| macOS only: a macOS path | 1 | 0 | 2 | safari |
| macOS only: screencapture | 1 | 0 | 2 | raycast-gemini |
| macOS only: open | 1 | 1 | 1 | screenshot |
| Missing API: Action.InstallMCPServer | 1 | 1 | 1 | model-context-protocol-registry |

## Parts of the API that Heyo does not have

Every member of @raycast/api that a command's bundle names (List.Item.Detail, getSelectedFinderItems, …) was looked up in the API object the runtime really hands to extensions. These are the ones that are not there. A minified bundle hides which top-level names it uses, so this undercounts.

| Member | Extensions | Commands | Of those, not clean | Which |
|---|---:|---:|---:|---|
| `Action.InstallMCPServer` | 1 | 1 | 1 | model-context-protocol-registry |
| `ActionPanel.Item` | 1 | 1 | 1 | raycast-gemini |
| `AI.refreshModels` | 1 | 1 | 1 | raycast-ollama |

## What AppleScript is used for

The applications that the AppleScript (and JXA) in the commands' bundles speaks to — “tell application …” — read from the text of the bundles, bundled libraries included. It says what an extension would need an equivalent for, not that the launch reached it.

| Application told | Extensions | Commands | Which |
|---|---:|---:|---|
| System Events | 24 | 101 | visual-studio-code, slack, brew, arc, coffee, apple-reminders, +18 |
| (named at run time) | 10 | 54 | visual-studio-code, brew, apple-reminders, music, remove-paywall, browser-bookmarks, +4 |
| Finder | 9 | 23 | visual-studio-code, chatgpt, warp, downloads-manager, sips, folder-search, +3 |
| Path Finder | 4 | 15 | visual-studio-code, warp, sips, cursor-recent-projects |
| iTerm | 3 | 16 | brew, shell, iterm |
| Mail | 3 | 12 | apple-reminders, mail, imessage-2fa |
| Terminal | 3 | 8 | visual-studio-code, brew, shell |
| Spotify | 2 | 46 | spotify-player, spotify-controls |
| Arc | 2 | 17 | arc, raindrop-io |
| Safari | 2 | 13 | sips, raindrop-io |
| Calendar | 2 | 2 | apple-reminders, quick-event |
| Music | 1 | 26 | music |
| Preview | 1 | 12 | sips |
| Notes | 1 | 6 | apple-notes |
| Amphetamine | 1 | 5 | amphetamine |
| Activity Monitor | 1 | 2 | raycast-system-monitor |
| Cursor | 1 | 2 | cursor-recent-projects |
| Messages | 1 | 2 | messages |
| Slack | 1 | 2 | slack |
| Brave Browser | 1 | 1 | raindrop-io |

## What the static check found

| Rule | Extensions | Commands |
|---|---:|---:|
| `applescript` | 47 | 289 |
| `mac-path` | 31 | 164 |
| `mac-program` | 26 | 147 |
| `open-flags` | 6 | 24 |
| `darwin-only` | 5 | 19 |
| `api-unsupported` | 3 | 25 |
| `api-missing` | 1 | 1 |

## Every extension

| # | Extension | Downloads | Verdict | Commands (clean / configure / partly / inconclusive / fail) | Mostly |
|---:|---|---:|---|---|---|
| 1 | [Google Translate](https://www.raycast.com/gebeto/translate) | 499K | Launches cleanly | 6 / 0 / 0 / 0 / 0 |  |
| 2 | [Spotify Player](https://www.raycast.com/mattisssa/spotify-player) | 491K | Partly | 0 / 34 / 1 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 3 | [Visual Studio Code](https://www.raycast.com/thomas/visual-studio-code) | 413K | Partly | 1 / 0 / 2 / 3 / 1 | Written for macOS (AppleScript, macOS programs or paths) |
| 4 | [Linear](https://www.raycast.com/linear/linear) | 403K | Needs configuration | 0 / 14 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 5 | [Slack](https://www.raycast.com/mommertf/slack) | 336K | Partly | 0 / 8 / 0 / 0 / 1 | Needs a sign-in in the browser (OAuth) |
| 6 | [Brew](https://www.raycast.com/nhojb/brew) | 295K | Partly | 1 / 0 / 0 / 11 / 0 | Needs a program that is not installed on the test machine |
| 7 | [Notion](https://www.raycast.com/notion/notion) | 281K | Needs configuration | 0 / 6 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 8 | [ChatGPT](https://www.raycast.com/abielzulio/chatgpt) | 261K | Partly | 0 / 7 / 2 / 0 / 0 | Has required preferences; launched with made-up values |
| 9 | [1Password](https://www.raycast.com/khasbilegt/1password) | 252K | Partly | 1 / 0 / 0 / 3 / 0 | Showed a message that something it needs is not there |
| 10 | [Arc](https://www.raycast.com/the-browser-company/arc) | 252K | Fails | 0 / 0 / 0 / 0 / 16 | Written for macOS (AppleScript, macOS programs or paths) |
| 11 | [GitHub](https://www.raycast.com/raycast/github) | 210K | Needs configuration | 0 / 22 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 12 | [Speedtest](https://www.raycast.com/tonka3000/speedtest) | 186K | Inconclusive | 0 / 0 / 0 / 1 / 0 | Still loading when time was up |
| 13 | [Obsidian](https://www.raycast.com/marcjulian/obsidian) | 176K | Partly | 2 / 0 / 0 / 11 / 0 | Showed a message that something it needs is not there |
| 14 | [Apple Notes](https://www.raycast.com/raycast/apple-notes) | 156K | Fails | 0 / 0 / 0 / 1 / 6 | Written for macOS (AppleScript, macOS programs or paths) |
| 15 | [Coffee](https://www.raycast.com/mooxl/coffee) | 153K | Partly | 3 / 0 / 0 / 2 / 4 | Written for macOS (AppleScript, macOS programs or paths) |
| 16 | [Google Search](https://www.raycast.com/mblode/google-search) | 152K | Launches cleanly | 2 / 0 / 0 / 0 / 0 |  |
| 17 | [Video Downloader](https://www.raycast.com/vimtor/video-downloader) | 136K | Inconclusive | 0 / 0 / 0 / 1 / 0 | Showed a message that something it needs is not there |
| 18 | [Timers](https://www.raycast.com/ThatNerd/timers) | 124K | Partly | 4 / 0 / 13 / 2 / 0 | Written for macOS (AppleScript, macOS programs or paths) |
| 19 | [Apple Reminders](https://www.raycast.com/raycast/apple-reminders) | 122K | Partly | 1 / 0 / 0 / 2 / 6 | Written for macOS (AppleScript, macOS programs or paths) |
| 20 | [System Monitor](https://www.raycast.com/hossammourad/raycast-system-monitor) | 122K | Partly | 0 / 0 / 1 / 0 / 1 | Written for macOS (AppleScript, macOS programs or paths) |
| 21 | [CleanShot X](https://www.raycast.com/Aayush9029/cleanshotx) | 121K | Partly | 2 / 0 / 1 / 20 / 0 | Hands over to another application through that application's own kind of link |
| 22 | [Zoom](https://www.raycast.com/raycast/zoom) | 117K | Needs configuration | 1 / 4 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 23 | [Warp](https://www.raycast.com/warpdotdev/warp) | 116K | Fails | 0 / 0 / 0 / 4 / 1 | Hands over to another application through that application's own kind of link |
| 24 | [Pomodoro](https://www.raycast.com/asubbotin/pomodoro) | 113K | Partly | 1 / 1 / 3 / 0 / 0 | Written for macOS (AppleScript, macOS programs or paths) |
| 25 | [Lorem Ipsum](https://www.raycast.com/AntonNiklasson/lorem-ipsum) | 112K | Needs configuration | 3 / 1 / 0 / 0 / 0 | Asks for an AI model; none is set up in the test |
| 26 | [YouTube](https://www.raycast.com/tonka3000/youtube) | 111K | Needs configuration | 0 / 4 / 0 / 0 / 0 | Needs an account, a key or a token |
| 27 | [GIF Search](https://www.raycast.com/josephschmitt/gif-search) | 108K | Partly | 1 / 0 / 0 / 2 / 0 | Showed an empty view |
| 28 | [Music](https://www.raycast.com/fedevitaledev/music) | 102K | Partly | 0 / 1 / 2 / 0 / 24 | Written for macOS (AppleScript, macOS programs or paths) |
| 29 | [Format JSON](https://www.raycast.com/destiner/json-format) | 101K | Partly | 3 / 0 / 0 / 3 / 0 | Showed a failure message of its own, which may be right for an empty machine |
| 30 | [Todoist](https://www.raycast.com/doist/todoist) | 97K | Needs configuration | 1 / 10 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 31 | [Remove Paywall](https://www.raycast.com/tegola/remove-paywall) | 87K | Partly | 1 / 0 / 0 / 1 / 1 | Written for macOS (AppleScript, macOS programs or paths) |
| 32 | [Google Gemini](https://www.raycast.com/EvanZhouDev/raycast-gemini) | 86K | Partly | 0 / 15 / 0 / 0 / 2 | Needs an account, a key or a token |
| 33 | [Downloads Manager](https://www.raycast.com/thomas/downloads-manager) | 85K | Partly | 1 / 0 / 0 / 6 / 0 | Showed a message that something it needs is not there |
| 34 | [Tailwind CSS](https://www.raycast.com/vimtor/tailwindcss) | 81K | Launches cleanly | 4 / 0 / 0 / 0 / 0 |  |
| 35 | [Clean Keyboard](https://www.raycast.com/ike-gg/clean-keyboard) | 80K | Launches cleanly | 1 / 0 / 0 / 0 / 0 |  |
| 36 | [Set Audio Device](https://www.raycast.com/benvp/audio-device) | 77K | Partly | 3 / 0 / 0 / 3 / 6 | Written for macOS (AppleScript, macOS programs or paths) |
| 37 | [Shell](https://www.raycast.com/asubbotin/shell) | 77K | Partly | 0 / 0 / 1 / 0 / 0 | Written for macOS (AppleScript, macOS programs or paths) |
| 38 | [Emoji Search](https://www.raycast.com/FezVrasta/emoji) | 73K | Launches cleanly | 1 / 0 / 0 / 0 / 0 |  |
| 39 | [Google Calendar](https://www.raycast.com/thomas/google-calendar) | 72K | Needs configuration | 0 / 5 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 40 | [Image Modification](https://www.raycast.com/HelloImSteven/sips) | 71K | Partly | 0 / 0 / 3 / 1 / 8 | Written for macOS (AppleScript, macOS programs or paths) |
| 41 | [Perplexity](https://www.raycast.com/third774/perplexity) | 69K | Partly | 0 / 0 / 1 / 0 / 0 | Written for macOS (AppleScript, macOS programs or paths) |
| 42 | [Jira](https://www.raycast.com/raycast/jira) | 68K | Needs configuration | 0 / 9 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 43 | [Browser Bookmarks](https://www.raycast.com/raycast/browser-bookmarks) | 68K | Inconclusive | 0 / 0 / 0 / 1 / 0 | Showed an empty view |
| 44 | [Bitwarden Vault](https://www.raycast.com/jomifepe/bitwarden) | 67K | Needs configuration | 0 / 11 / 0 / 0 / 0 | Has required preferences; launched with made-up values |
| 45 | [Deepcast](https://www.raycast.com/mooxl/deepcast) | 63K | Needs configuration | 0 / 34 / 0 / 0 / 0 | Needs an account, a key or a token |
| 46 | [Port Manager](https://www.raycast.com/lucaschultz/port-manager) | 63K | Inconclusive | 0 / 0 / 0 / 4 / 0 | Needs a program that is not installed on the test machine |
| 47 | [OpenAI GPT](https://www.raycast.com/simicvm/openai-gpt) | 62K | Needs configuration | 0 / 1 / 0 / 0 / 0 | Has required preferences; launched with made-up values |
| 48 | [ScreenOCR](https://www.raycast.com/huzef44/screenocr) | 62K | Partly | 1 / 0 / 0 / 0 / 4 | Says it works only on Windows (the test ran on Linux) |
| 49 | [Google Workspace](https://www.raycast.com/raycast/google-workspace) | 61K | Needs configuration | 0 / 8 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 50 | [Safari](https://www.raycast.com/loris/safari) | 61K | Partly | 0 / 0 / 1 / 0 / 11 | Written for macOS (AppleScript, macOS programs or paths) |
| 51 | [Toothpick](https://www.raycast.com/VladCuciureanu/toothpick) | 59K | Partly | 1 / 0 / 0 / 1 / 17 | Written for macOS (AppleScript, macOS programs or paths) |
| 52 | [Change Case](https://www.raycast.com/erics118/change-case) | 55K | Launches cleanly | 22 / 0 / 0 / 0 / 0 |  |
| 53 | [WhatsApp](https://www.raycast.com/vimtor/whatsapp) | 54K | Partly | 0 / 0 / 2 / 2 / 0 | Showed an empty view |
| 54 | [Docker](https://www.raycast.com/priithaamer/docker) | 53K | Inconclusive | 0 / 0 / 0 / 3 / 0 | Showed a message that something it needs is not there |
| 55 | [ray.so](https://www.raycast.com/garrett/ray-so) | 53K | Launches cleanly | 2 / 0 / 0 / 0 / 0 |  |
| 56 | [Messages](https://www.raycast.com/thomaslombart/messages) | 53K | Fails | 0 / 0 / 0 / 1 / 4 | An error of the extension's own (thrown in its code, or shown as a failure toast) |
| 57 | [Google Maps Search](https://www.raycast.com/ratoru/google-maps-search) | 53K | Needs configuration | 0 / 4 / 0 / 0 / 0 | Has required preferences; launched with made-up values |
| 58 | [Visual Studio Code - Project Manager](https://www.raycast.com/MarkusLanger/vscode-project-manager) | 52K | Fails | 0 / 0 / 0 / 0 / 1 | Written for macOS (AppleScript, macOS programs or paths) |
| 59 | [Folder Search](https://www.raycast.com/GastroGeek/folder-search) | 52K | Fails | 0 / 0 / 0 / 0 / 2 | Written for macOS (AppleScript, macOS programs or paths) |
| 60 | [Quit Applications](https://www.raycast.com/mackopes/quit-applications) | 52K | Fails | 0 / 0 / 0 / 0 / 1 | Written for macOS (AppleScript, macOS programs or paths) |
| 61 | [MyIP](https://www.raycast.com/Kang/myip) | 51K | Launches cleanly | 2 / 0 / 0 / 0 / 0 |  |
| 62 | [Figma File Search](https://www.raycast.com/michaelschultz/figma-files-raycast-extension) | 50K | Needs configuration | 0 / 3 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 63 | [Installed Extensions](https://www.raycast.com/pernielsentikaer/installed-extensions) | 50K | Inconclusive | 0 / 0 / 0 / 1 / 0 | Showed an empty view |
| 64 | [Raycast Explorer](https://www.raycast.com/raycast/raycast-explorer) | 48K | Launches cleanly | 5 / 0 / 0 / 0 / 0 |  |
| 65 | [Wikipedia](https://www.raycast.com/vimtor/wikipedia) | 48K | Partly | 3 / 0 / 0 / 1 / 0 | Hands over to another application through that application's own kind of link |
| 66 | [Apple Mail](https://www.raycast.com/yug2005/mail) | 47K | Partly | 1 / 0 / 1 / 3 / 6 | Written for macOS (AppleScript, macOS programs or paths) |
| 67 | [Cursor](https://www.raycast.com/degouville/cursor-recent-projects) | 46K | Partly | 0 / 0 / 1 / 1 / 4 | Written for macOS (AppleScript, macOS programs or paths) |
| 68 | [TinyPNG](https://www.raycast.com/kawamataryo/tinypng) | 46K | Needs configuration | 0 / 3 / 0 / 0 / 0 | Has required preferences; launched with made-up values |
| 69 | [Spotify Controls](https://www.raycast.com/thomas/spotify-controls) | 46K | Partly | 0 / 0 / 1 / 16 / 5 | Showed a message that something it needs is not there |
| 70 | [Weather](https://www.raycast.com/tonka3000/weather) | 45K | Launches cleanly | 2 / 0 / 0 / 0 / 0 |  |
| 71 | [Quick Event](https://www.raycast.com/mblode/quick-event) | 44K | Partly | 0 / 0 / 1 / 0 / 0 | Written for macOS (AppleScript, macOS programs or paths) |
| 72 | [iTerm](https://www.raycast.com/ron-myers/iterm) | 44K | Partly | 0 / 0 / 2 / 5 / 4 | Written for macOS (AppleScript, macOS programs or paths) |
| 73 | [Ruler](https://www.raycast.com/anwarulislam/ruler) | 44K | Inconclusive | 0 / 0 / 0 / 1 / 0 | Showed a failure message of its own, which may be right for an empty machine |
| 74 | [Model Context Protocol Registry](https://www.raycast.com/raycast/model-context-protocol-registry) | 43K | Fails | 0 / 0 / 0 / 0 / 1 | Uses part of the Raycast API that Heyo does not have |
| 75 | [Svgl](https://www.raycast.com/1weiho/svgl) | 43K | Partly | 2 / 0 / 0 / 1 / 0 | Showed an empty view |
| 76 | [Cheatsheets](https://www.raycast.com/destiner/cheatsheets) | 43K | Launches cleanly | 1 / 0 / 0 / 0 / 0 |  |
| 77 | [Password Generator](https://www.raycast.com/joshuaiz/password-generator) | 43K | Launches cleanly | 3 / 0 / 0 / 0 / 0 |  |
| 78 | [Word Count](https://www.raycast.com/itsmingjie/word-count) | 43K | Partly | 2 / 0 / 0 / 1 / 0 | Showed a message that something it needs is not there |
| 79 | [Base64](https://www.raycast.com/DanielSinclair/base64) | 42K | Launches cleanly | 3 / 0 / 0 / 0 / 0 |  |
| 80 | [Ollama AI](https://www.raycast.com/massimiliano_pasquini/raycast-ollama) | 42K | Partly | 1 / 0 / 17 / 6 / 0 | Showed a failure message of its own, which may be right for an empty machine |
| 81 | [Things](https://www.raycast.com/loris/things) | 42K | Partly | 0 / 1 / 0 / 0 / 9 | Written for macOS (AppleScript, macOS programs or paths) |
| 82 | [Raindrop.io](https://www.raycast.com/lardissone/raindrop-io) | 42K | Needs configuration | 0 / 5 / 0 / 0 / 0 | Has required preferences; launched with made-up values |
| 83 | [Gmail](https://www.raycast.com/tonka3000/gmail) | 41K | Needs configuration | 0 / 9 / 0 / 0 / 0 | Needs a sign-in in the browser (OAuth) |
| 84 | [2FA Code Finder](https://www.raycast.com/yuercl/imessage-2fa) | 40K | Fails | 0 / 0 / 0 / 0 / 1 | Written for macOS (AppleScript, macOS programs or paths) |
| 85 | [Easy Dictionary](https://www.raycast.com/isfeng/easydict) | 40K | Partly | 1 / 0 / 1 / 2 / 1 | Written for macOS (AppleScript, macOS programs or paths) |
| 86 | [Apple Intelligence](https://www.raycast.com/EvanZhouDev/raycast-apple-intelligence) | 39K | Partly | 0 / 0 / 1 / 0 / 12 | Written for macOS (AppleScript, macOS programs or paths) |
| 87 | [Claude](https://www.raycast.com/florisdobber/claude) | 38K | Partly | 0 / 4 / 1 / 0 / 0 | Has required preferences; launched with made-up values |
| 88 | [QR Code Generator](https://www.raycast.com/Melvynx/qrcode-generator) | 38K | Launches cleanly | 3 / 0 / 0 / 0 / 0 |  |
| 89 | [Unsplash](https://www.raycast.com/eggsy/unsplash) | 38K | Partly | 0 / 2 / 2 / 0 / 0 | Written for macOS (AppleScript, macOS programs or paths) |
| 90 | [Google Meet](https://www.raycast.com/vitoorgomes/google-meet) | 37K | Partly | 0 / 0 / 2 / 0 / 2 | Written for macOS (AppleScript, macOS programs or paths) |
| 91 | [Random Data Generator](https://www.raycast.com/loris/random) | 37K | Partly | 1 / 0 / 0 / 1 / 0 | Showed a failure message of its own, which may be right for an empty machine |
| 92 | [App Cleaner](https://www.raycast.com/dziad/appcleaner) | 37K | Partly | 0 / 0 / 1 / 0 / 0 | Written for macOS (AppleScript, macOS programs or paths) |
| 93 | [UUID Generator](https://www.raycast.com/jmaeso/uuid-generator) | 36K | Partly | 4 / 0 / 0 / 5 / 0 | Turned away the made-up arguments it was launched with |
| 94 | [JetBrains Toolbox Recent Projects](https://www.raycast.com/gdsmith/jetbrains) | 34K | Partly | 0 / 0 / 1 / 1 / 0 | Showed a message that something it needs is not there |
| 95 | [Amphetamine](https://www.raycast.com/gstvds/amphetamine) | 33K | Partly | 0 / 0 / 1 / 0 / 4 | Written for macOS (AppleScript, macOS programs or paths) |
| 96 | [File Manager](https://www.raycast.com/erics118/file-manager) | 33K | Partly | 0 / 0 / 1 / 0 / 0 | Written for macOS (AppleScript, macOS programs or paths) |
| 97 | [Google Drive](https://www.raycast.com/vishaltelangre/google-drive) | 33K | Inconclusive | 0 / 0 / 0 / 1 / 0 | Showed a failure message of its own, which may be right for an empty machine |
| 98 | [Unix Timestamp](https://www.raycast.com/destiner/unix-timestamp) | 32K | Launches cleanly | 3 / 0 / 0 / 0 / 0 |  |
| 99 | [Screenshot](https://www.raycast.com/Aayush9029/screenshot) | 32K | Partly | 7 / 0 / 0 / 0 / 1 | Written for macOS (AppleScript, macOS programs or paths) |
| 100 | [Home Assistant](https://www.raycast.com/tonka3000/homeassistant) | 30K | Needs configuration | 0 / 44 / 0 / 0 / 0 | Has required preferences; launched with made-up values |

## What this cannot tell

1. A launch is not a use. Each command was started once and watched until it went quiet; no action was pressed, no form submitted. An empty list that searches as you type had “test” typed into it, and that is all the typing there was.
2. The host is a stand-in. Requests to the Heyo host (applications, clipboard, selection, windows, opening things) were answered with small made-up data and nothing was really opened, pasted or changed. What the real host does with them is not tested here.
3. No real accounts. Required preferences were filled with made-up values, sign-ins were not completed. “Needs configuration” therefore says where the test stopped, not that the extension works once configured.
4. The test machine is a bare Linux system: none of the applications that extensions drive (browsers, editors, password managers, Homebrew, Docker) is installed, and it has no desktop session. An extension that reads another application's files finds none.
5. The static check reads text. It finds AppleScript and macOS programs in a command's own code whether or not the launch reaches them; it can miss what is built at run time, and it can flag code that never runs on Linux.
6. Extensions were run for real, as Node.js programs. With bubblewrap they saw only the system's programs, the runtime and a scratch directory; that is a restriction, not a security boundary anyone should rely on, and with the network allowed they could reach it.
7. One machine, one day. Services answer differently from one day and one address to the next.

Made with `scripts/compat/run.mjs`; see [../compatibility.md](../compatibility.md).
