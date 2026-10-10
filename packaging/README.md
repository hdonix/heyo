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
- **AppImage: not bundled yet.** It is the one Linux format that is meant
  to stand alone, so it should carry one (`bundle.linux.appimage.files`,
  `"/usr/bin/node"`: beside the program is where Heyo looks first). Not
  done: it needs a download step for the right architecture.

## Linux packages

    pnpm install
    pnpm tauri build --bundles deb,rpm      # or appimage; no --bundles builds all three

Results are in `src-tauri/target/release/bundle/{deb,rpm,appimage}/`.

Dependencies: Tauri adds `libwebkit2gtk-4.1-0`, `libgtk-3-0` and
`libayatana-appindicator3-1` (the rpm: the matching `lib….so` names) by
itself. `tauri.conf.json` adds `xdg-utils` (opening things, registering the
sign-in link handler), and recommends, besides Node.js: `libnotify-bin`
(`notify-send`), `curl` (listing a local model server's models) and
`pulseaudio-utils` (`pactl`, choosing a microphone).

Verified:

- deb (`Heyo_0.1.0_arm64.deb`, 6 MiB): built; `dpkg-deb -I` shows the
  control fields above; `dpkg-deb -c` shows `usr/bin/heyo`, the three
  scripts in `usr/lib/Heyo/`, `usr/share/applications/Heyo.desktop` and
  icons at 32, 128, 256@2 and 512.
- rpm (`Heyo-0.1.0-1.aarch64.rpm`): built; the file list (read with
  `bsdtar`) is the same, and the header holds the required and recommended
  names. There is no `rpm` on the machine, so `rpm -qlp`/`rpm -qpR` and
  `rpmlint` were not run.
- AppImage (`Heyo_0.1.0_aarch64.AppImage`, 81 MiB): built; the AppDir holds
  `usr/bin/heyo`, `usr/lib/Heyo/*.cjs` and WebKitGTK's libraries. Not
  started.

Not verified: installing any of them, `lintian`, x86_64 builds (build on
an x86_64 machine: Tauri does not cross-build Linux packages), and the rpm
names on openSUSE (`nodejs-npm`, `pulseaudio-utils`, `libnotify` are
Fedora's).

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

    INSTALLER=1 scripts/windows-test/build.sh        # ARCH=x86_64 for Intel/AMD

Cross-compiles as before and then has Tauri make an NSIS installer
(`Heyo_<version>_<arch>-setup.exe`, copied to
`src-tauri/target/heyo-windows/`). It installs for the current user without
administrator rights (`bundle.windows.nsis.installMode`), fetches WebView2
if the machine lacks it, and carries `node.exe`, `libunwind.dll` and
`WebView2Loader.dll` beside `heyo.exe` (`packaging/windows/installer.conf.json`,
merged in only for this build because those files exist only once the
script has fetched or built them; Tauri by itself packs none of the three).

It needs `makensis` (NSIS 3) on `PATH`: `sudo apt install nsis`. Without
root, as was done here: `apt-get download nsis nsis-common`, unpack both
with `dpkg-deb -x` into one directory, and put on `PATH` a two-line script
named `makensis` that sets `NSISDIR` to that directory's `usr/share/nsis`
and runs its `usr/bin/makensis` (setting `NSISDIR` in the environment is
not enough: Tauri clears it before it runs `makensis`). Tauri downloads its
own NSIS plugin (`nsis_tauri_utils.dll`) on the first build.

Verified: `Heyo_0.1.0_arm64-setup.exe` (22 MiB) was built this way, from
Linux, for `aarch64-pc-windows-gnullvm`, and the script NSIS was given
lists `heyo.exe`, the three helper scripts, `node.exe`, `libunwind.dll` and
`WebView2Loader.dll`. Not verified: running the installer, what it
installs, its uninstaller, and an x86_64 build. Tauri calls building
Windows installers from another system experimental. The installer is
not signed.

Launch at login on Windows is a value named `Heyo` under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
(`src-tauri/src/platform/win/autostart.rs`), in place of the autostart
entry Linux uses. It compiles for Windows; it has not been run. The
uninstaller does not remove that value yet (an NSIS hook,
`bundle.windows.nsis.installerHooks`, would).

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
3. On x86_64 and on aarch64: `pnpm tauri build`, then for each package
   read `dpkg-deb -I`/`-c` and `rpm -qpi`/`-qpR`/`-qlp`, and run `lintian`
   and `rpmlint`.
4. Install each package on a clean machine of its kind (Ubuntu LTS, Debian
   stable, Fedora) and check: Heyo is in the applications menu with its
   icon; it starts; an extension runs (so the scripts were found and Node.js
   is used); with no Node.js, Settings → Advanced says so and the launcher
   still works; launch at login survives a sign-out; removing the package
   leaves nothing in `/usr`.
5. Start the AppImage on a distribution other than the one it was built on.
6. `INSTALLER=1 scripts/windows-test/build.sh` for each architecture;
   install on Windows, start, run an extension, turn launch at login on and
   sign in again, uninstall.
7. Upgrade over the previous release on each platform; settings,
   extensions and history must survive.
8. If updates are on: sign, publish the artefacts with their signatures and
   the JSON, and update a copy of the previous release through the button.
9. Tag the release; update the PKGBUILD's `pkgver` and checksum.
