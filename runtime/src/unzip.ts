//! Minimal zip extraction (stored and deflated entries), enough for store bundles.

import fs from "node:fs";
import path from "node:path";
import { inflateRawSync } from "node:zlib";

interface Entry {
  name: string;
  method: number;
  compressedSize: number;
  localHeader: number;
}

function entries(zip: Buffer): Entry[] {
  // The end-of-central-directory record sits in the last 64 KiB + 22 bytes.
  let eocd = -1;
  for (let i = zip.length - 22; i >= Math.max(0, zip.length - 65_557); i--) {
    if (zip.readUInt32LE(i) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd === -1) throw new Error("not a zip archive");

  const count = zip.readUInt16LE(eocd + 10);
  let offset = zip.readUInt32LE(eocd + 16);
  const out: Entry[] = [];
  for (let i = 0; i < count; i++) {
    if (zip.readUInt32LE(offset) !== 0x02014b50) throw new Error("corrupt zip central directory");
    const nameLength = zip.readUInt16LE(offset + 28);
    const extraLength = zip.readUInt16LE(offset + 30);
    const commentLength = zip.readUInt16LE(offset + 32);
    out.push({
      method: zip.readUInt16LE(offset + 10),
      compressedSize: zip.readUInt32LE(offset + 20),
      localHeader: zip.readUInt32LE(offset + 42),
      name: zip.toString("utf8", offset + 46, offset + 46 + nameLength),
    });
    offset += 46 + nameLength + extraLength + commentLength;
  }
  return out;
}

function contents(zip: Buffer, entry: Entry) {
  const header = entry.localHeader;
  const start = header + 30 + zip.readUInt16LE(header + 26) + zip.readUInt16LE(header + 28);
  const data = zip.subarray(start, start + entry.compressedSize);
  if (entry.method === 0) return data;
  if (entry.method === 8) return inflateRawSync(data);
  throw new Error(`unsupported zip compression method ${entry.method} (${entry.name})`);
}

/// Extracts `zip` into `dest`, dropping a top-level directory shared by all entries.
export function extract(zip: Buffer, dest: string) {
  const all = entries(zip).filter((e) => !e.name.startsWith("__MACOSX/"));
  const roots = new Set(all.map((e) => e.name.split("/")[0]));
  const strip = roots.size === 1 && all.every((e) => e.name.includes("/")) ? [...roots][0].length + 1 : 0;

  const root = path.resolve(dest);
  for (const entry of all) {
    const name = entry.name.slice(strip);
    if (!name) continue;
    const target = path.resolve(root, name);
    if (target !== root && !target.startsWith(root + path.sep)) {
      throw new Error(`zip entry escapes destination: ${entry.name}`);
    }
    if (name.endsWith("/")) {
      fs.mkdirSync(target, { recursive: true });
      continue;
    }
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, contents(zip, entry));
  }
}
