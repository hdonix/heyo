// The screenshot overlay: a fullscreen, transparent window. The frozen screen
// underneath is painted natively by the window; this draws what goes on top.
// Drag to select a region, or click a window to select exactly that,
// optionally annotate it, then copy, save, pin or extract its text. Also the
// small always-on-top window of a pinned capture.

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ArrowUpRight, Copy, Download, Grid3x3, Pen, Pin, ScanText, Square, Type, Undo2, X } from "lucide-react";
import { event } from "../shared/bindings/events";
import { pixelRatio, toOverlay, windowAt, type Rect } from "./screenshotWindows";

interface Capture {
  path: string;
  /// `pick`: an extension asked for the area; selecting it is all there is to do.
  mode: "capture" | "text" | "pick";
  x: number;
  y: number;
  scale: number;
  /// Size of the capture in physical pixels.
  width?: number;
  height?: number;
  /// The windows in it, topmost first, in physical pixels from its corner.
  windows?: Rect[];
}

type Tool = "select" | "rect" | "arrow" | "pen" | "text" | "mosaic";

/// An annotation, in logical pixels of the screen.
interface Shape {
  tool: Exclude<Tool, "select">;
  color: string;
  points: [number, number][];
  text?: string;
}

/// Where annotations are drawn: a canvas showing the screen from `origin`,
/// `scale` canvas pixels per logical pixel.
interface Surface {
  ctx: CanvasRenderingContext2D;
  scale: number;
  origin: [number, number];
  image: HTMLImageElement;
}

const HANDLES = ["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const;
type Handle = (typeof HANDLES)[number];
const TEXT_SIZE = 18;
const TEXT_LINE = 24;
const MOSAIC_BLOCK = 9;
const TOOLBAR_WIDTH = 436;

const COLORS = ["#ff453a", "#ffc531", "#59d499", "#56c2ff"];
const MIN_SIZE = 6;

function normalize(a: [number, number], b: [number, number]): Rect {
  return { x: Math.min(a[0], b[0]), y: Math.min(a[1], b[1]), width: Math.abs(a[0] - b[0]), height: Math.abs(a[1] - b[1]) };
}

/// Blurs a region into blocks, sampling the capture itself.
function mosaic({ ctx, scale, origin, image }: Surface, rect: Rect) {
  // Not loaded yet: the next redraw will have it.
  if (rect.width < 1 || rect.height < 1 || !image.naturalWidth) return;
  const imageScale = image.naturalWidth / window.innerWidth;
  const small = document.createElement("canvas");
  small.width = Math.max(1, Math.round(rect.width / MOSAIC_BLOCK));
  small.height = Math.max(1, Math.round(rect.height / MOSAIC_BLOCK));
  small.getContext("2d")!.drawImage(image, rect.x * imageScale, rect.y * imageScale, rect.width * imageScale, rect.height * imageScale, 0, 0, small.width, small.height);
  ctx.save();
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(small, (rect.x - origin[0]) * scale, (rect.y - origin[1]) * scale, rect.width * scale, rect.height * scale);
  ctx.restore();
}

function draw(surface: Surface, shape: Shape) {
  const { ctx, scale, origin } = surface;
  const points = shape.points.map(([x, y]) => [(x - origin[0]) * scale, (y - origin[1]) * scale] as const);
  ctx.strokeStyle = ctx.fillStyle = shape.color;
  if (shape.tool === "text") {
    ctx.font = `600 ${TEXT_SIZE * scale}px system-ui, sans-serif`;
    ctx.textBaseline = "middle";
    ctx.fillText(shape.text ?? "", points[0][0], points[0][1] + (TEXT_LINE / 2) * scale);
    return;
  }
  if (points.length < 2) return;
  if (shape.tool === "mosaic") return mosaic(surface, normalize(shape.points[0], shape.points[shape.points.length - 1]));
  ctx.lineWidth = 3 * scale;
  ctx.lineCap = ctx.lineJoin = "round";
  const [start, end] = [points[0], points[points.length - 1]];
  ctx.beginPath();
  if (shape.tool === "rect") {
    ctx.strokeRect(start[0], start[1], end[0] - start[0], end[1] - start[1]);
    return;
  }
  if (shape.tool === "pen") {
    ctx.moveTo(start[0], start[1]);
    for (const [x, y] of points.slice(1)) ctx.lineTo(x, y);
    ctx.stroke();
    return;
  }
  // Arrow: a line with a filled head.
  const angle = Math.atan2(end[1] - start[1], end[0] - start[0]);
  const head = 14 * scale;
  ctx.moveTo(start[0], start[1]);
  ctx.lineTo(end[0] - Math.cos(angle) * head * 0.6, end[1] - Math.sin(angle) * head * 0.6);
  ctx.stroke();
  ctx.beginPath();
  ctx.moveTo(end[0], end[1]);
  ctx.lineTo(end[0] - head * Math.cos(angle - 0.45), end[1] - head * Math.sin(angle - 0.45));
  ctx.lineTo(end[0] - head * Math.cos(angle + 0.45), end[1] - head * Math.sin(angle + 0.45));
  ctx.closePath();
  ctx.fill();
}

/// Distance from a point to the segment a–b.
function distanceToSegment(p: [number, number], a: [number, number], b: [number, number]) {
  const [dx, dy] = [b[0] - a[0], b[1] - a[1]];
  const length = dx * dx + dy * dy;
  const t = length ? Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length)) : 0;
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

