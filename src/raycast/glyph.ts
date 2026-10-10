//! Telling an emoji or a short symbol from the name of an image.
//!
//! An image given as a string is an icon name, an address, a path, the name
//! of one of the extension's assets, or a character to draw as it is. An
//! emoji can be many code points long (a family is seven, the flag of
//! Wales seven), so its length says nothing: what it is made of does.

// What an emoji starts with, and what may follow: more of the same joined
// by ZWJ, skin tones, variation selectors, the keycap mark, and the tag
// characters that spell a flag's region.
const EMOJI =
  /^(?:\p{Extended_Pictographic}|\p{Regional_Indicator}|[0-9#*](?=️?⃣))(?:\p{Extended_Pictographic}|\p{Regional_Indicator}|\p{Emoji_Modifier}|[‍︎️⃣]|[\u{E0020}-\u{E007F}])*$/u;

const LOCATION = /^(https?|data|blob):|^\//;

/// Whether `source` is drawn as text: an emoji, or a symbol of a few characters.
/// Whether a source names a file on this computer: `/usr/share/…`, or
/// `C:\Users\…` on Windows.
export function isFilePath(source: string): boolean {
  return source.startsWith("/") || /^[A-Za-z]:[\\/]/.test(source);
}

export function isGlyph(source: string): boolean {
  if (!source || LOCATION.test(source) || source.includes(".")) return false;
  return EMOJI.test(source) || [...source].length <= 4;
}
