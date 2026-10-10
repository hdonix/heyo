//! Development only: how smoothly the list on screen scrolls.
//!
//! Ctrl+Alt+Shift+B scrolls the tallest scrolling area down and back up, a
//! little every frame, and reports how long the frames took to the dev
//! server (`/__bench`). A frame that takes longer than the display's
//! interval is one the eye sees as a stutter.

function scroller(): HTMLElement | undefined {
  const candidates = [...document.querySelectorAll<HTMLElement>("*")].filter((element) => {
    const overflow = getComputedStyle(element).overflowY;
    return (overflow === "auto" || overflow === "scroll") && element.scrollHeight > element.clientHeight + 40;
  });
  return candidates.sort((a, b) => b.scrollHeight - b.clientHeight - (a.scrollHeight - a.clientHeight))[0];
}

function run(label: string) {
  const target = scroller();
  if (!target) return report(label, "nothing scrolls");
  const frames: number[] = [];
  const span = target.scrollHeight - target.clientHeight;
  const DURATION = 3000;
  const started = performance.now();
  let last = started;
  const tick = (now: number) => {
    frames.push(now - last);
    last = now;
    const progress = (now - started) / DURATION;
    if (progress >= 1) {
      target.scrollTop = 0;
      return summarize(label, frames.slice(1), span);
    }
    // Down, then back up.
    target.scrollTop = span * (progress < 0.5 ? progress * 2 : 2 - progress * 2);
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

function summarize(label: string, frames: number[], span: number) {
  const sorted = [...frames].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))];
  const mean = frames.reduce((sum, f) => sum + f, 0) / frames.length;
  const text = [
    `frames=${frames.length}`,
    `mean=${mean.toFixed(1)}ms`,
    `median=${at(0.5).toFixed(1)}ms`,
    `p95=${at(0.95).toFixed(1)}ms`,
    `worst=${sorted[sorted.length - 1].toFixed(1)}ms`,
    `over20ms=${frames.filter((f) => f > 20).length}`,
    `scrolled=${span}px`,
  ].join(" ");
  report(label, text);
}

function report(label: string, text: string) {
  void fetch(`/__bench?${encodeURIComponent(`${label}: ${text}`)}`);
}

/// Ctrl+Alt+Shift+N watches the selected row for three seconds while keys
/// move the selection (the test script holds an arrow key). Seen from the
/// window, a selection walking down a list should only ever move down or
/// stand still at the edge; every time it goes the other way is a jolt.
function watchSelection(label: string) {
  // Where the time of a key press goes: the script that handles it (until
  // the frame's callbacks run), then style, layout and paint (until the
  // page is free again).
  const script: number[] = [];
  const render: number[] = [];
  const onKey = (event: KeyboardEvent) => {
    if (!event.key.startsWith("Arrow")) return;
    pressed = performance.now();
  };
  // After the handlers of the press, and the rendering React does in a
  // microtask once they return.
  const afterKey = (event: KeyboardEvent) => {
    if (!event.key.startsWith("Arrow")) return;
    const from = pressed;
    queueMicrotask(() =>
      queueMicrotask(() => {
        script.push(performance.now() - from);
        requestAnimationFrame(() => {
          const frame = performance.now();
          setTimeout(() => render.push(performance.now() - frame));
        });
      }),
    );
  };
  let pressed = 0;
  window.addEventListener("keydown", onKey, true);
  window.addEventListener("keydown", afterKey);
  const median = (values: number[]) => ([...values].sort((a, b) => a - b)[Math.floor(values.length / 2)] ?? 0).toFixed(1);
  const tops: number[] = [];
  const frames: number[] = [];
  const started = performance.now();
  let last = started;
  const tick = (now: number) => {
    frames.push(now - last);
    last = now;
    const mark = document.querySelector<HTMLElement>(".selection-indicator, .grid-item.selected, .action-row.selected");
    if (mark) tops.push(Math.round(mark.getBoundingClientRect().top));
    if (now - started < 3000) return requestAnimationFrame(tick);
    let back = 0;
    let furthest = 0;
    let steps = 0;
    for (let i = 1; i < tops.length; i++) {
      const moved = tops[i] - tops[i - 1];
      if (moved !== 0) steps++;
      if (moved < 0) {
        back++;
        furthest = Math.max(furthest, -moved);
      }
    }
    const slow = frames.slice(1).filter((f) => f > 20).length;
    window.removeEventListener("keydown", onKey, true);
    window.removeEventListener("keydown", afterKey);
    report(
      label,
      `frames=${tops.length} moves=${steps} backward=${back} furthestBack=${furthest}px over20ms=${slow} keys=${script.length} script=${median(script)}ms render=${median(render)}ms`,
    );
  };
  requestAnimationFrame(tick);
}

let runs = 0;
window.addEventListener(
  "keydown",
  (event) => {
    if (event.ctrlKey && event.altKey && event.shiftKey && event.code === "KeyB") {
      event.preventDefault();
      event.stopPropagation();
      run(`run ${++runs}`);
    }
    if (event.ctrlKey && event.altKey && event.shiftKey && event.code === "KeyN") {
      event.preventDefault();
      event.stopPropagation();
      watchSelection(`run ${++runs}`);
    }
  },
  true,
);
export {};
