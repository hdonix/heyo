# Packaging Heyo

How each thing a user can install is built, what has actually been tried,
and what has to happen for a release. "Verified" below means on the
development machine: aarch64, Ubuntu 24.04. Nothing here has been installed
or started from a package: the packages were built and their contents read.

## What a Heyo installation is

| What | From source (`scripts/install.sh`) | deb, rpm, AUR | Windows |
| --- | --- | --- | --- |
| the program | `~/.local/bin/heyo` | `/usr/bin/heyo` | `heyo.exe` |
| helper scripts `heyo-*.cjs` | `~/.local/lib/Heyo/` | `/usr/lib/Heyo/` | beside `heyo.exe` |
| desktop entry | `~/.local/share/applications/heyo.desktop` | `/usr/share/applications/Heyo.desktop` | Start menu shortcut |
| icons | `~/.local/share/icons/hicolor/…/heyo.png` | `/usr/share/icons/hicolor/…/heyo.png` | in the executable |

The program finds its scripts in `../lib/Heyo` from where it is (Tauri's
resource directory), on Windows in its own directory. The scripts are
declared once, under `bundle.resources` in `src-tauri/tauri.conf.json`; the
desktop entry once, in `packaging/linux/heyo.desktop` (the packages and
`install.sh` both fill it in).

**The program and its scripts must come from the same build.** Each script
starts with `// heyo-helper <version>` (`runtime/build.mjs`, from the
version in `tauri.conf.json`); the program reads that line before starting
a script and refuses one of another version, naming both and the directory
(`src-tauri/src/helper.rs`). It compares versions, not builds: two different
builds of the same version are not told apart.

### Node.js

Extensions, dictation and text recognition need Node.js 22 or newer; the
rest of the launcher works without. Heyo uses, in order: `HEYO_NODE`, the
path in its settings, a `node` beside its own executable, `node` on `PATH`
(`settings.rs`), and its settings page says when what it found is missing
or too old.

- **deb / rpm: recommended, not required, not bundled.** Ubuntu 24.04 has
  Node.js 18 and Debian 13 has 20, so `Depends: nodejs (>= 22)` would make
  the package impossible to install exactly where most users are, and
  would not see a Node.js from nodejs.org, nvm or fnm at all. Bundling one
  adds about 100 MB to a 6 MB package and a second Node.js to keep patched.
  So: `Recommends: nodejs (>= 22), npm` (text recognition installs its
  engine with `npm`), which a distribution that has it installs by default.
- **AUR: required.** Arch's `nodejs` is current.
- **Windows: bundled.** There is no package manager to lean on; the
  installer carries `node.exe` beside `heyo.exe`, as the test build always
  has.
- **AppImage: bundled.** It is the one Linux format that is meant to stand
  alone, so it carries one at `usr/bin/node`: beside the program is where
  Heyo looks first (`packaging/linux/appimage.conf.json`). It makes the
  AppImage 119 MiB where it was 81.

Which Node.js those two carry is said once, in `packaging/node.json`: the
version, and what each download from nodejs.org must hash to (from its
`SHASUMS256.txt`). `scripts/package/node.mjs` fetches it, keeps it in
`~/.cache/heyo-build/`, and uses nothing that hashes differently. To move to
another version, change the version and the four hashes together.

## Building and packaging: `scripts/package/`

A package is made in two steps, which need not happen on the same machine:

    scripts/package/build.sh <target>…           # compiles, packages nothing
    scripts/package/bundle.sh <target> [bundles]  # packages, compiles nothing

| Target | Packages | Compiled from Linux with |
| --- | --- | --- |
| `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` | deb, rpm, AppImage | the system's compiler; for the other processor, llvm-mingw's clang and that architecture's libraries in a sysroot (`scripts/package/sysroot.sh arm64`, or `amd64`) |
| `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc` | NSIS installer | llvm-mingw's clang and lld (`scripts/package/toolchain.sh` fetches a pinned release), against Microsoft's headers and libraries (`scripts/package/winsdk.sh` fetches them, pinned) |

