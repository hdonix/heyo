//! Heyo's embedding helper: turns text into vectors whose angles say how
//! alike two texts are in meaning, across Chinese and English, for finding
//! clipboard entries by meaning. The model is multilingual-e5-small
//! (quantised to 8 bits) through onnxruntime; nothing leaves this computer.
//!
//! The runtime is the one text recognition installs, when that is there;
//! otherwise it is installed into `$HEYO_EMBED_DIR`, where the model goes.
//!
//!   heyo-embed.cjs install   → JSON lines: progress | installed | error
//!   heyo-embed.cjs           → serves, a JSON line each way:
//!       in:  { id, role: "query" | "passage", texts: string[] }
//!       out: { id, model, size, vectors } | { id, error }
//!     `vectors` is base64 of the vectors one after another, `size` 32-bit
//!     floats each. Queries are answered before passages that wait. It ends
//!     when its stdin closes, or after a while with nothing to do.
//!   heyo-embed.cjs cases     → writes search cases for evaluation with the
//!     local model (`cases.ts`); a JSON document in, JSON lines out.

import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";

import { casesFor, type CaseRequest } from "./cases";
import { padded, pooled, prefixed, Tokenizer } from "./embedding";
import { download, installPackages, type PinnedFile } from "./install";
import engineLock from "./locks/embed.json";

// Absolute: packages are found from it.
const DIR = process.env.HEYO_EMBED_DIR ? path.resolve(process.env.HEYO_EMBED_DIR) : "";
/// Where text recognition keeps its engine, which brings the same runtime.
const OCR_DIR = process.env.HEYO_OCR_DIR ? path.resolve(process.env.HEYO_OCR_DIR) : "";
const RUNTIME = "onnxruntime-node";

/// Names the model and everything about how a text becomes a vector. The
/// host stores it with each vector: change anything here that changes the
/// vectors, change this, and the old ones are made again.
const MODEL = "multilingual-e5-small-int8/1";
/// A passage is read this far (in tokens, about 180 English words or 350
/// Chinese characters). The cost of a text grows with its length, and what
/// a person remembers of a long copy is nearly always how it begins.
const PASSAGE_TOKENS = 256;
const QUERY_TOKENS = 64;
/// No text has more tokens than characters, and few pieces are longer than
/// this many: what is past it would be cut anyway.
const CHARACTERS_A_TOKEN = 12;
const IDLE_EXIT_MS = 5 * 60 * 1000;

// The model repository at one revision (its head on 2026-10-10, unchanged
// since 2025-07-22). The hashes are of the files as downloaded then, and
// are the `lfs.sha256` Hugging Face lists for them.
const REVISION = "Xenova/multilingual-e5-small/resolve/761b726dd34fb83930e26aab4e9ac3899aa1fa78";
const FILES: PinnedFile[] = [
  {
    file: path.join(DIR, "model", "model_quantized.onnx"),
    label: "Language model",
    sha256: "f80102d3f2a1229f387d3c81909990d8945513e347b0eab049f7de3c6f98c193",
    urls: [`https://huggingface.co/${REVISION}/onnx/model_quantized.onnx`, `https://hf-mirror.com/${REVISION}/onnx/model_quantized.onnx`],
  },
  {
    file: path.join(DIR, "model", "tokenizer.json"),
    label: "Vocabulary",
    sha256: "0b44a9d7b51c3c62626640cda0e2c2f70fdacdc25bbbd68038369d14ebdf4c39",
    urls: [`https://huggingface.co/${REVISION}/tokenizer.json`, `https://hf-mirror.com/${REVISION}/tokenizer.json`],
  },
];

function send(message: Record<string, unknown>) {
  process.stdout.write(JSON.stringify(message) + "\n");
}

/// The directory whose `node_modules` has the runtime at the version this
/// was written against: Heyo's own, or text recognition's.
function runtimeHome(): string | undefined {
  const wanted = engineLock.packages[`node_modules/${RUNTIME}`].version;
  return [DIR, OCR_DIR].find((dir) => {
    try {
      return dir && JSON.parse(fs.readFileSync(path.join(dir, "node_modules", RUNTIME, "package.json"), "utf8")).version === wanted;
    } catch {
      return false;
    }
  });
}

async function install() {
  if (!runtimeHome()) {
    send({ type: "progress", step: "Engine", received: 0, total: 0 });
    // onnxruntime-node 1.30.0, from the lockfile (`npm install
    // --package-lock-only`, 2026-10-10). It carries its libraries; its
    // install script (which would fetch GPU libraries) is not run.
    await installPackages(DIR, engineLock);
  }
  for (const entry of FILES) await download(entry, (step, received, total) => send({ type: "progress", step, received, total }));
  send({ type: "installed" });
}

interface Engine {
  embed(role: "query" | "passage", texts: string[]): Promise<Float32Array[]>;
}