let measure: CanvasRenderingContext2D | null = null;

/// Whether a point is on an annotation, with a little slack for thin lines.
function hits(shape: Shape, p: [number, number]) {
  const slack = 7;
  const within = (r: Rect) => p[0] >= r.x - slack && p[0] <= r.x + r.width + slack && p[1] >= r.y - slack && p[1] <= r.y + r.height + slack;
  if (shape.tool === "text") {
    measure ??= document.createElement("canvas").getContext("2d");
    if (measure) measure.font = `600 ${TEXT_SIZE}px system-ui, sans-serif`;
    const width = measure?.measureText(shape.text ?? "").width ?? 80;
    return within({ x: shape.points[0][0], y: shape.points[0][1], width, height: TEXT_LINE });
  }
  const [first, last] = [shape.points[0], shape.points[shape.points.length - 1]];
  if (shape.tool === "mosaic") return within(normalize(first, last));
  if (shape.tool === "rect") {
    // The outline, not the inside: what is inside may be another annotation.
    const r = normalize(first, last);
    const corners: [number, number][] = [[r.x, r.y], [r.x + r.width, r.y], [r.x + r.width, r.y + r.height], [r.x, r.y + r.height]];
    return corners.some((corner, i) => distanceToSegment(p, corner, corners[(i + 1) % 4]) <= slack);
  }
  // Arrow and pen: near any stretch of the line.
  return shape.points.slice(1).some((point, i) => distanceToSegment(p, shape.points[i], point) <= slack);
}

/// The selection after dragging one of its handles, or all of it, by dx, dy.
function adjusted(from: Rect, handle: Handle | "move", dx: number, dy: number): Rect {
  const [maxX, maxY] = [window.innerWidth, window.innerHeight];
  if (handle === "move") {
    return {
      ...from,
      x: Math.max(0, Math.min(from.x + dx, maxX - from.width)),
      y: Math.max(0, Math.min(from.y + dy, maxY - from.height)),
    };
  }
  let [left, top, right, bottom] = [from.x, from.y, from.x + from.width, from.y + from.height];
  if (handle.includes("w")) left = Math.max(0, Math.min(left + dx, right - MIN_SIZE));
  if (handle.includes("e")) right = Math.min(maxX, Math.max(right + dx, left + MIN_SIZE));
  if (handle.includes("n")) top = Math.max(0, Math.min(top + dy, bottom - MIN_SIZE));
  if (handle.includes("s")) bottom = Math.min(maxY, Math.max(bottom + dy, top + MIN_SIZE));
  return { x: left, y: top, width: right - left, height: bottom - top };
}

