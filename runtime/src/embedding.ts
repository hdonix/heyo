//! What turns text into the numbers the embedding model reads, and the
//! model's output into one vector: the tokenizer of multilingual-e5-small
//! (XLM-RoBERTa's SentencePiece model, as `tokenizer.json` describes it)
//! and masked mean pooling.
//!
//! Written here rather than taken from a library: the libraries that read
//! `tokenizer.json` come with a native image library or a second ONNX
//! runtime. It follows the Rust `tokenizers` crate step for step, quirks
//! included, because the model only means anything for the ids that crate
//! would have produced.

/// The part of `tokenizer.json` this reads.
export interface TokenizerFile {
  normalizer: { normalizers: { type: string; precompiled_charsmap?: string }[] };
  model: { unk_id: number; vocab: [string, number][] };
}

const BOS = 0;
const PAD = 1;
const EOS = 2;
/// What stands for a space, and begins every word.
const SPACE = "▁";
/// How much worse than the rarest piece an unknown character scores.
const UNKNOWN_PENALTY = 10;

/// SentencePiece's character map: a trie (a double array, as `darts-clone`
/// lays it out) from the bytes of a character sequence to where its
/// replacement starts in a block of zero-terminated strings.
class CharacterMap {
  private readonly trie: Uint32Array;
  private readonly replacements: Uint8Array;
  private readonly decoder = new TextDecoder();
  private readonly encoder = new TextEncoder();
  private readonly segmenter = new Intl.Segmenter(undefined, { granularity: "grapheme" });

  constructor(base64: string) {
    const bytes = Buffer.from(base64, "base64");
    const size = bytes.readUInt32LE(0);
    this.trie = new Uint32Array(size / 4);
    for (let i = 0; i < this.trie.length; i++) this.trie[i] = bytes.readUInt32LE(4 + i * 4);
    this.replacements = bytes.subarray(4 + size);
  }

  /// The replacement of the shortest mapped sequence `chunk` begins with.
  private replacement(chunk: string): string | undefined {
    const offset = (unit: number) => (unit >>> 10) << ((unit & 0x200) >>> 6);
    let node = offset(this.trie[0]);
    for (const byte of this.encoder.encode(chunk)) {
      if (byte === 0) break;
      node ^= byte;
      const unit = this.trie[node];
      if (unit === undefined || (unit & 0x800000ff) >>> 0 !== byte) return undefined;
      node ^= offset(unit);
      if ((unit >>> 8) & 1) {
        const start = this.trie[node] & 0x7fffffff;
        let end = start;
        while (end < this.replacements.length && this.replacements[end] !== 0) end++;
        return this.decoder.decode(this.replacements.subarray(start, end));
      }
    }
    return undefined;
  }

  normalize(text: string): string {
    let out = "";
    for (const { segment } of this.segmenter.segment(text)) {
      // A whole cluster first (a letter and its accents), when it is short.
      const whole = Buffer.byteLength(segment) < 6 ? this.replacement(segment) : undefined;
      if (whole !== undefined) {
        out += whole;
        continue;
      }
      for (const character of segment) out += this.replacement(character) ?? character;
    }
    return out;
  }
}

export class Tokenizer {
  private readonly map?: CharacterMap;
  private readonly pieces = new Map<string, number>();
  private readonly scores: Float64Array;
  private readonly unknown: number;
  private readonly unknownScore: number;
  /// The longest piece, in UTF-16 units: no need to look further ahead.
  private readonly longest: number;

  constructor(file: TokenizerFile) {
    const charsmap = file.normalizer.normalizers.find((n) => n.type === "Precompiled")?.precompiled_charsmap;
    this.map = charsmap ? new CharacterMap(charsmap) : undefined;
    this.unknown = file.model.unk_id;
    this.scores = new Float64Array(file.model.vocab.length);
    let lowest = Infinity;
    let longest = 1;
    file.model.vocab.forEach(([piece, score], id) => {
      // The markers (`<s>`, `</s>`, `<mask>`…) are not pieces of text: text
      // that happens to spell one must not become one. (Here the reference
      // differs: it takes them literally.)
      if (/^<(s|\/s|pad|unk|mask)>$/.test(piece)) return;
      this.pieces.set(piece, id);
      this.scores[id] = score;
      lowest = Math.min(lowest, score);
      longest = Math.max(longest, piece.length);
    });
    this.unknownScore = lowest - UNKNOWN_PENALTY;
    this.longest = longest;
  }