async function load(): Promise<Engine> {
  const home = runtimeHome();
  if (!home || FILES.some((entry) => !fs.existsSync(entry.file))) throw new Error("Finding by meaning is not set up yet. Download it in Heyo Settings.");
  const ort = createRequire(path.join(home, "package.json"))(RUNTIME);
  const tokenizer = new Tokenizer(JSON.parse(fs.readFileSync(FILES[1].file, "utf8")));
  // Two threads: a query is answered in a few milliseconds either way, and
  // indexing a long history must not be what the computer is busy with.
  const session = await ort.InferenceSession.create(FILES[0].file, { intraOpNumThreads: threads(), interOpNumThreads: 1 });
  const wantsTypes = (session.inputNames as string[]).includes("token_type_ids");
  return {
    async embed(role, texts) {
      const limit = role === "query" ? QUERY_TOKENS : PASSAGE_TOKENS;
      const { ids, mask, longest } = padded(texts.map((text) => tokenizer.encode(prefixed(role, text.slice(0, limit * CHARACTERS_A_TOKEN)), limit)));
      const shape = [texts.length, longest];
      const feeds: Record<string, unknown> = {
        input_ids: new ort.Tensor("int64", ids, shape),
        attention_mask: new ort.Tensor("int64", mask, shape),
      };
      if (wantsTypes) feeds.token_type_ids = new ort.Tensor("int64", new BigInt64Array(ids.length), shape);
      const output = await session.run(feeds);
      const hidden = output.last_hidden_state ?? output[session.outputNames[0]];
      return pooled(hidden.data as Float32Array, mask, texts.length, longest, hidden.dims[2]);
    },
  };
}

function threads() {
  const asked = Number(process.env.HEYO_EMBED_THREADS);
  return Number.isInteger(asked) && asked > 0 ? asked : 2;
}

interface Job {
  id: unknown;
  role: "query" | "passage";
  texts: string[];
  /// Of the texts done so far.
  vectors: Float32Array[];
}

function serve() {
  let engine: Promise<Engine> | undefined;
  const waiting: Job[] = [];
  let working = false;
  let idle: NodeJS.Timeout | undefined;

  const rest = () => {
    clearTimeout(idle);
    // Gone when not needed: the model is some hundreds of megabytes of memory.
    idle = setTimeout(() => process.exit(0), IDLE_EXIT_MS);
  };

  const work = async () => {
    if (working) return;
    working = true;
    clearTimeout(idle);
    for (;;) {
      // Someone is typing a query; passages are the background's.
      const job = waiting.find((job) => job.role === "query") ?? waiting[0];
      if (!job) break;
      try {
        // What stopped the model from loading may be put right by the next try.
        const model = await (engine ??= load().catch((e) => {
          engine = undefined;
          throw e;
        }));
        // A query whole. Of passages one at a time: nothing is spent on
        // filling short texts up to the longest, and a query that arrives
        // meanwhile waits for one passage at most.
        const texts = job.role === "query" ? job.texts : job.texts.slice(job.vectors.length, job.vectors.length + 1);
        if (texts.length) job.vectors.push(...(await model.embed(job.role, texts)));
        if (job.vectors.length < job.texts.length) continue;
        send({ id: job.id, model: MODEL, size: job.vectors[0]?.length ?? 0, vectors: packed(job.vectors) });
      } catch (e) {
        send({ id: job.id, error: e instanceof Error ? e.message : String(e) });
      }
      waiting.splice(waiting.indexOf(job), 1);
    }
    working = false;
    rest();
  };

  const accept = (line: string) => {
    let job: Job;
    try {
      job = { ...JSON.parse(line), vectors: [] };
    } catch {
      return;
    }
    if (!Array.isArray(job.texts) || (job.role !== "query" && job.role !== "passage")) {
      send({ id: job.id, error: "not a request: { id, role, texts }" });
      return;
    }
    waiting.push(job);
    void work();
  };

  rest();
  // Split at line feeds and nothing else. `readline` also ends a line at
  // U+2028 and U+2029, which JSON leaves as they are and copied text can
  // contain: a request would arrive in halves, and never be answered.
  let partial = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk: string) => {
    const lines = (partial + chunk).split("\n");
    partial = lines.pop() ?? "";
    lines.forEach(accept);
  });
  // The host closed our stdin: it is done with us.
  process.stdin.on("end", () => process.exit(0));
}

function packed(vectors: Float32Array[]) {
  const all = new Float32Array(vectors.reduce((n, v) => n + v.length, 0));
  let offset = 0;
  for (const vector of vectors) {
    all.set(vector, offset);
    offset += vector.length;
  }
  return Buffer.from(all.buffer).toString("base64");
}

async function cases() {
  let input = "";
  for await (const chunk of process.stdin) input += chunk;
  const request = JSON.parse(input) as CaseRequest;
  for (const entry of request.entries) {
    try {
      send({ type: "cases", entry: entry.id, queries: await casesFor(entry, request.ai) });
    } catch (e) {
      // Without the model there is nothing more to do; say so once.
      send({ type: "error", message: e instanceof Error ? e.message : String(e) });
      return;
    }
  }
  send({ type: "done" });
}

const failed = (e: unknown) => send({ type: "error", message: e instanceof Error ? e.message : String(e) });
if (process.argv[2] === "cases") cases().catch(failed);
else if (!DIR) send({ type: "error", message: "HEYO_EMBED_DIR is not set" });
else if (process.argv[2] === "install") install().catch(failed);
else serve();
