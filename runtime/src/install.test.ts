import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { download, type Lock, manifest, type PinnedFile, sha256 } from "./install";
import dictationLock from "./locks/dictation.json";
import esbuildLock from "./locks/esbuild.json";
import ocrLock from "./locks/ocr.json";

const hex = (text: string) => createHash("sha256").update(text).digest("hex");

/// A network that serves the given bodies by URL, and remembers what was asked.
function network(bodies: Record<string, string | number>) {
  const asked: string[] = [];
  const fetcher = (async (url: string) => {
    asked.push(url);
    const body = bodies[url];
    if (typeof body === "number") return new Response("no", { status: body, statusText: "Nope" });
    if (body === undefined) throw new Error("getaddrinfo ENOTFOUND");
    return new Response(body);
  }) as unknown as typeof fetch;
  return { asked, fetcher };
}

describe("a pinned download", () => {
  let dir: string;
  let entry: PinnedFile;
  beforeEach(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-install-test-"));
    entry = { file: path.join(dir, "models", "model.bin"), label: "Speech model", sha256: hex("the model"), urls: ["https://a.example/model.bin", "https://b.example/model.bin"] };
  });
  afterEach(() => fs.rmSync(dir, { recursive: true, force: true }));

  it("is kept when it is the expected file", async () => {
    const { asked, fetcher } = network({ "https://a.example/model.bin": "the model" });
    const steps: string[] = [];
    await download(entry, (step) => steps.push(step), fetcher);
    expect(fs.readFileSync(entry.file, "utf8")).toBe("the model");
    expect(await sha256(entry.file)).toBe(entry.sha256);
    expect(asked).toEqual(["https://a.example/model.bin"]);
    expect(steps).toEqual(["Speech model"]);
    expect(fs.readdirSync(path.dirname(entry.file))).toEqual(["model.bin"]);
  });

  it("is refused when it is anything else, and nothing is left behind", async () => {
    const { fetcher } = network({ "https://a.example/model.bin": "something else", "https://b.example/model.bin": "the model, tampered with" });
    const refused = download(entry, undefined, fetcher);
    await expect(refused).rejects.toThrow(/Could not download the speech model: what b\.example sent is not the expected file \(SHA-256 [0-9a-f]{64}, expected [0-9a-f]{64}\)/);
    expect(fs.readdirSync(path.dirname(entry.file))).toEqual([]);
  });

  it("comes from the next address when the first fails or sends the wrong thing", async () => {
    for (const first of ["something else", 503, undefined] as const) {
      fs.rmSync(entry.file, { force: true });
      const bodies: Record<string, string | number> = { "https://b.example/model.bin": "the model" };
      if (first !== undefined) bodies["https://a.example/model.bin"] = first;
      const { asked, fetcher } = network(bodies);
      await download(entry, undefined, fetcher);
      expect(fs.readFileSync(entry.file, "utf8")).toBe("the model");
      expect(asked).toHaveLength(2);
    }
  });

  it("says why when no address answers", async () => {
    const { fetcher } = network({ "https://a.example/model.bin": 404, "https://b.example/model.bin": 500 });
    await expect(download(entry, undefined, fetcher)).rejects.toThrow("Could not download the speech model: 500 Nope");
  });

  it("is not fetched again when the file is already there and right", async () => {
    fs.mkdirSync(path.dirname(entry.file), { recursive: true });
    fs.writeFileSync(entry.file, "the model");
    const { asked, fetcher } = network({});
    await download(entry, undefined, fetcher);
    expect(asked).toEqual([]);
  });

  it("replaces a file that is there but is not the expected one", async () => {
    fs.mkdirSync(path.dirname(entry.file), { recursive: true });
    fs.writeFileSync(entry.file, "left by an older version, or damaged");
    const { fetcher } = network({ "https://a.example/model.bin": "the model" });
    await download(entry, undefined, fetcher);
    expect(fs.readFileSync(entry.file, "utf8")).toBe("the model");
    // And is not left in place when it cannot be replaced.
    fs.writeFileSync(entry.file, "damaged again");
    await expect(download(entry, undefined, network({}).fetcher)).rejects.toThrow("Could not download");
    expect(fs.existsSync(entry.file)).toBe(false);
  });
});

describe("the engines' lockfiles", () => {
  const locks: [string, Lock, string, string][] = [
    ["dictation", dictationLock, "sherpa-onnx-node", "1.13.8"],
    ["ocr", ocrLock, "@gutenye/ocr-node", "1.4.9"],
    ["esbuild", esbuildLock, "esbuild", "0.28.2"],
  ];

  it.each(locks)("%s names one exact version", (_name, lock, dependency, version) => {
    expect(manifest(lock).dependencies).toEqual({ [dependency]: version });
    expect(lock.packages[`node_modules/${dependency}`].version).toBe(version);
    expect(manifest(lock).private).toBe(true);
  });

  it.each(locks)("%s pins every package to the registry's file and its hash", (_name, lock) => {
    const installed = Object.entries(lock.packages).filter(([where]) => where !== "");
    expect(installed.length).toBeGreaterThan(1);
    for (const [where, entry] of installed) {
      expect(entry.version, where).toMatch(/^\d+\.\d+\.\d+/);
      expect(entry.resolved, where).toMatch(/^https:\/\/registry\.npmjs\.org\/.+\.tgz$/);
      expect(entry.integrity, where).toMatch(/^sha512-[A-Za-z0-9+/]{86}==$/);
    }
  });
});
