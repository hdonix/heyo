# Which Raycast extensions run on Heyo

`scripts/compat/` answers that with evidence: it takes the most-downloaded
extensions of the Raycast Store, reads each with the static checker
(`heyo ext check`), launches every command in the real extension runtime
against a stand-in host, and writes down what happened.

The last run is in [compatibility/](compatibility/README.md): a summary
(`README.md`), a page with every command and what was seen of it
(`index.html`, one file, opens from disk) and the data (`results.json`).

## Running it

```
pnpm build:runtime                  # the runtime and the checker it uses
node scripts/compat/run.mjs         # the 100 most-downloaded extensions
```

Node.js 22 or newer, Linux. With [bubblewrap](https://github.com/containers/bubblewrap)
(`bwrap`) installed and usable without privileges, each launch is confined
(see [Safety](#safety)); without it the tool says so and runs the
extensions with only their environment changed.

| Option | Default | |
|---|---|---|
| `--count N` | 100 | How many extensions, the most downloaded first |
| `--only a,b` | | Only these, by their Store name, whatever their rank |
| `--cache DIR` | `<system temp>/heyo-compat` | Downloads and finished results. Never inside Heyo's own directories: the tool refuses |
| `--out DIR` | `docs/compatibility` | Where `results.json`, `index.html` and `README.md` are written |
| `--network on\|off\|both` | `on` | Whether launched commands may reach the network. `both` launches each command twice and reports both counts; `off` and `both` need bubblewrap |
| `--sandbox auto\|bwrap\|none` | `auto` | `auto` uses bubblewrap when it works here |
| `--jobs N` | 4 | Extensions launched side by side |
| `--timeout MS` | 15000 | How long a command has to settle |
| `--memory MB` | 1024 | Memory a launch may use before it is stopped |
| `--pause MS` | 500 | Pause between requests to the Store |
| `--refresh` | | Ask the Store for the listing again |
| `--force` | | Launch again what the cache has results for |
| `--report-only` | | Write the report from what the cache has; launch nothing |

A run can be stopped and started again: the listing, each download and
each extension's finished record are in the cache, and only what is
missing is done. A record is good for the same extension version, runtime
bundle, timeout, memory, network mode and sandbox; change one and that
extension is launched again. The verdicts are not in the cache: they are
worked out from the records every time the report is written, so a change
to `classify.mjs` needs `--report-only`, not a new run.

### The Store

The listing is the one the Store command in Heyo uses
(`https://www.raycast.com/api/v1/store_listings`). It gives each
extension's `download_count`, but its own order is not by downloads (and a
`sort` parameter changes nothing), so the tool reads the whole listing — 17
pages of 200 at the time of writing — and sorts it itself: **download
count, descending; ties in the Store's order**. Requests go one at a time
with a pause between them and the User-Agent Heyo's runtime sends
(`node`, Node's own, since the runtime sets none).

## What is recorded for each command

- **Static findings**: `checkExtension` from `runtime/dist/heyo-dev.cjs`,
  the same as `heyo ext check`, counted by rule for the command's own file,
  with a few to look at.
- **The API it names**: every member of `@raycast/api` the command's bundle
  reaches for (`List.Item.Detail`, `getSelectedFinderItems`, …), looked up
  in the API object the runtime really hands to extensions. That object is
  not read from Heyo's sources: a small command is launched in the runtime
  bundle and writes down what it was given. A member that is not there is
  a missing API, by name. (A minified bundle hides which top-level names it
  uses; for those only deeper members are checked.)
- **What its AppleScript speaks to**: the applications in `tell
  application "…"`, read from the bundle's text.
- **The launch**: the command is started in `runtime/dist/heyo-runtime.cjs`
  in a process of its own (`launch`, or `background-run` for a menu bar
  command) and watched until it has been quiet for a moment or its time is
  up:
  - the view on top: its root (`List`, `Grid`, `Detail`, `Form`), how many
    items, fields and actions, whether it is still loading, its empty view,
    the first few titles;
  - whether the command ended by itself; for a menu bar command, its menu;
  - toasts and HUDs with their text; an error nobody caught, with where in
    the extension it was raised and whether its stack starts in Heyo's
    runtime or in the extension; the runtime's error screen, if the view
    threw while rendering;
  - every `host.*` request, counted; the ones that would do something
    (open, paste, trash, copy, launch another command) with what they asked;
  - the process ending early, or being stopped for memory.
- An empty list that searches as you type has `test` typed into it, once.
  Nothing else is pressed.

Required preferences are filled before the launch — Heyo would otherwise
show its own form and the extension's code would not run. A text field
gets something of the shape its name suggests (`https://example.invalid`
for a URL, a directory for a path), a password gets
`heyo-compat-dummy-secret`. Arguments get `test`, or `1` where the name
suggests a number. Which preferences and arguments were made up is in the
record, and a command launched this way is never called working (below).

### The stand-in host

`host.mjs` answers what the Rust host would (`src-tauri/src/host.rs`):
five applications, a frontmost application, two windows on one monitor, a
clipboard, `selected text` as the selection, two files, two audio devices,
two processes, no AI model configured. A request that would do something
is written down and answered as done; nothing is opened, pasted, trashed
or ended. An alert (`confirmAlert`) is declined.

## Reading the result

A command gets one verdict:

| Verdict | Means |
|---|---|
| **Launches cleanly** | It showed something of its own (a list with items, a text, a form) or ran to its end, with no error and no failure message, and its code has nothing the static check knows cannot work |
| **Needs configuration** | It stops at something only its user can give: required preferences (it was launched with made-up ones), an account or key, a sign-in in the browser, an AI model |
| **Partly** | It launches, but its code holds macOS-only parts or a missing API the launch did not reach; or it showed its content and a failure message besides |
| **Inconclusive** | An empty view, still loading or nothing at all when time was up; a failure message of the extension's own (“No timers found”); a screen that says what is missing (“yt-dlp is not installed”); a hand-over to another application's own kind of link (`cleanshot://…`); a program or another application's files the test machine lacks |
| **Fails** | An error was thrown or the process ended; or it failed for a reason that is Heyo's or the platform's: macOS-only code, a missing API, an error inside the runtime |

The order matters where two apply: what was seen at the launch comes
first, what the code is known to hold second. A command that throws
`spawn osascript ENOENT` fails as macOS-only; one that shows its list and
has AppleScript in an action nobody pressed is “partly”.

and, when it is not a clean launch, one reason (`macos-only`,
`missing-api`, `heyo-runtime`, `extension-error`, `needs-oauth`, …; the
full list with its wording is `REASONS` in `classify.mjs`).

An extension **launches cleanly** when all its commands do, **needs
configuration** when each either launches cleanly or stops at
configuration, **fails** when none gets anywhere, and is **partly**
working for every mix in between.

“Launches cleanly” is the most this method can say. It is not “works”:
only the first screen was seen.

The table “What stands in the way” counts, for each blocker, the extensions
it holds back and those for which it is the *only* thing in the way — what
fixing that one thing would turn.

### What it cannot tell

- A launch is not a use: no action was pressed, no form submitted.
- The host is a stand-in. What the real host does with a request is not
  tested here.
- No real accounts: “needs configuration” says where the test stopped, not
  that the extension works once configured.
- The test machine has none of the applications extensions drive, and no
  desktop session. An extension that reads another application's files
  finds none and is “inconclusive”, not “fails”.
- The static check reads text: it flags AppleScript in a command's code
  whether or not anything reaches it, and misses what is put together at
  run time.
- A failure message with nothing to name its cause, in a command whose
  code calls AppleScript, is counted as macOS-only and marked “probably”.
  That is a guess, and the report says so where it makes it.
- One machine, one day; services answer differently tomorrow.

## Safety

These are third-party programs and they are run for real.

- **Environment.** A launch gets a fresh home directory under the cache
  and an environment written from scratch: `HOME`, the XDG directories and
  Heyo's own (`HEYO_DATA_DIR`, …) point into it; there is no `DISPLAY`, no
  session bus address, no SSH agent, no token of any kind.
- **Bubblewrap**, when available: the process sees the system's programs
  and libraries read-only, the runtime bundle, the extension's copy and its
  scratch directory — not the user's home, not the repository. It has its
  own process, IPC and user namespaces, and with `--network off` no
  network.
- **The host does nothing** (above).
- **Limits.** A deadline, a memory cap (Node's heap limit, and the whole
  process tree's resident memory watched from outside), one process per
  launch, killed at the end with everything it started.

- **What is published.** Before a record is kept, the run's directories
  and anything shaped like a network or hardware address are taken out of
  its texts: with the network allowed an extension sees the machine's
  interfaces and its public address, and one of the hundred puts them on
  screen. Read the page before publishing it all the same; an extension
  can show anything it can find out (a city from a weather service, say).

This is not a sandbox to rely on. With the network allowed — the default,
because most extensions are front ends to a service — an extension can
send and fetch what it likes; with bubblewrap it has little to send.
**Without bubblewrap it runs as you**, with only its environment changed,
and can read whatever you can: do not run the tool that way on a machine
with anything on it you would not hand to a hundred strangers' code.

## The pieces

| File | |
|---|---|
| `run.mjs` | The command line: picks the sample, runs the launches, writes the report |
| `store.mjs` | The Store's listing and downloads, cached and paced |
| `drive.mjs` | Launches one command in the runtime and records the run |
| `host.mjs` | The stand-in host |
| `observe.mjs` | Reads the runtime's messages: the view, the toasts, the stack of an error |
| `preferences.mjs` | Required preferences and arguments, and made-up values for them |
| `api.mjs` | The API members a bundle names, the ones the runtime lacks, and what its AppleScript speaks to |
| `check-worker.mjs` | Runs the static checker and the reading of bundles in a process of their own |
| `sandbox.mjs` | The environment and the bubblewrap arguments |
| `classify.mjs` | The verdicts, and the ranking of blockers |
| `report.mjs` | `index.html` and `README.md` from the results |

The static check has two minutes per extension. If it does not finish
(it is slow on minified bundles) the extension is launched all the same and
its record says that it has no static findings.

The parts that decide something (`classify`, `host`, `observe`,
`preferences`, `api`, `sandbox`, the options) are plain functions with tests
beside them (`*.test.ts`); `drive.test.ts` launches small extensions in
the real runtime bundle and is skipped when it has not been built.
