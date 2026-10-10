// Raycast-style dynamic placeholders for quicklinks and snippets, shared by
// the palette (src/) and the extension runtime (runtime/).
//
//   {query} / {argument}                 text typed by the user
//   {argument name="x" default="y"}       same, with a fallback
//   {clipboard}                          current clipboard text
//   {date} {time} {datetime} {day}       localized now
//   {date format="yyyy-MM-dd HH:mm"}     formatted now; words go in single
//                                        quotes: "d MMMM 'at' HH:mm"
//   {uuid}                               a random UUID
//   {cursor}                             removed (no cursor placement)

const TOKEN = /\{(\w+)((?:\s+\w+="[^"]*")*)\s*\}/g;
const KNOWN = new Set(["query", "argument", "clipboard", "date", "time", "datetime", "day", "uuid", "cursor"]);

export interface PlaceholderContext {
  /// Text for `{query}` / `{argument}`.
  query?: string;
  readClipboard(): Promise<string>;
  /// URL-encode inserted values (for links).
  encode?: boolean;
}

function options(raw: string) {
  return Object.fromEntries([...raw.matchAll(/(\w+)="([^"]*)"/g)].map((m) => [m[1], m[2]]));
}

/// Whether the template asks the user for text.
export function needsArgument(template: string) {
  return [...template.matchAll(TOKEN)].some((m) => m[1] === "query" || m[1] === "argument");
}

const pad = (n: number, width = 2) => String(n).padStart(width, "0");

/// Formats a date with Unicode/Raycast-style tokens (yyyy, MM, dd, HH, mm, …).
/// Text in single quotes is kept as it is, and `''` is a quote.
export function formatDate(date: Date, format: string) {
  const month = (style: "long" | "short") => date.toLocaleString(undefined, { month: style });
  const weekday = (style: "long" | "short") => date.toLocaleString(undefined, { weekday: style });
  const tokens: Record<string, () => string> = {
    yyyy: () => String(date.getFullYear()),
    yy: () => pad(date.getFullYear() % 100),
    MMMM: () => month("long"),
    MMM: () => month("short"),
    MM: () => pad(date.getMonth() + 1),
    M: () => String(date.getMonth() + 1),
    dd: () => pad(date.getDate()),
    d: () => String(date.getDate()),
    EEEE: () => weekday("long"),
    EEE: () => weekday("short"),
    HH: () => pad(date.getHours()),
    H: () => String(date.getHours()),
    hh: () => pad(date.getHours() % 12 || 12),
    h: () => String(date.getHours() % 12 || 12),
    mm: () => pad(date.getMinutes()),
    m: () => String(date.getMinutes()),
    ss: () => pad(date.getSeconds()),
    s: () => String(date.getSeconds()),
    a: () => (date.getHours() < 12 ? "AM" : "PM"),
  };
  // Longer tokens are listed before the shorter ones they begin with.
  const pattern = new RegExp(`''|'(?:[^']|'')*'|${Object.keys(tokens).join("|")}`, "g");
  return format.replace(pattern, (token) => {
    if (token === "''") return "'";
    if (token.startsWith("'")) return token.slice(1, -1).replace(/''/g, "'");
    return tokens[token]();
  });
}

export async function expand(template: string, context: PlaceholderContext): Promise<string> {
  const matches = [...template.matchAll(TOKEN)].filter((m) => KNOWN.has(m[1]));
  // A clipboard that cannot be read is an empty one: the rest still expands.
  const clipboard = matches.some((m) => m[1] === "clipboard") ? await context.readClipboard().catch(() => "") : "";
  const now = new Date();
  let out = "";
  let last = 0;
  for (const m of matches) {
    const opts = options(m[2]);
    let value: string;
    switch (m[1]) {
      case "query":
      case "argument":
        value = context.query || opts.default || "";
        break;
      case "clipboard":
        value = clipboard;
        break;
      case "date":
        value = opts.format ? formatDate(now, opts.format) : now.toLocaleDateString();
        break;
      case "time":
        value = opts.format ? formatDate(now, opts.format) : now.toLocaleTimeString();
        break;
      case "datetime":
        value = opts.format ? formatDate(now, opts.format) : now.toLocaleString();
        break;
      case "day":
        value = now.toLocaleDateString(undefined, { weekday: "long" });
        break;
      case "uuid":
        value = crypto.randomUUID();
        break;
      default:
        value = "";
    }
    if (context.encode && m[1] !== "cursor") value = encodeURIComponent(value);
    out += template.slice(last, m.index) + value;
    last = m.index! + m[0].length;
  }
  return out + template.slice(last);
}
