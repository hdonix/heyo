# Heyo's documents

What each document is for.

## Using Heyo

| Document | What is in it |
|---|---|
| [../README.md](../README.md) | What Heyo is, with pictures: what it does, where it runs, how to install it, the first minute. |
| [usage.md](usage.md) | Every feature with its keys, the command line, and where Heyo keeps things. |
| [compatibility.md](compatibility.md) | Which Raycast extensions run on Heyo: how that is measured, and how to run the measurement. The last results are in [compatibility/](compatibility/README.md). |

## Writing extensions

| Document | What is in it |
|---|---|
| [developing-extensions.md](developing-extensions.md) | `heyo dev`, `heyo build`, `heyo ext check` and `heyo migrate`: writing an extension, or bringing one over from Raycast. |
| [../packages/api/README.md](../packages/api/README.md) | `@heyo/api` for its users: what it adds to `@raycast/api`, and its permissions. |
| [extension-api.md](extension-api.md) | The specification of `@heyo/api`: packaging, versions, permissions, the wire protocol, every interface. |
| [live-activities.md](live-activities.md) | The design of live activities: what each desktop offers for showing something in progress, the interface, the implementation. |

## Working on Heyo

| Document | What is in it |
|---|---|
| [architecture.md](architecture.md) | How Heyo is built: the host, the web view and the runtime, how they talk, the source tree, the platform layer. Read this before the code. |
| [development.md](development.md) | Building, running and testing Heyo; the GUI sandbox, the Wayland container and the Windows machine; debugging; installing your own build. |
| [../CONTRIBUTING.md](../CONTRIBUTING.md) | The rule about compatibility before the first release, and how commit messages are written. |
| [wayland.md](wayland.md) | What Heyo needs of a Wayland compositor, how GNOME is served through the Shell extension, and what KDE and the wlroots compositors could offer. |
| [../gnome-extension/README.md](../gnome-extension/README.md) | The GNOME Shell extension, for those who review or publish it. |
| [../packaging/README.md](../packaging/README.md) | How each package is built, what has been verified, updates, and the release checklist. |
| [../scripts/wayland-test/README.md](../scripts/wayland-test/README.md) | The GNOME-on-Wayland container: how to drive it, and what a test in it does and does not show. |
| [../scripts/windows-test/CHECKLIST.md](../scripts/windows-test/CHECKLIST.md) | What remains to be tried by hand on Windows. |
| [flutter-evaluation.md](flutter-evaluation.md) | A record of trying Flutter for the interface, what was measured, and why the web view stayed. |
| [../TODO.md](../TODO.md) | The to-do list: what is done and what is not, with priorities. |

`images/` holds the README's pictures, which
`scripts/gui-test/readme-shots.sh` takes
([development.md](development.md#the-readmes-pictures)).