export function ScreenshotOverlay() {
  const [capture, setCapture] = useState<Capture | null>(null);
  const [selection, setSelection] = useState<Rect | null>(null);
  const [tool, setTool] = useState<Tool>("select");
  const [color, setColor] = useState(COLORS[0]);
  const [shapes, setShapes] = useState<Shape[]>([]);
  const [status, setStatus] = useState<{ text: string; error?: boolean } | null>(null);
  /// Hides the toolbar while the mouse is down.
  const [dragging, setDragging] = useState(false);
  const imageRef = useRef<HTMLImageElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  /// The drag in progress: a new selection, a shape being drawn, or the
  /// selection being moved or resized (`from` is where it started).
  /// … or an annotation being moved (`moving` is its index, `origin` where its points were).
  const drag = useRef<{ start: [number, number]; shape?: Shape; adjust?: Handle | "move"; from?: Rect; moving?: number; origin?: [number, number][] } | null>(
    null,
  );
  /// The pointer is over an annotation that can be dragged.
  const [overShape, setOverShape] = useState(false);
  /// Where text is being typed, in screen coordinates.
  const [typing, setTyping] = useState<[number, number] | null>(null);
  /// The same, read when the text is committed (so it commits only once).
  const typingAt = useRef<[number, number] | null>(null);
  typingAt.current = typing;
  const busy = useRef(false);
  /// Draw nothing: the overlay is about to hide.
  const [closing, setClosing] = useState(false);
  /// The window the pointer is on, which a click would select: an index
  /// into `windowRects()`, or -1. It changes when the pointer crosses into
  /// another window, not as it moves.
  const [hover, setHover] = useState(-1);
  const placed = useRef<{ capture: Capture; width: number; rects: Rect[] } | null>(null);

  /// The capture's windows in overlay pixels, topmost first. Worked out when
  /// first needed and again if the overlay's size has changed: the capture
  /// can arrive before the window has its final size.
  const windowRects = () => {
    if (!capture?.windows?.length) return [];
    const width = window.innerWidth;
    if (placed.current?.capture !== capture || placed.current.width !== width) {
      const ratio = pixelRatio(capture.width, width, capture.scale);
      placed.current = { capture, width, rects: capture.windows.map((w) => toOverlay(w, ratio)) };
    }
    return placed.current.rects;
  };

  useEffect(() => {
    const unlisten = listen<Capture>(event.screenshot, ({ payload }) => {
      setCapture(payload);
      setSelection(null);
      setShapes([]);
      setTool("select");
      setStatus(null);
      setTyping(null);
      setHover(-1);
      busy.current = false;
    });
    return () => void unlisten.then((off) => off());
  }, []);

  // The element exists only once the capture has rendered.
  useEffect(() => {
    if (capture) rootRef.current?.focus();
  }, [capture]);

  // Redraw the annotations whenever they or the selection change. Before the
  // frame is painted, not after: the canvas moves and resizes with the
  // selection, so for a frame the annotations would move or stretch with it
  // and then jump back, which shows as a jitter while dragging.
  useLayoutEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !selection) return;
    const ratio = window.devicePixelRatio || 1;
    const ctx = canvas.getContext("2d")!;
    const [width, height] = [Math.round(selection.width * ratio), Math.round(selection.height * ratio)];
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    } else {
      // Resizing clears the canvas but allocates a new one: too much to do for every stroke of the pen.
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, width, height);
    }
    const surface = { ctx, scale: ratio, origin: [selection.x, selection.y] as [number, number], image: imageRef.current! };
    for (const shape of shapes) draw(surface, shape);
  }, [shapes, selection]);

  /// The selected region with its annotations, as base64 PNG at screen resolution.
  const render = (rect: Rect) => {
    const image = imageRef.current!;
    const scale = image.naturalWidth / window.innerWidth;
    const out = document.createElement("canvas");
    out.width = Math.round(rect.width * scale);
    out.height = Math.round(rect.height * scale);
    const ctx = out.getContext("2d")!;
    ctx.drawImage(image, rect.x * scale, rect.y * scale, rect.width * scale, rect.height * scale, 0, 0, out.width, out.height);
    for (const shape of shapes) draw({ ctx, scale, origin: [rect.x, rect.y], image }, shape);
    return out.toDataURL("image/png").split(",")[1];
  };

  const finish = async (action: "copy" | "save" | "pin" | "text" | "pick" | "cancel", rect = selection) => {
    // Cancelling must always work, even if this page lost its capture.
    if (action === "cancel" && !capture) return void invoke("screenshot_finish", { request: { action } });
    if (busy.current || !capture) return;
    if (action !== "cancel" && !rect) return;
    busy.current = true;
    if (action === "text") setStatus({ text: "Recognizing text…" });
    try {
      // The capture loads in the background; a very quick selection can beat it.
      if (action !== "cancel") await imageRef.current?.decode();
      const png = action === "cancel" ? "" : render(rect!);
      // Clear the drawing before the window hides: a hidden window keeps its
      // last frame and would flash it when the next capture opens.
      if (action !== "text") {
        setClosing(true);
        await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      }
      await invoke("screenshot_finish", {
        request: {
          action,
          png,
          ...(rect ?? {}),
          originX: capture.x,
          originY: capture.y,
          scale: capture.scale,
        },
      });
      setCapture(null);
    } catch (e) {
      setStatus({ text: String(e), error: true });
    } finally {
      busy.current = false;
      setClosing(false);
    }
  };

  const point = (e: React.MouseEvent): [number, number] => [e.clientX, e.clientY];
  const inside = (p: [number, number], r: Rect) => p[0] >= r.x && p[0] <= r.x + r.width && p[1] >= r.y && p[1] <= r.y + r.height;

  const onMouseDown = (e: React.MouseEvent) => {
    const target = e.target as HTMLElement;
    if (e.button !== 0 || busy.current || target.closest(".shot-toolbar, .shot-text")) return;
    const p = point(e);
    // No text or image selection, which would tint the capture.
    e.preventDefault();
    rootRef.current?.focus();
    setStatus(null);
    setDragging(true);
    setHover(-1);
    const handle = target.dataset.handle as Handle | undefined;
    const grabbed = shapeAt(p);
    if (selection && handle) {
      drag.current = { start: p, adjust: handle, from: selection };
    } else if (grabbed !== -1) {
      // Bring it to the front, then move it with the pointer.
      const shape = shapes[grabbed];
      setShapes((all) => [...all.filter((_, i) => i !== grabbed), shape]);
      drag.current = { start: p, moving: shapes.length - 1, origin: shape.points };
    } else if (tool === "text" && selection && inside(p, selection)) {
      setDragging(false);
      setTyping([p[0], p[1] - TEXT_LINE / 2]);
    } else if (tool !== "select" && selection && inside(p, selection)) {
      drag.current = { start: p, shape: { tool, color, points: [p, p] } };
      setShapes((s) => [...s, drag.current!.shape!]);
    } else if (selection && inside(p, selection)) {
      drag.current = { start: p, adjust: "move", from: selection };
    } else {
      // Dragging outside the selection starts a new one.
      drag.current = { start: p };
      setShapes([]);
      setTool("select");
      setSelection({ x: p[0], y: p[1], width: 0, height: 0 });
    }
  };

  /// The topmost annotation under a point that a press would move, or -1.
  /// With the selection tool that is any of them; with a drawing tool only
  /// text, since pressing elsewhere there starts a new shape.
  const shapeAt = (p: [number, number]) => {
    if (!selection || capture?.mode !== "capture") return -1;
    for (let i = shapes.length - 1; i >= 0; i--) {
      if ((tool === "select" || shapes[i].tool === "text") && hits(shapes[i], p)) return i;
    }
    return -1;
  };

  const onMouseMove = (e: React.MouseEvent) => {
    const d = drag.current;
    const p = point(e);
    if (!d) {
      const onTool = !!(e.target as HTMLElement).closest(".shot-toolbar, .shot-text, .shot-handle");
      const over = shapeAt(p) !== -1 && !onTool;
      if (over !== overShape) setOverShape(over);
      // Where a press would start a new selection, a click takes the window there.
      const free = !onTool && !over && !typing && !busy.current && !(selection && inside(p, selection));
      const hovered = free ? windowAt(windowRects(), p[0], p[1]) : -1;
      if (hovered !== hover) setHover(hovered);
      return;
    }
    if (d.moving !== undefined && d.origin) {
      const [dx, dy] = [p[0] - d.start[0], p[1] - d.start[1]];
      const points = d.origin.map(([x, y]) => [x + dx, y + dy] as [number, number]);
      setShapes((all) => all.map((shape, i) => (i === d.moving ? { ...shape, points } : shape)));
    } else if (d.shape && selection) {
      const points: [number, number][] = d.shape.tool === "pen" ? [...d.shape.points, p] : [d.shape.points[0], p];
      d.shape = { ...d.shape, points };
      setShapes((s) => [...s.slice(0, -1), d.shape!]);
    } else if (d.adjust && d.from) {
      setSelection(adjusted(d.from, d.adjust, p[0] - d.start[0], p[1] - d.start[1]));
    } else {
      setSelection(normalize(d.start, p));
    }
  };

  const onMouseUp = (e: React.MouseEvent) => {
    const d = drag.current;
    drag.current = null;
    setDragging(false);
    if (!d || d.shape || d.adjust || d.moving !== undefined) return;
    let rect = normalize(d.start, point(e));
    if (rect.width < MIN_SIZE || rect.height < MIN_SIZE) {
      // A click rather than a drag: the window that was pressed on, exactly.
      const windows = windowRects();
      const picked = windows[windowAt(windows, d.start[0], d.start[1])];
      if (!picked) return setSelection(null);
      rect = picked;
    }
    setSelection(rect);
    // Without a toolbar (text, or an area for an extension) selecting is
    // the whole interaction.
    if (capture?.mode === "text" || capture?.mode === "pick") finish(capture.mode, rect);
  };

  /// Adds the typed text as an annotation.
  const commitText = (text: string) => {
    // Enter commits, and the box then loses focus as it goes away, which
    // would commit a second copy on top of the first.
    const at = typingAt.current;
    typingAt.current = null;
    if (at && text.trim()) setShapes((s) => [...s, { tool: "text", color, points: [at], text }]);
    setTyping(null);
    rootRef.current?.focus();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    // Keys typed into a text annotation are its own.
    if ((e.target as HTMLElement).closest(".shot-text")) return;
    // Alt+F4 arrives here as a key: the overlay holds the keyboard, so the
    // window manager never sees it.
    if (e.key === "Escape" || (e.altKey && e.key === "F4")) finish("cancel");
    // An area for an extension goes to the extension, never to the clipboard or a file.
    else if (capture?.mode === "pick") {
      if (e.key === "Enter") finish("pick");
    } else if (e.key === "Enter" || (e.ctrlKey && e.key === "c")) finish("copy");
    else if (e.ctrlKey && e.key === "s") finish("save");
    else if (e.ctrlKey && e.key === "z") setShapes((s) => s.slice(0, -1));
    else return;
    e.preventDefault();
  };

  if (!capture || closing) {
    return <div className="shot" tabIndex={-1} onKeyDown={onKeyDown} onContextMenu={() => finish("cancel")} onClick={() => finish("cancel")} />;
  }

  const toolButton = (id: Tool, Icon: typeof Pen, title: string) => (
    <button className={tool === id ? "active" : undefined} title={title} onClick={() => setTool(tool === id ? "select" : id)}>
      <Icon size={16} />
    </button>
  );
  // Below the selection, or above it when there is no room.
  const toolbarBelow = selection ? selection.y + selection.height + 52 < window.innerHeight : true;
  const selected = selection && selection.width >= MIN_SIZE && selection.height >= MIN_SIZE && !dragging;

  const source = convertFileSrc(capture.path);
  const hovered = dragging ? undefined : windowRects()[hover];

  return (
    <div
      className={`shot${tool === "select" ? "" : " drawing"}${overShape ? " over-shape" : ""}`}
      ref={rootRef}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={onMouseUp}
      onContextMenu={(e) => {
        e.preventDefault();
        finish("cancel");
      }}
    >
      {/* Loaded with CORS so the canvas it is drawn on can be exported. */}
      <img
        ref={imageRef}
        className="shot-image"
        crossOrigin="anonymous"
        src={source}
        alt=""
        draggable={false}
      />
      {/* Changes only when the pointer enters another window, so it costs a drag nothing. */}
      {hovered && (
        <div
          className="shot-window"
          style={{
            left: hovered.x,
            top: hovered.y,
            width: hovered.width,
            height: hovered.height,
            backgroundImage: `url("${source}")`,
            backgroundSize: `${window.innerWidth}px ${window.innerHeight}px`,
            backgroundPosition: `${-hovered.x}px ${-hovered.y}px`,
          }}
        />
      )}
      {selection ? (
        <div
          className="shot-selection"
          style={{
            left: selection.x,
            top: selection.y,
            width: selection.width,
            height: selection.height,
            // Its part of the capture, undimmed.
            backgroundImage: `url("${source}")`,
            backgroundSize: `${window.innerWidth}px ${window.innerHeight}px`,
            backgroundPosition: `${-selection.x}px ${-selection.y}px`,
          }}
        >
          {/* A canvas the size of the selection is costly to keep resizing; it is there only once something is drawn. */}
          {shapes.length > 0 && <canvas ref={canvasRef} />}
          <span className="shot-size">
            {Math.round(selection.width * capture.scale)} × {Math.round(selection.height * capture.scale)}
          </span>
          {selected && capture.mode === "capture" && tool === "select" && HANDLES.map((h) => <i key={h} className={`shot-handle ${h}`} data-handle={h} />)}
        </div>
      ) : (
        <div className="shot-dim">
          <span className="shot-hint">
            {capture.mode === "text" ? "Drag over the text to copy" : "Drag to select an area"}
            {capture.windows?.length ? ", or click a window" : ""} · Esc to cancel
          </span>
        </div>
      )}
      {selection && selected && capture.mode === "capture" && (
        <div
          className="shot-toolbar"
          style={{
            left: Math.max(8, Math.min(selection.x + selection.width - TOOLBAR_WIDTH, window.innerWidth - TOOLBAR_WIDTH - 8)),
            top: toolbarBelow ? selection.y + selection.height + 8 : Math.max(8, selection.y - 46),
          }}
        >
          {toolButton("rect", Square, "Rectangle")}
          {toolButton("arrow", ArrowUpRight, "Arrow")}
          {toolButton("pen", Pen, "Pen")}
          {toolButton("text", Type, "Text")}
          {toolButton("mosaic", Grid3x3, "Blur")}
          {COLORS.map((c) => (
            <button key={c} className={`shot-color${c === color ? " active" : ""}`} title="Color" onClick={() => setColor(c)}>
              <span style={{ background: c }} />
            </button>
          ))}
          <button title="Undo (Ctrl+Z)" disabled={!shapes.length} onClick={() => setShapes((s) => s.slice(0, -1))}>
            <Undo2 size={16} />
          </button>
          <span className="shot-separator" />
          <button title="Copy text in the selection" onClick={() => finish("text")}>
            <ScanText size={16} />
          </button>
          <button title="Pin to screen" onClick={() => finish("pin")}>
            <Pin size={16} />
          </button>
          <button title="Save (Ctrl+S)" onClick={() => finish("save")}>
            <Download size={16} />
          </button>
          <button className="primary" title="Copy (Enter)" onClick={() => finish("copy")}>
            <Copy size={16} />
          </button>
          <button title="Cancel (Esc)" onClick={() => finish("cancel")}>
            <X size={16} />
          </button>
        </div>
      )}
      {typing && (
        <input
          // A new box for each place clicked, so it starts empty and focused.
          key={typing.join()}
          className="shot-text"
          autoFocus
          spellCheck={false}
          style={{ left: typing[0], top: typing[1], color, height: TEXT_LINE, fontSize: TEXT_SIZE }}
          onBlur={(e) => commitText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.nativeEvent.isComposing) commitText(e.currentTarget.value);
            else if (e.key === "Escape" && !e.nativeEvent.isComposing) {
              setTyping(null);
              rootRef.current?.focus();
            }
          }}
        />
      )}
      {status && <div className={`shot-status${status.error ? " error" : ""}`}>{status.text}</div>}
    </div>
  );
}

/// A pinned capture: stays on top, drag to move, double-click or Esc to close.
export function PinWindow({ path }: { path: string }) {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => e.key === "Escape" && getCurrentWindow().close();
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
  // Dragging starts only once the pointer moves, so a double-click still closes.
  const pressed = useRef<[number, number] | null>(null);
  return (
    <div
      className="pin"
      title="Drag to move · double-click or right-click to close"
      onMouseDown={(e) => (pressed.current = e.button === 0 ? [e.clientX, e.clientY] : null)}
      onMouseUp={() => (pressed.current = null)}
      onMouseMove={(e) => {
        const start = pressed.current;
        if (!start || Math.hypot(e.clientX - start[0], e.clientY - start[1]) < 4) return;
        pressed.current = null;
        getCurrentWindow().startDragging();
      }}
      onDoubleClick={() => getCurrentWindow().close()}
      onContextMenu={(e) => {
        e.preventDefault();
        getCurrentWindow().close();
      }}
    >
      <img src={convertFileSrc(path)} alt="" draggable={false} />
    </div>
  );
}
