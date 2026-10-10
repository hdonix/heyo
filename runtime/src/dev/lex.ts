//! Just enough of a JavaScript/TypeScript lexer to tell code from comments
//! and strings, which is what rewriting imports and looking for macOS-only
//! code need. It does not parse.
//!
//! It is lenient where telling exactly would take a parser: text inside JSX
//! (`<p>don't</p>`) can look like the start of a string. A quoted string or
//! a regular expression therefore never runs past the end of its line, so a
//! wrong guess costs one line at most.

export interface Token {
  kind: "word" | "number" | "string" | "template" | "regex" | "comment" | "punct";
  start: number;
  end: number;
}

/// Words after which a `/` starts a regular expression, not a division.
const BEFORE_REGEX = new Set(["return", "typeof", "case", "do", "else", "in", "of", "void", "delete", "throw", "new", "instanceof", "yield", "await"]);

const isWordStart = (c: string) => /[A-Za-z_$\u0080-￿]/.test(c);
const isWordPart = (c: string) => /[\w$\u0080-￿]/.test(c);

/// The end of the template literal that starts at `start` (a backtick),
/// with whatever its `${…}` hold.
function templateEnd(source: string, start: number): number {
  let i = start + 1;
  while (i < source.length) {
    const c = source[i];
    if (c === "\\") i += 2;
    else if (c === "`") return i + 1;
    else if (c === "$" && source[i + 1] === "{") i = expressionEnd(source, i + 2);
    else i++;
  }
  return source.length;
}

/// The end of the `${…}` whose contents start at `start`.
function expressionEnd(source: string, start: number): number {
  let depth = 1;
  let i = start;
  while (i < source.length) {
    const c = source[i];
    if (c === "`") i = templateEnd(source, i);
    else if (c === '"' || c === "'") i = stringEnd(source, i);
    else {
      if (c === "{") depth++;
      else if (c === "}" && --depth === 0) return i + 1;
      i++;
    }
  }
  return source.length;
}

function stringEnd(source: string, start: number): number {
  const quote = source[start];
  let i = start + 1;
  while (i < source.length) {
    const c = source[i];
    if (c === "\\") i += 2;
    else if (c === quote) return i + 1;
    else if (c === "\n") return i;
    else i++;
  }
  return source.length;
}

/// The end of the regular expression starting at `start`, or -1 if the line
/// ends before it does (then it was not one).
function regexEnd(source: string, start: number): number {
  let inClass = false;
  for (let i = start + 1; i < source.length; i++) {
    const c = source[i];
    if (c === "\n") return -1;
    if (c === "\\") i++;
    else if (c === "[") inClass = true;
    else if (c === "]") inClass = false;
    else if (c === "/" && !inClass) {
      let end = i + 1;
      while (end < source.length && /[a-z]/.test(source[end])) end++;
      return end;
    }
  }
  return -1;
}

export function lex(source: string): Token[] {
  const tokens: Token[] = [];
  /// The last token that is not a comment.
  let last: Token | undefined;
  const push = (kind: Token["kind"], start: number, end: number) => {
    const token = { kind, start, end };
    tokens.push(token);
    if (kind !== "comment") last = token;
  };
  let i = 0;
  while (i < source.length) {
    const c = source[i];
    if (/\s/.test(c)) {
      i++;
    } else if (c === "/" && source[i + 1] === "/") {
      let end = source.indexOf("\n", i);
      if (end < 0) end = source.length;
      push("comment", i, end);
      i = end;
    } else if (c === "/" && source[i + 1] === "*") {
      const close = source.indexOf("*/", i + 2);
      const end = close < 0 ? source.length : close + 2;
      push("comment", i, end);
      i = end;
    } else if (c === '"' || c === "'") {
      const end = stringEnd(source, i);
      push("string", i, end);
      i = end;
    } else if (c === "`") {
      const end = templateEnd(source, i);
      push("template", i, end);
      i = end;
    } else if (c === "/") {
      const text = last && source.slice(last.start, last.end);
      const after =
        !last ||
        (last.kind === "punct" && ![")", "]", "}", "<"].includes(text!)) ||
        (last.kind === "word" && BEFORE_REGEX.has(text!));
      // `</tag>` and `/>` are JSX.
      const end = after && source[i + 1] !== ">" ? regexEnd(source, i) : -1;
      if (end > 0) {
        push("regex", i, end);
        i = end;
      } else {
        push("punct", i, i + 1);
        i++;
      }
    } else if (isWordStart(c)) {
      let end = i + 1;
      while (end < source.length && isWordPart(source[end])) end++;
      push("word", i, end);
      i = end;
    } else if (/\d/.test(c)) {
      let end = i + 1;
      while (end < source.length && /[\w.]/.test(source[end])) end++;
      push("number", i, end);
      i = end;
    } else {
      push("punct", i, i + 1);
      i++;
    }
  }
  return tokens;
}

/// The source with its comments blanked out, every character still where
/// it was, so positions and line numbers hold.
export function withoutComments(source: string, tokens = lex(source)): string {
  let out = "";
  let at = 0;
  for (const token of tokens) {
    if (token.kind !== "comment") continue;
    out += source.slice(at, token.start) + source.slice(token.start, token.end).replace(/[^\n]/g, " ");
    at = token.end;
  }
  return out + source.slice(at);
}
