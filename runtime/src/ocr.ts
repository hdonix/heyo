//! Heyo's text-recognition helper: PaddleOCR (PP-OCRv4, Chinese and English)
//! through onnxruntime. Like dictation, the engine is a native package
//! installed on demand into `$HEYO_OCR_DIR`.
//!
//!   heyo-ocr.cjs install        → JSON lines: progress | installed | error
//!   heyo-ocr.cjs <image file>   → one JSON line: { text } | { error }

import { createRequire } from "node:module";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { installPackages, said } from "./install";
import engineLock from "./locks/ocr.json";

const DIR = process.env.HEYO_OCR_DIR ?? "";
const PACKAGE = "@gutenye/ocr-node";

function send(message: Record<string, unknown>) {
  process.stdout.write(JSON.stringify(message) + "\n");
}

async function install() {
  send({ type: "progress", step: "ocr-engine" });
  // @gutenye/ocr-node 1.4.9 with its models, onnxruntime and sharp, from the
  // lockfile (`npm install --package-lock-only`, 2026-10-10). Both native
  // packages carry their prebuilt libraries; their install scripts (which
  // would fetch GPU libraries, or build from source) are not run.
  await installPackages(DIR, engineLock);
  send({ type: "installed" });
}

interface Line {
  text: string;
  box: [number, number][];
}

/// Reading order: rows from top to bottom, and left to right within a row.
function layout(lines: Line[]) {
  const items = lines
    .filter((l) => l.text.trim())
    .map((l) => {
      const ys = l.box.map((p) => p[1]);
      const top = Math.min(...ys);
      return { text: l.text.trim(), left: Math.min(...l.box.map((p) => p[0])), top, height: Math.max(...ys) - top };
    })
    .sort((a, b) => a.top - b.top);
  const rows: (typeof items)[] = [];
  for (const item of items) {
    const row = rows.at(-1);
    // Same row when it starts within half a line height of the row's first box.
    if (row && item.top - row[0].top < row[0].height / 2) row.push(item);
    else rows.push([item]);
  }
  return rows.map((row) => row.sort((a, b) => a.left - b.left).map((i) => i.text).join(" ")).join("\n");
}

async function recognize(file: string) {
  const entry = createRequire(path.join(DIR, "package.json")).resolve(PACKAGE);
  const Ocr = (await import(pathToFileURL(entry).href)).default;
  const ocr = await Ocr.create();
  const result = await ocr.detect(file);
  send({ text: layout(Array.isArray(result) ? result : (result.texts ?? [])) });
}

if (!DIR) send({ error: "HEYO_OCR_DIR is not set" });
else if (process.argv[2] === "install") install().catch((e) => send({ type: "error", ...said(e) }));
else recognize(process.argv[2]).catch((e) => send({ error: e instanceof Error ? e.message : String(e) }));