  /// The most likely way to cut one word into pieces (Viterbi). Characters
  /// no piece covers are unknown, and a run of them is one unknown piece.
  private word(word: string, ids: number[]) {
    // Positions between characters, as UTF-16 offsets.
    const starts: number[] = [];
    for (let i = 0; i < word.length; i += word.codePointAt(i)! > 0xffff ? 2 : 1) starts.push(i);
    starts.push(word.length);
    const count = starts.length - 1;
    const best = new Float64Array(count + 1);
    const from = new Int32Array(count + 1).fill(-1);
    const id = new Int32Array(count + 1);
    for (let start = 0; start < count; start++) {
      const base = best[start];
      let single = false;
      for (let end = start + 1; end <= count && starts[end] - starts[start] <= this.longest; end++) {
        const piece = this.pieces.get(word.slice(starts[start], starts[end]));
        if (piece === undefined) continue;
        const score = base + this.scores[piece];
        if (from[end] === -1 || score > best[end]) {
          best[end] = score;
          from[end] = start;
          id[end] = piece;
        }
        if (end === start + 1) single = true;
      }
      if (!single) {
        const score = base + this.unknownScore;
        if (from[start + 1] === -1 || score > best[start + 1]) {
          best[start + 1] = score;
          from[start + 1] = start;
          id[start + 1] = this.unknown;
        }
      }
    }
    const found: number[] = [];
    for (let end = count; end > 0; end = from[end]) {
      // Going backwards: an unknown piece before an unknown piece is the same one.
      if (id[end] === this.unknown && found.at(-1) === this.unknown) continue;
      found.push(id[end]);
    }
    for (let i = found.length - 1; i >= 0; i--) ids.push(found[i]);
  }

  /// The ids of a text, between the markers of its beginning and end, and
  /// at most `limit` of them: what is past that is left out.
  encode(text: string, limit: number): number[] {
    const normal = (this.map?.normalize(text) ?? text).replace(/ {2,}/g, " ").replaceAll(" ", SPACE);
    if (!normal) return [BOS, EOS];
    const marked = normal.startsWith(SPACE) ? normal : SPACE + normal;
    const ids = [BOS];
    // Each word carries the space before it.
    for (const word of marked.split(new RegExp(`(?=${SPACE})`))) {
      if (ids.length >= limit - 1) break;
      this.word(word, ids);
    }
    ids.length = Math.min(ids.length, limit - 1);
    ids.push(EOS);
    return ids;
  }
}

/// Texts of different lengths as one block of `longest` ids each, and which
/// of those ids are text rather than filling.
export function padded(texts: number[][]) {
  const longest = Math.max(...texts.map((ids) => ids.length));
  const ids = new BigInt64Array(texts.length * longest).fill(BigInt(PAD));
  const mask = new BigInt64Array(texts.length * longest);
  texts.forEach((text, row) =>
    text.forEach((id, column) => {
      ids[row * longest + column] = BigInt(id);
      mask[row * longest + column] = 1n;
    }),
  );
  return { ids, mask, longest };
}

/// One vector a text: the mean of the model's vectors for its tokens (not
/// for the filling), scaled to length one so that a dot product of two is
/// the cosine of the angle between them.
export function pooled(hidden: Float32Array, mask: BigInt64Array, texts: number, tokens: number, size: number): Float32Array[] {
  const vectors: Float32Array[] = [];
  for (let text = 0; text < texts; text++) {
    const sum = new Float64Array(size);
    let counted = 0;
    for (let token = 0; token < tokens; token++) {
      if (mask[text * tokens + token] === 0n) continue;
      counted++;
      const at = (text * tokens + token) * size;
      for (let i = 0; i < size; i++) sum[i] += hidden[at + i];
    }
    let norm = 0;
    for (let i = 0; i < size; i++) {
      sum[i] /= Math.max(counted, 1);
      norm += sum[i] * sum[i];
    }
    norm = Math.sqrt(norm) || 1;
    vectors.push(Float32Array.from(sum, (value) => value / norm));
  }
  return vectors;
}

/// How a text is put to the model: E5 was trained with these two words in
/// front, and finds little without them.
export function prefixed(role: "query" | "passage", text: string) {
  return `${role}: ${text}`;
}