Windows is compiled as Microsoft's own tools would compile it (the `msvc`
targets), because Skia, which Slint draws Heyo's settings with
(`docs/slint-settings.md`), comes prebuilt for those and for no other kind
of Windows target, and cannot be built for another.
No tool of Microsoft's is run: llvm-mingw's clang compiles as Microsoft's
compiler does when it is called `clang-cl`, and its lld links as
Microsoft's linker does. What no compiler brings are the headers and
libraries of Microsoft's C and C++ runtime and of the Windows SDK, which
only Microsoft has. `winsdk.sh` fetches them with
[xwin](https://github.com/Jake-Shadle/xwin) into
`~/.cache/heyo-build/xwin` (1.1 GB laid out).

**Fetching them accepts Microsoft's licence terms** for them, those of
Visual Studio (<https://go.microsoft.com/fwlink/?LinkId=2086102>). xwin
asks before it fetches anything, and `XWIN_ACCEPT_LICENSE=true` in the
environment is the answer given beforehand; `winsdk.sh` does not answer
for whoever runs it. The project's owner accepted the terms for Heyo's
builds on 12 October 2026, which is what the workflow's setting of that
variable stands on.

What is fetched is the same every time: xwin at a version written in the
script, and the files that one manifest of Visual Studio's names (Visual
Studio 2022 17.14.41: the runtime 14.44, the Windows SDK 10.0.26100). A
version number for the SDK and the runtime would not pin as much, because
Microsoft publishes changed files under an unchanged number, and the
manifest's usual address gives a new one every few weeks. So the script
takes the manifest from the address of that one release and checks its
SHA-256; checks the SHA-256 of the list of packages the manifest points
to, which xwin itself does not; and xwin checks every file against the
SHA-256 that list gives for it. The script's header says how to move to a
newer manifest.

Linux for the other processor is not compiled against libraries installed
on the machine: Ubuntu's development packages for two architectures cannot
be installed side by side (WebKitGTK's and others conflict). `sysroot.sh`
instead downloads them, and all they depend on, with an apt that knows only
the other architecture, and unpacks them into a directory of its own, from
Ubuntu 22.04 whatever the machine runs; it needs no root.

`build.sh` takes any number of targets and compiles them into one target
directory, so that what is compiled for the building machine itself (build
scripts, macros) is compiled once for all, and one cache serves them all.
It leaves each program in `src-tauri/target/<target>/release/`. `bundle.sh`
needs only that directory's program and `runtime/dist/heyo-*.cjs`: Tauri's
`bundle` command packs what is already built, and what a package carries
besides (a Node.js, and for Windows the DLLs the program asks for) the
script fetches. The packages are in
`src-tauri/target/<target>/release/bundle/` and copied together to
`src-tauri/target/packages/`.

One thing cannot be done from another processor: an AppImage is made of the
libraries of the machine that makes it, so the arm64 AppImage is made on
arm64 and the x86_64 one on x86_64. The deb, the rpm and the Windows
installers can be made anywhere.

### In GitHub Actions: `.github/workflows/package.yml`

    gh workflow run package.yml            # the packages, left on the run to download
    git tag v0.0.1 && git push origin v0.0.1     # the same, and a draft release

One job on x86_64 Ubuntu 22.04 (the oldest with WebKitGTK 4.1: a program
runs where the C library is at least as new as where it was built) compiles
all four targets with `build.sh`; four jobs then make the packages with
`bundle.sh` from its programs, the arm64 Linux one on an arm64 machine. For
a tag, the tag must be the version in `tauri.conf.json`, and the packages
with a `SHA256SUMS` become a draft release: nothing is published until
someone publishes it.

The compiling job keeps one cache of the dependencies for all four targets.
A run for a tag can read the cache of the default branch but not another
tag's, so the job also runs, alone, for a push to master that changes
`Cargo.lock` or the packaging scripts: the cache a release starts from is
then never older than the dependencies. The compiler with the arm64
libraries, and Microsoft's headers and libraries, are two caches more,
each fetched again only when the script that fetches it changes. The step
that fetches Microsoft's sets `XWIN_ACCEPT_LICENSE`, and says there whose
acceptance of the terms that is: somebody who runs the workflow in a fork
accepts them for themselves by it.

Skia's prebuilt library is fetched while compiling, by the build script of
the crate that binds it, with `curl`, from a release of
`rust-skia/skia-binaries` on GitHub named by the crate's version, the
target and the features. Nothing has to be installed for it: no clang, no
Python, no ninja (those are for building Skia from source, which is what
the crate turns to if the download fails). It checks no hash of what it
fetched.

Verified (11 October 2026), when Windows was still compiled for
`*-pc-windows-gnullvm`: the workflow made all eight packages. The
compiling job took 21 minutes with no cache and 13 with one (what is left
is each target's own final optimisation, which no cache holds); each
packaging job one to three. Of what it made, on arm64 Ubuntu 24.04: the
program in the arm64 deb, which was compiled on x86_64, starts, shows the
palette and runs a command in the runtime; the arm64 AppImage does the
same through its own launcher and carries Node.js 24. The deb for x86_64
has the control fields and the files it should. Not verified: starting
anything for x86_64.

**Not run since Windows moved to the `msvc` targets** (12 October 2026):
the workflow was changed for them and read through (`actionlint` finds
nothing), and the same scripts made all four programs and both installers
on the development machine, which is arm64. What only a run shows: that
`winsdk.sh` fetches on a runner (it was not run to the end anywhere: the
files on the development machine were fetched before the script was
written, and are what it pins), that x86_64 llvm-mingw compiles for
`msvc` as the arm64 one does, how large the cache of Microsoft's files is
and whether the runner's disk holds everything, and how long it all takes.

## Linux packages

    scripts/package/build.sh aarch64-unknown-linux-gnu      # or x86_64-…
    scripts/package/bundle.sh aarch64-unknown-linux-gnu deb,rpm     # no list: all three

Dependencies: Tauri adds `libwebkit2gtk-4.1-0`, `libgtk-3-0` and
`libayatana-appindicator3-1` (the rpm: the matching `lib….so` names) by
itself. `tauri.conf.json` adds `xdg-utils` (opening things, registering the
sign-in link handler), and recommends, besides Node.js: `libnotify-bin`
(`notify-send`), `curl` (listing a local model server's models) and
`pulseaudio-utils` (`pactl`, choosing a microphone).

With Skia compiled in, the program is linked with three libraries more:
`libstdc++.so.6`, `libfontconfig.so.1` and `libfreetype.so.6`. No package
names them; WebKitGTK's package, which Heyo's depends on, depends on all
three (read on Ubuntu 24.04), so they are there where Heyo can be
installed. Skia's prebuilt library asks for nothing newer than Ubuntu
22.04 has (glibc 2.35, `GLIBCXX_3.4.30`): the newest symbols the programs
with it need are `GLIBC_2.35`, `GLIBCXX_3.4.26` and `CXXABI_1.3.9`
(`objdump -T`, 12 October 2026: the x86_64 program linked against the
22.04 sysroot; the arm64 one linked on the development machine's Ubuntu
24.04, where Rust's own library refers besides, weakly, to two symbols of
`GLIBC_2.39`, `pidfd_spawnp` and `pidfd_getpid`, which a program linked
against 22.04, as the packages are, does not). Both link with Skia in the
arrangement above, the x86_64 one from arm64. With Skia and
the fonts the arm64 deb is 28 MiB and the AppImage 139; what is said of
their sizes below was measured before.

Verified:

- deb (`Heyo_0.1.0_arm64.deb`, 6 MiB): built; `dpkg-deb -I` shows the
  control fields above; `dpkg-deb -c` shows `usr/bin/heyo`, the three
  scripts in `usr/lib/Heyo/`, `usr/share/applications/Heyo.desktop` and
  icons at 32, 128, 256@2 and 512.
- rpm (`Heyo-0.1.0-1.aarch64.rpm`): built; the file list (read with
  `bsdtar`) is the same, and the header holds the required and recommended
  names. There is no `rpm` on the machine, so `rpm -qlp`/`rpm -qpR` and
  `rpmlint` were not run.
- AppImage (`Heyo_0.1.0_aarch64.AppImage`, 119 MiB): built; the AppDir holds
  `usr/bin/heyo`, `usr/bin/node` (which runs and says its version),
  `usr/lib/Heyo/*.cjs` and WebKitGTK's libraries. Not started.

Not verified: installing any of them, `lintian`, and the rpm names on
openSUSE (`nodejs-npm`, `pulseaudio-utils`, `libnotify` are Fedora's).

Known gap:

- The identifier `dev.heyo.app` ends in `.app`, which Tauri warns about
  (it only matters on macOS).

## From source: `scripts/install.sh`

    scripts/install.sh --build     # or: pnpm tauri build --no-bundle && scripts/install.sh
    scripts/install.sh --uninstall

Installs into `~/.local` (or `$PREFIX`) what the table above lists, each
file put in place by rename so that a running Heyo is not disturbed, and
refuses to install scripts whose version line is not the one in
`tauri.conf.json`. It does not restart Heyo. `--uninstall` also removes the
two entries Heyo writes itself (autostart, the `raycast://` link handler)
and keeps settings and data.

Verified with `HOME` pointed at an empty directory: install, install
again, the files and their modes, uninstall, uninstall again, a stale
script refused. Not run against a real home.

## Arch (AUR): `aur/PKGBUILD`

Builds from a release's source archive with `pnpm tauri build --no-bundle`
and lays the files out as the deb does. **Never run**: there is no
`makepkg` here. Before publishing: fill in `source` and the checksum
(marked TODO; there is no release archive to point at yet), build in a
clean chroot, run `namcap`, and
generate `.SRCINFO`. Unchecked guesses in it: the package names
`webkit2gtk-4.1`, `libayatana-appindicator`, `libpulse` (for `pactl`), and
that `pnpm install --frozen-lockfile` is content with `pnpm-workspace.yaml`
as it stands (pnpm 11 here stops over an unapproved build script of
`@parcel/watcher` until that file says `false` for it).

## Flatpak: `flatpak/dev.heyo.app.yml`

**Not viable as Heyo stands.** The manifest is a starting point that has
never been built (no `flatpak-builder` here). It repackages the deb and
puts the SDK's Node.js 22 beside the program. A launcher's job is to reach
out of itself, and the sandbox is in the way of nearly all of it:

| Heyo does | In a Flatpak |
| --- | --- |
| Starts applications from their `.desktop` files | They are not in the sandbox. Needs `flatpak-spawn --host` for every launch, which needs `--talk-name=org.freedesktop.Flatpak`: permission to run anything on the host, i.e. no sandbox. Code change in `open.rs`. |
| Reads `.desktop` files and icons from `/usr/share`, `/var/lib/flatpak`, `/snap` | The host's `/usr` appears as `/run/host/usr` (with `--filesystem=host-os`). `apps.rs`, icon lookup and the asset protocol's scope in `tauri.conf.json` all name the usual places. |
| Global hotkeys on GNOME: writes custom keybindings with `gsettings` that run `heyo …` | `gsettings` in the sandbox writes a private dconf, not the user's; reaching the real one needs dconf's socket and files opened up. The command to bind would be `flatpak run dev.heyo.app …`, not the path of the program. |
| Global hotkeys on X11, reading the selection, pasting into other applications | Work with `--socket=x11`, which is itself no isolation on X11. On Wayland they go through Heyo's GNOME Shell extension over D-Bus: needs `--talk-name=org.gnome.Shell`. |
| Installs its GNOME Shell extension into `~/.local/share/gnome-shell/extensions` | Needs that directory opened for writing. |
| Runs extensions in the host's Node.js | The host's is invisible; one must be bundled (the manifest does). Extensions that run the user's programs (`git`, `docker`, `code`, …) find none of them. |
| `notify-send`, `pactl`, `curl`, `gnome-screenshot`, `systemd-inhibit`, `xdg-user-dir`, `xdg-mime` | Mostly absent from the runtime; each needs a portal or D-Bus call instead (notifications, screenshots and inhibiting have portals). |
| Launch at login: writes `~/.config/autostart/heyo.desktop` with its own path | `/app/bin/heyo` means nothing outside. Needs the Background portal. |
| Registers `heyo-links.desktop` for `raycast://` sign-in links, with its own path | Same: belongs in the exported desktop entry's `MimeType` instead. |
| Search Files, and extensions reading the user's files | Needs `--filesystem=home` or wider. |
| Keeps data in `~/.local/share/heyo`, `~/.config/heyo` | Moves to `~/.var/app/dev.heyo.app/…`; an existing installation's data is not found. |

With every permission that list calls for, nothing is left of the sandbox,
and five of the rows still need code. Also unchecked: that the GNOME
runtime named in the manifest (48) is still supported and still ships
WebKitGTK 4.1; if not, WebKitGTK has to be built as a module. Flathub
would further require building from source offline. Recommendation: do not
offer a Flatpak until launching and hotkeys go through portals.

## Windows installer

    scripts/package/toolchain.sh                         # once: the compiler
    scripts/package/winsdk.sh                            # once: Microsoft's headers and libraries (see above for their terms)
    scripts/package/build.sh aarch64-pc-windows-msvc     # or x86_64-…
    scripts/package/bundle.sh aarch64-pc-windows-msvc

(`INSTALLER=1 scripts/windows-test/build.sh` does the last two and copies
the result where the test machine sees it.) Tauri makes an NSIS installer,
`Heyo_<version>_<arch>-setup.exe`. It installs for the current user without
administrator rights (`bundle.windows.nsis.installMode`), into
`%LOCALAPPDATA%\Heyo`, and fetches WebView2 if the machine lacks it.

Beside `heyo.exe` it carries what Tauri packs by itself (the helper
scripts) and what the scripts here fetch and name to it:

- **`node.exe`**, always (`packaging/windows/installer.conf.json`, merged
  in only for this step because the file exists only once
  `scripts/package/node.mjs` has fetched it).
- **The DLLs of Microsoft's Visual C++ runtime that this `heyo.exe` asks
  for**: `msvcp140.dll` and `vcruntime140.dll`, and for x86_64
  `vcruntime140_1.dll` as well, when Skia is compiled in; none when it is
  not. Skia comes prebuilt against the C++ library as a DLL, a Windows
  that never had the Visual C++ redistributable installed has not got it,
  and a program finds a DLL in its own directory first. Which ones is not
  said by a flag: `scripts/package/dlls.sh` reads it from the program
  (`llvm-objdump -p`: what it imports, and what those import in turn;
  `heyo.exe` itself imports only `msvcp140.dll`, having the C runtime
  linked in), and `scripts/package/vcruntime.sh` fetches them from Visual
  Studio's own package of redistributable files, at a release and by
  hashes written there. They are Microsoft's files, distributed with Heyo
  under the terms Microsoft gives for exactly that (`vcruntime.sh` names
  them); `bundle.sh` hands their names to Tauri as resources.

A program built for the `msvc` targets needs nothing else beside it: the
WebView2 loader is linked in, and there is no unwinder's DLL.

It needs `makensis` of NSIS 3.09 or newer on `PATH` (the script Tauri gives
it includes a file older ones lack): `sudo apt install nsis` on Ubuntu 24.04;
22.04's is 3.08. Without
root, as was done here: `apt-get download nsis nsis-common`, unpack both
with `dpkg-deb -x` into one directory, and put on `PATH` a two-line script
named `makensis` that sets `NSISDIR` to that directory's `usr/share/nsis`
and runs its `usr/bin/makensis` (setting `NSISDIR` in the environment is
not enough: Tauri clears it before it runs `makensis`). Tauri downloads its
own NSIS plugin (`nsis_tauri_utils.dll`) on the first build. `dlls.sh`
needs `llvm-objdump`: llvm-mingw's where that is, or else one on `PATH`
(`sudo apt install llvm`).

Verified (12 October 2026), with Skia compiled in
(`FEATURES=slint-skia`), from arm64 Linux: both programs compile and link;
`Heyo_0.1.0_arm64-setup.exe` (36 MiB) and `Heyo_0.1.0_x64-setup.exe`
(41 MiB) were made, and the script NSIS was given for each lists
`heyo.exe`, the five helper scripts, `node.exe`, `msvcp140.dll` and
`vcruntime140.dll`, and for x64 `vcruntime140_1.dll`. Most of an
installer's size is Node.js, and most of `heyo.exe`'s 45 MB the fonts in
it. Built without Skia, the arm64 installer (23 MiB) lists no DLL, and
those an earlier build had left in the directory they are gathered in
were removed from it. Earlier the same day an arm64 `heyo.exe` built this way, with the two
DLLs beside it and not installed, started on a Windows 11 without the
Visual C++ redistributable
(`experiments/overlay-bench/results/frameworks/slint/NOTES.md`). Not
verified here: running an installer, what it installs, its uninstaller,
and anything of x86_64 at run time: `scripts/windows-test/installer.sh`
puts an installer in front of the test machine with a test that does the
first three (`scripts/windows-test/in/installer/`). Tauri calls building
Windows installers from another system experimental. The installer is not
signed.

Launch at login on Windows is a value named `Heyo` under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
(`src-tauri/src/platform/win/autostart.rs`), in place of the autostart
entry Linux uses. It compiles for Windows; it has not been run. The
uninstaller removes a value of that name there (it is in the script Tauri
gives NSIS, for the product's name); that has not been run either.

Known gap, by reading: the installer's directory, `%LOCALAPPDATA%\Heyo`,
is the directory Heyo keeps its cache and its log in on Windows,
`%LOCALAPPDATA%\heyo` (`src-tauri/src/paths.rs`; Windows does not tell the
two names apart). The program's files and what it caches are then in one
directory, and the uninstaller, which removes its own files and the
directory only if it is empty, leaves it.

Not done: code signing (unsigned installers meet SmartScreen), an MSI
(needs WiX, which runs only on Windows).

## Updates

Heyo can update itself with `tauri-plugin-updater`, and by default does
not: the plugin is an optional dependency behind the cargo feature
`updater`, which no ordinary build turns on, so it is not even compiled.
A build has an updater only when both are true:

1. it is built with `--features updater`, and
2. its configuration has a `plugins.updater` section (endpoint and public
   key), merged in at build time.

Then, in a copy that owns its files (the AppImage, a Windows
installation), Settings → About shows **Check for Updates**, which asks the
endpoint only when clicked, and **Install … and Restart**. Packages are
left to the package manager and source builds to `install.sh`
(`src-tauri/src/updates.rs`). There is no check in the background.

There is no release server and there are no signing keys; none were made.
To turn it on:

1. `pnpm tauri signer generate -w ~/.tauri/heyo.key`. Keep the private key
   and its password out of the repository; losing it means existing
   installations can never be updated again.
2. Copy `packaging/updater.example.json` somewhere outside the repository's
   tracked files, and put in the public key and the real endpoint. The
   endpoint serves Tauri's update JSON (`version`, `url`, `signature` per
   platform); a static `latest.json` beside the release files is enough.
3. Build with the private key in the environment:

       TAURI_SIGNING_PRIVATE_KEY=… TAURI_SIGNING_PRIVATE_KEY_PASSWORD=… \
         pnpm tauri build --bundles appimage --features updater --config /path/to/updater.json

   (`createUpdaterArtifacts` in that file makes Tauri write the `.sig`
   files to publish. For Windows, `scripts/windows-test/build.sh` would
   need to pass the same two options.)
4. Publish the artefacts, their `.sig` files and the JSON.

Verified: Heyo compiles (`cargo check`) with and without the feature, for
Linux and for Windows. Not verified: anything at run time (no endpoint or
key exists to try against), a release build or a bundle with the feature,
and the webview's button, which nobody has seen. The feature adds 36 crates
(an HTTP client with TLS, archive readers, signature checking) to
`Cargo.lock`, compiled only with the feature.

## Release checklist

1. One version everywhere: `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`,
   `package.json`, `packaging/aur/PKGBUILD`, the deb names in
   `packaging/flatpak/dev.heyo.app.yml`. The one in `tauri.conf.json` is
   what the program and its scripts compare.
2. All of the verification in the repository's rules passes (`cargo test`,
   both `tsc`, `vitest`, the Windows `cargo check`).
3. Run the packaging workflow by hand (`gh workflow run package.yml`) and
   download its packages; for each read `dpkg-deb -I`/`-c` and
   `rpm -qpi`/`-qpR`/`-qlp`, and run `lintian` and `rpmlint`.
4. Install each package on a clean machine of its kind (Ubuntu LTS, Debian
   stable, Fedora) and check: Heyo is in the applications menu with its
   icon; it starts; an extension runs (so the scripts were found and Node.js
   is used); with no Node.js, Settings → Advanced says so and the launcher
   still works; launch at login survives a sign-out; removing the package
   leaves nothing in `/usr`.
5. Start the AppImage on a distribution other than the one it was built on.
6. Install each Windows installer on Windows of its processor, start, run
   an extension, turn launch at login on and sign in again, uninstall.
7. Upgrade over the previous release on each platform; settings,
   extensions and history must survive.
8. If updates are on: sign, publish the artefacts with their signatures and
   the JSON, and update a copy of the previous release through the button.
9. Tag the release (`v` and the version) and push the tag: the workflow
   drafts the release with the packages and their hashes. Read it, publish
   it, and update the PKGBUILD's `pkgver` and checksum.
