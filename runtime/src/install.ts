//! Fetching what Heyo installs on demand (the speech and text-recognition
//! engines and their models), so that what runs is what was chosen when
//! Heyo was built and not whatever the network hands over that day.
//!
//! What a helper has to say to the user it says as a code and its
//! particulars (`Failure`, `said`), never in words: it has no translations,
//! and the host words it in the user's language (`helper::worded` in
//! `src-tauri/src/helper.rs`). The steps of an installation are codes too.
//!
//! Files are downloaded from an address that names one revision and are
//! checked against a SHA-256 written here in the source before they get
//! their name. npm packages are installed from a lockfile that ships with
//! Heyo (`locks/`): exact versions of the whole tree, each checked by npm
//! against the lockfile's sha512, and none of their install scripts run.

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";

/// What went wrong, as the host words it: a code, and its particulars.
/// `detail` is what only the words of whatever failed can say (a server's
/// answer, an exception's message), shown as it is.
export class Failure extends Error {
  constructor(
    readonly code: string,
    readonly particulars: Record<string, string> = {},
  ) {
    super(particulars.detail ? `${code}: ${particulars.detail}` : code);
  }
}

/// What a helper sends for something that went wrong: a `Failure` as it
/// is, anything else as a failure the host has no other words for.
export function said(e: unknown): Record<string, string> {
  if (e instanceof Failure) return { code: e.code, ...e.particulars };
  return { code: "failed", detail: e instanceof Error ? e.message : String(e) };
}

/// A file to download: where it goes, where it comes from, and what it must be.
export interface PinnedFile {
  file: string;
  /// What it is, as a step of the installation the host names
  /// ("speech-model").
  step: string;
  /// Lowercase hex.
  sha256: string;
  /// Tried in order; mirrors of the same bytes.
  urls: string[];
}

export type Progress = (step: string, received: number, total: number) => void;

export function sha256(file: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    fs.createReadStream(file)
      .on("data", (chunk) => hash.update(chunk))
      .on("error", reject)
      .on("end", () => resolve(hash.digest("hex")));
  });
}

/// Makes sure `entry.file` exists and is the pinned file, downloading it if
/// it is missing or is something else. A download that is not the pinned
/// file never gets the file's name.
export async function download(entry: PinnedFile, progress: Progress = () => {}, fetcher: typeof fetch = fetch) {
  // Left by an earlier installation, which did not check what it got.
  if (fs.existsSync(entry.file)) {
    if ((await sha256(entry.file)) === entry.sha256) return;
    fs.rmSync(entry.file);
  }
  fs.mkdirSync(path.dirname(entry.file), { recursive: true });
  const partial = `${entry.file}.part`;
  let lastError: unknown;
  for (const url of entry.urls) {
    try {
      const response = await fetcher(url);
      if (!response.ok || !response.body) throw new Error(`${response.status} ${response.statusText}`);
      const total = Number(response.headers.get("content-length") ?? 0);
      const hash = createHash("sha256");
      let received = 0;
      let reported = 0;
      const counter = async function* (source: AsyncIterable<Uint8Array>) {
        for await (const chunk of source) {
          hash.update(chunk);
          received += chunk.length;
          if (Date.now() - reported > 200) {
            reported = Date.now();
            progress(entry.step, received, total);
          }
          yield chunk;
        }
      };
      await pipeline(Readable.fromWeb(response.body as never), counter, fs.createWriteStream(partial));
      const got = hash.digest("hex");
      if (got !== entry.sha256) {
        throw new Error(`what ${new URL(url).host} sent is not the expected file (SHA-256 ${got}, expected ${entry.sha256}); it was not installed`);
      }
      fs.renameSync(partial, entry.file);
      return;
    } catch (e) {
      lastError = e;
      fs.rmSync(partial, { force: true });
    }
  }
  const reason = lastError instanceof Error ? lastError.message : String(lastError);
  throw new Failure("download", { step: entry.step, detail: reason });
}

/// An npm lockfile (version 3), as far as this module reads it.
export interface Lock {
  name: string;
  packages: Record<string, { version?: string; resolved?: string; integrity?: string; dependencies?: Record<string, string>; hasInstallScript?: boolean }>;
}

/// The `package.json` a lockfile was made for.
export function manifest(lock: Lock) {
  return { name: lock.name, private: true, dependencies: lock.packages[""].dependencies ?? {} };
}

function run(command: string, args: string[], cwd: string) {
  return new Promise<void>((resolve, reject) => {
    // On Windows npm is a script (`npm.cmd`), which only a shell can start;
    // the shell wants a path with spaces in quotes.
    const windows = process.platform === "win32";
    const child = spawn(windows ? `"${command}"` : command, args, { cwd, stdio: ["ignore", "ignore", "pipe"], shell: windows });
    let stderr = "";
    child.stderr?.on("data", (d) => (stderr += d));
    child.once("error", reject);
    child.once("exit", (code) => (code === 0 ? resolve() : reject(new Failure("packages", { detail: stderr.trim().split("\n").pop() || `${command} failed` }))));
  });
}

/// Installs exactly the packages of `lock` into `dir`.
export async function installPackages(dir: string, lock: Lock) {
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "package.json"), JSON.stringify(manifest(lock), null, 2));
  fs.writeFileSync(path.join(dir, "package-lock.json"), JSON.stringify(lock, null, 2));
  // npm ships next to node.
  const beside = path.join(path.dirname(process.execPath), process.platform === "win32" ? "npm.cmd" : "npm");
  const npm = fs.existsSync(beside) ? beside : "npm";
  // `ci` installs the lockfile and nothing else, and fails on a package
  // whose contents are not what the lockfile says. No install scripts: the
  // engines come prebuilt, and a script is code nobody pinned.
  await run(npm, ["ci", "--ignore-scripts", "--no-audit", "--no-fund"], dir);
}
