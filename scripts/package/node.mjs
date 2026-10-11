#!/usr/bin/env node
//! Fetches the Node.js a package of Heyo carries, for where there is no
//! package manager to bring one: the Windows installer and the AppImage.
//!
//!   node scripts/package/node.mjs --out <dir> [--platform linux|win32] [--arch x64|arm64]
//!
//! Which Node.js, and what each download must hash to, is said once in
//! packaging/node.json (the hashes are nodejs.org's SHASUMS256.txt for that
//! version). The download is kept in the cache and checked every time; what
//! does not hash to what is written there is not used.
//!
//! It leaves `node` (or `node.exe`) in <dir>. `bundle.sh` beside this puts
//! it in the package, beside the program, which is where Heyo looks first.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const pinned = JSON.parse(fs.readFileSync(path.join(REPO, "packaging/node.json"), "utf8"));

function option(name, otherwise) {
  const at = process.argv.indexOf(`--${name}`);
  return at === -1 ? otherwise : process.argv[at + 1];
}

const platform = option("platform", process.platform);
const arch = option("arch", process.arch);
const out = option("out");
if (!out || !["linux", "win32"].includes(platform) || !["x64", "arm64"].includes(arch)) {
  console.error("usage: node.mjs --out <dir> [--platform linux|win32] [--arch x64|arm64]");
  process.exit(2);
}

const windows = platform === "win32";
/// The file as nodejs.org names it: the program alone for Windows, and an
/// archive with the program in it for Linux.
const artifact = windows ? `win-${arch}/node.exe` : `node-${pinned.version}-linux-${arch}.tar.xz`;
const expected = pinned.sha256[artifact];
if (!expected) throw new Error(`packaging/node.json has no hash for ${artifact}`);

const sha256 = (file) => createHash("sha256").update(fs.readFileSync(file)).digest("hex");

/// The download, from the cache when it is there and is what it should be.
async function fetched() {
  const cache = path.join(process.env.XDG_CACHE_HOME || path.join(os.homedir(), ".cache"), "heyo-build");
  const kept = path.join(cache, windows ? `node-${pinned.version}-win-${arch}.exe` : artifact);
  if (fs.existsSync(kept) && sha256(kept) === expected) return kept;
  fs.mkdirSync(cache, { recursive: true });
  const url = `https://nodejs.org/dist/${pinned.version}/${artifact}`;
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status} ${response.statusText}`);
  const part = `${kept}.part`;
  fs.writeFileSync(part, Buffer.from(await response.arrayBuffer()));
  const got = sha256(part);
  if (got !== expected) {
    fs.rmSync(part);
    throw new Error(`${url} hashes to ${got}, not to the ${expected} of packaging/node.json`);
  }
  fs.renameSync(part, kept);
  return kept;
}

const download = await fetched();
fs.mkdirSync(out, { recursive: true });
const node = path.resolve(out, windows ? "node.exe" : "node");
// Replaced rather than written over: something may be running the last one.
fs.rmSync(node, { force: true });
if (windows) {
  fs.copyFileSync(download, node);
} else {
  const inside = `node-${pinned.version}-linux-${arch}/bin/node`;
  execFileSync("tar", ["-xJf", download, "-C", path.dirname(node), "--strip-components=2", inside]);
  fs.chmodSync(node, 0o755);
}

console.log(`${node} (Node.js ${pinned.version}, ${platform} ${arch})`);
