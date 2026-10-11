# Flutter for the interface: tried, and shelved (2026-10-10 to 10-11)

Conclusion: **no migration. The interface stays on Tauri's web view.** The
code of the trial is not on the main branch; it is kept on the branch
`flutter-ui`.

This document records what was tried, what was measured, why it stopped,
and where to pick it up if it is ever reconsidered.

## Why it was tried

On the development machine (NVIDIA's proprietary driver, X11) WebKitGTK
cannot use the GPU, and the interface is drawn on the CPU.
Stuttering scrolling and shadows being recomputed both come from this (see
the "Performance" section of `TODO.md`). Flutter brings its own rendering
engine, and the question was whether it could get around the problem.

## What was measured

| Item | Result | How it was measured |
|---|---|---|
| GPU | Flutter did use the GPU on this machine (the process appears in `nvidia-smi`) | The real desktop, with a test window that does not take focus |
| Holding an arrow key, 2000 rows, time of a whole frame (median / 95th percentile) | The real desktop 3.2 / 4.6 ms; the sandbox without a GPU 7.3 / 10.3 ms | Flutter's `FrameTiming` |
| The web view in the same scenario | 11–13 ms a step in the sandbox | `scripts/gui-test/scroll.sh`. The measure differs (from the frame callback to the page being idle), so only the order of magnitude can be compared |
| The island expanding and collapsing (a continuous change of shape) | The real desktop 2.8 / 3.9 ms | As above |
| Chinese input method | Works in an X11 session | Tried by the user's own hand |
| Chinese fonts | Fallback fonts have to be listed explicitly, otherwise there are empty boxes | Sandbox screenshots |
| Memory (resident / proportional), with the palette hidden | The web view 838 / 294 MB; Flutter 319 / 194 MB | The sandbox, release builds |
| Memory, with the palette and the settings window both open | The web view 1102 / 399 MB; Flutter 616 / 363 MB | As above. Every further Flutter window is about 170 MB, and with all windows open it would exceed the web view |
| From being called up to being drawn | 13–56 ms, mostly around 20 ms | The sandbox, software rendering |
| Package size | The Flutter interface is about 50 MB (27 MiB in the deb) | The build output |

Not measured: Wayland, Windows, 200% scaling, memory on the real desktop.

## Why it stopped

The gain in rendering performance is real, but the costs and the gaps
together outweigh it:

1. **The whole interface layer would have to be rewritten.** About 7000
   lines of TypeScript and 2900 lines of SCSS, the heaviest of which is the
   renderer of extension views. When the groundwork was done, the Flutter
   side had only a minimal palette.
2. **Without a web engine, the HTML and SVG in extensions cannot be
   drawn.** The detail pages of Raycast extensions are Markdown, which may
   have HTML mixed in; many extensions use an SVG generated in code as a
   canvas (the whole dashboard of Speedtest is one SVG image). WebKit
   supports all of this fully today; after a move to Flutter it would have
   to be made up for by implementing a subset, which is a lasting gap in
   compatibility.
   Embedding a web view back into Flutter does not solve it: on Linux an
   embedded web view sits on top of what Flutter draws, and overlays (the
   action panel, toasts) cannot be drawn over it.
3. **Several windows.** Stable Flutter has no multi-window interface
   (`enable-windowing` is only available on the development branch), so it
   could only be one process per window, each about 170 MB.
4. **Windows cannot be cross-compiled.** Today's Windows build is
   cross-compiled from Linux; Flutter has to be built on a Windows machine.
5. **Raycast itself went the opposite way.** For the sake of being
   cross-platform, Raycast 2.0 changed from native rendering to "a native
   shell plus the system web view".
   Our whole reason for changing engines lies in one driver on Linux.

## What is left

- **This document** and the figures above.
- **The branch `flutter-ui`**: the complete groundwork. It includes `ui/`
  (the Flutter project: a host client, the design specification, the icon
  mapping, shared components, and one palette that goes the whole way,
  with 87 tests), `src-tauri/src/shell/` (the abstraction of the interface:
  a web view window or a Flutter process; the command dispatch table;
  event forwarding), `docs/flutter-ui.md` (architecture and conventions)
  and `scripts/gui-test/flutter-shell.sh`.
  The subagent that built this groundwork passed its 26 checks in its own
  sandbox; after being merged into the main branch it was withdrawn without
  being verified again.
- **Two improvements to the sandbox scripts** (kept on the main branch):
  `HEYO_TEST_DISPLAY` can give the display number, so that several
  sandboxes can run in parallel; `down.py` only ends the processes on its
  own display, and kills them when `SIGTERM` has no effect.
- **The effect on the Dynamic Island design**: the expanded interface is
  drawn as a web page. The experiments confirmed that the rendering cost of
  the web view depends only on the area that changes and is zero when idle,
  which is enough.

## The rendering performance problem is still there

Shelving Flutter does not mean the problem is solved. Two roads have not
been seriously tried yet, both far cheaper:

1. **Getting WebKitGTK to use the GPU.** Only two configurations were tried
   (the default, and naming NVIDIA's GBM backend), and both failed because
   creating GBM buffers failed. It has not been studied systematically.
2. **Changing to a Chromium engine.** Its GPU acceleration on NVIDIA is
   mature, the front-end code needs no change, and HTML and SVG are all
   there. The price is size and memory. How mature Tauri's CEF runtime is
   has not been verified.

## If it is reconsidered later

First confirm whether these three things have changed: whether Flutter's
multi-window interface has reached the stable channel; whether there is a
way to render SVG and common HTML fully in Flutter (for example, rendering
SVG to a bitmap in the Rust host); whether the GPU problem of the web view
on Linux has been solved some other way (in which case the reason for
changing engines is gone). Then pick it up from the `flutter-ui` branch.
