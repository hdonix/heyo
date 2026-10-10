// The root search calculator: arithmetic, unit and currency conversion, and
// time zones, e.g. `2^10`, `15% of 80`, `5 km in mi`, `100 usd in cny`,
// `time in tokyo`, `3pm pst in beijing`.

export interface Calculation {
  question: string;
  /// Formatted for display.
  answer: string;
  /// Plain value for copying.
  value: string;
  /// What kind of result this is, shown under the answer.
  label: string;
}

// --- Arithmetic --------------------------------------------------------------

const FUNCTIONS: Record<string, (x: number) => number> = {
  sqrt: Math.sqrt,
  cbrt: Math.cbrt,
  abs: Math.abs,
  round: Math.round,
  floor: Math.floor,
  ceil: Math.ceil,
  sin: Math.sin,
  cos: Math.cos,
  tan: Math.tan,
  asin: Math.asin,
  acos: Math.acos,
  atan: Math.atan,
  log: Math.log10,
  lg: Math.log10,
  ln: Math.log,
  log2: Math.log2,
  exp: Math.exp,
};

const CONSTANTS: Record<string, number> = { pi: Math.PI, "π": Math.PI, e: Math.E, tau: 2 * Math.PI };

/// Looks a name the user typed up in a table. Only what the table itself
/// holds counts: "constructor" is a property of every object, not a unit.
function lookup<T>(table: Record<string, T>, name: string): T | undefined {
  return Object.prototype.hasOwnProperty.call(table, name) ? table[name] : undefined;
}

type Token = { type: "num"; value: number } | { type: "op"; value: string } | { type: "id"; value: string };

function tokenize(input: string): Token[] | null {
  const tokens: Token[] = [];
  // Thousands separators: 1,000,000
  const text = input.replace(/(\d),(?=\d{3}(?!\d))/g, "$1");
  let i = 0;
  while (i < text.length) {
    const rest = text.slice(i);
    const space = /^\s+/.exec(rest);
    if (space) {
      i += space[0].length;
      continue;
    }
    const num = /^(\d+\.?\d*|\.\d+)(e[+-]?\d+)?/i.exec(rest);
    if (num) {
      tokens.push({ type: "num", value: parseFloat(num[0]) });
      i += num[0].length;
      continue;
    }
    const id = /^([a-zπ][a-z0-9]*)/i.exec(rest);
    if (id) {
      const word = id[0].toLowerCase();
      // "x" between operands means multiplication.
      tokens.push(word === "x" ? { type: "op", value: "*" } : word === "mod" ? { type: "op", value: "mod" } : { type: "id", value: word });
      i += id[0].length;
      continue;
    }
    const op = /^(\*\*|[-+*/^%()!×÷−])/.exec(rest);
    if (!op) return null;
    const value = { "×": "*", "÷": "/", "−": "-", "**": "^" }[op[0]] ?? op[0];
    tokens.push({ type: "op", value });
    i += op[0].length;
  }
  return tokens;
}

/// A value that may be a bare percentage ("15%"), which `+`/`-` apply
/// relative to the left operand: `80 + 15%` is 92.
interface Value {
  n: number;
  percent?: boolean;
}

class Parser {
  i = 0;
  readonly tokens: Token[];
  constructor(tokens: Token[]) {
    this.tokens = tokens;
  }

  peek(value?: string) {
    const t = this.tokens[this.i];
    return t && (value === undefined || (t.type !== "num" && t.value === value)) ? t : undefined;
  }
  take(value: string) {
    if (this.peek(value)) {
      this.i++;
      return true;
    }
    return false;
  }

  expr(): Value {
    let left = this.term();
    for (;;) {
      if (this.take("+")) {
        const right = this.term();
        left = { n: right.percent ? left.n * (1 + right.n) : left.n + right.n };
      } else if (this.take("-")) {
        const right = this.term();
        left = { n: right.percent ? left.n * (1 - right.n) : left.n - right.n };
      } else return left;
    }
  }

  term(): Value {
    let left = this.unary();
    for (;;) {
      if (this.take("*")) left = { n: left.n * this.unary().n };
      else if (this.take("/")) left = { n: left.n / this.unary().n };
      else if (this.take("mod")) left = { n: left.n % this.unary().n };
      else return left;
    }
  }

  unary(): Value {
    if (this.take("-")) return { n: -this.unary().n };
    if (this.take("+")) return this.unary();
    return this.power();
  }

  power(): Value {
    const base = this.postfix();
    if (this.take("^")) return { n: Math.pow(base.n, this.unary().n) };
    return base;
  }

  postfix(): Value {
    let value = this.primary();
    for (;;) {
      if (this.take("%")) {
        // "15% of 80"
        const of = this.tokens[this.i];
        if (of?.type === "id" && of.value === "of") {
          this.i++;
          value = { n: (value.n / 100) * this.unary().n };
        } else value = { n: value.n / 100, percent: true };
      } else if (this.take("!")) {
        if (!Number.isInteger(value.n) || value.n < 0 || value.n > 170) throw new Error("factorial");
        let f = 1;
        for (let k = 2; k <= value.n; k++) f *= k;
        value = { n: f };
      } else return value;
    }
  }

  primary(): Value {
    const t = this.tokens[this.i++];
    if (!t) throw new Error("unexpected end");
    if (t.type === "num") return { n: t.value };
    if (t.type === "op" && t.value === "(") {
      const inner = this.expr();
      if (!this.take(")")) throw new Error("missing )");
      return inner;
    }
    if (t.type === "id") {
      const apply = lookup(FUNCTIONS, t.value);
      if (apply) {
        const arg = this.peek("(") ? this.primary() : this.unary();
        return { n: apply(arg.n) };
      }
      const constant = lookup(CONSTANTS, t.value);
      if (constant !== undefined) return { n: constant };
    }
    throw new Error(`unexpected ${t.value}`);
  }
}

export function evaluate(input: string): number | null {
  const tokens = tokenize(input);
  if (!tokens?.length) return null;
  try {
    const parser = new Parser(tokens);
    const result = parser.expr();
    if (parser.i !== tokens.length || !Number.isFinite(result.n)) return null;
    return result.n;
  } catch {
    return null;
  }
}

function plain(n: number) {
  if (n !== 0 && (Math.abs(n) >= 1e15 || Math.abs(n) < 1e-9)) return n.toExponential(10).replace(/\.?0+e/, "e");
  return String(Number(n.toPrecision(12)));
}

function pretty(n: number) {
  if (n !== 0 && (Math.abs(n) >= 1e15 || Math.abs(n) < 1e-9)) return plain(n);
  return Number(n.toPrecision(12)).toLocaleString("en-US", { maximumFractionDigits: 10 });
}

// --- Units -------------------------------------------------------------------

interface Unit {
  category: string;
  /// Multiplier to the category's base unit.
  factor: number;
  label: string;
}

const UNITS = new Map<string, Unit>();
function units(category: string, entries: [string[], number, string][]) {
  for (const [names, factor, label] of entries) for (const name of names) UNITS.set(name, { category, factor, label });
}
units("length", [
  [["mm", "millimeter", "millimeters"], 0.001, "mm"],
  [["cm", "centimeter", "centimeters"], 0.01, "cm"],
  [["m", "meter", "meters", "metre", "metres"], 1, "m"],
  [["km", "kilometer", "kilometers"], 1000, "km"],
  [["in", "inch", "inches", '"'], 0.0254, "in"],
  [["ft", "foot", "feet", "'"], 0.3048, "ft"],
  [["yd", "yard", "yards"], 0.9144, "yd"],
  [["mi", "mile", "miles"], 1609.344, "mi"],
  [["nmi"], 1852, "nmi"],
]);
units("mass", [
  [["mg", "milligram", "milligrams"], 1e-6, "mg"],
  [["g", "gram", "grams"], 0.001, "g"],
  [["kg", "kilogram", "kilograms", "kilo", "kilos"], 1, "kg"],
  [["t", "tonne", "tonnes", "ton", "tons"], 1000, "t"],
  [["lb", "lbs", "pound", "pounds"], 0.45359237, "lb"],
  [["oz", "ounce", "ounces"], 0.028349523125, "oz"],
  [["st", "stone"], 6.35029318, "st"],
  [["jin", "斤"], 0.5, "斤"],
]);
units("volume", [
  [["ml", "milliliter", "milliliters"], 0.001, "ml"],
  [["cl"], 0.01, "cl"],
  [["dl"], 0.1, "dl"],
  [["l", "liter", "liters", "litre", "litres"], 1, "l"],
  [["m3", "m³"], 1000, "m³"],
  [["gal", "gallon", "gallons"], 3.785411784, "gal"],
  [["qt", "quart", "quarts"], 0.946352946, "qt"],
  [["pt", "pint", "pints"], 0.473176473, "pt"],
  [["cup", "cups"], 0.2365882365, "cups"],
  [["floz"], 0.0295735295625, "fl oz"],
  [["tbsp"], 0.01478676478125, "tbsp"],
  [["tsp"], 0.00492892159375, "tsp"],
]);
units("time", [
  [["ms", "millisecond", "milliseconds"], 0.001, "ms"],
  [["s", "sec", "secs", "second", "seconds"], 1, "s"],
  [["min", "mins", "minute", "minutes"], 60, "min"],
  [["h", "hr", "hrs", "hour", "hours"], 3600, "h"],
  [["d", "day", "days"], 86400, "days"],
  [["wk", "week", "weeks"], 604800, "weeks"],
  [["month", "months"], 2629746, "months"],
  [["yr", "year", "years"], 31556952, "years"],
]);
units("data", [
  [["bit", "bits"], 0.125, "bits"],
  [["B", "byte", "bytes"], 1, "B"],
  [["KB", "kb"], 1e3, "KB"],
  [["MB", "mb"], 1e6, "MB"],
  [["GB", "gb"], 1e9, "GB"],
  [["TB", "tb"], 1e12, "TB"],
  [["PB", "pb"], 1e15, "PB"],
  [["KiB", "kib"], 1024, "KiB"],
  [["MiB", "mib"], 1024 ** 2, "MiB"],
  [["GiB", "gib"], 1024 ** 3, "GiB"],
  [["TiB", "tib"], 1024 ** 4, "TiB"],
]);
units("speed", [
  [["m/s", "mps"], 1, "m/s"],
  [["km/h", "kmh", "kph"], 1 / 3.6, "km/h"],
  [["mph"], 0.44704, "mph"],
  [["kn", "knot", "knots"], 0.514444, "kn"],
]);
units("area", [
  [["cm2", "cm²"], 1e-4, "cm²"],
  [["m2", "m²", "sqm"], 1, "m²"],
  [["km2", "km²"], 1e6, "km²"],
  [["ha", "hectare", "hectares"], 1e4, "ha"],
  [["acre", "acres"], 4046.8564224, "acres"],
  [["ft2", "ft²", "sqft"], 0.09290304, "ft²"],
  [["mi2", "mi²"], 2589988.110336, "mi²"],
]);

const TEMPERATURES: Record<string, string> = {
  c: "°C",
  "°c": "°C",
  celsius: "°C",
  f: "°F",
  "°f": "°F",
  fahrenheit: "°F",
  k: "K",
  kelvin: "K",
};

function toCelsius(n: number, unit: string) {
  return unit === "°F" ? ((n - 32) * 5) / 9 : unit === "K" ? n - 273.15 : n;
}
function fromCelsius(n: number, unit: string) {
  return unit === "°F" ? (n * 9) / 5 + 32 : unit === "K" ? n + 273.15 : n;
}

const unit = (name: string) => UNITS.get(name) ?? UNITS.get(name.toLowerCase());

const CONVERSION = /^(.+?)\s*([a-zA-Z°²³µ'"/斤]+[0-9²³]?)\s+(?:in|to|as|=)\s+([a-zA-Z°²³µ'"/斤]+[0-9²³]?)$/i;

function convertUnits(query: string): Calculation | null {
  const m = CONVERSION.exec(query);
  if (!m) return null;
  const amount = evaluate(m[1]);
  if (amount === null) return null;
  const [from, to] = [m[2], m[3]];
  const [tFrom, tTo] = [lookup(TEMPERATURES, from.toLowerCase()), lookup(TEMPERATURES, to.toLowerCase())];
  if (tFrom && tTo) {
    const result = fromCelsius(toCelsius(amount, tFrom), tTo);
    return { question: `${pretty(amount)} ${tFrom}`, answer: `${pretty(result)} ${tTo}`, value: plain(result), label: "Temperature" };
  }
  const [uFrom, uTo] = [unit(from), unit(to)];
  if (!uFrom || !uTo || uFrom.category !== uTo.category) return null;
  const result = (amount * uFrom.factor) / uTo.factor;
  const category = uFrom.category[0].toUpperCase() + uFrom.category.slice(1);
  return { question: `${pretty(amount)} ${uFrom.label}`, answer: `${pretty(result)} ${uTo.label}`, value: plain(result), label: category };
}

// --- Currency ----------------------------------------------------------------

export interface Rates {
  /// Units of each currency per US dollar.
  rates: Record<string, number>;
  /// Milliseconds since the epoch.
  updated: number;
}

const SYMBOLS: Record<string, string> = { $: "USD", "€": "EUR", "£": "GBP", "¥": "CNY", "￥": "CNY", "元": "CNY", "₩": "KRW", "₹": "INR", "₽": "RUB" };
const RATES_KEY = "heyo.currencyRates";
const RATES_TTL = 6 * 3600 * 1000;

function cachedRates(): Rates | null {
  try {
    return JSON.parse(localStorage.getItem(RATES_KEY) ?? "null");
  } catch {
    return null;
  }
}

let pending: Promise<Rates | null> | null = null;

/// Exchange rates, refreshed when older than six hours.
export async function loadRates(): Promise<Rates | null> {
  const cached = cachedRates();
  if (cached && Date.now() - cached.updated < RATES_TTL) return cached;
  pending ??= fetch("https://open.er-api.com/v6/latest/USD")
    .then((r) => r.json())
    .then((json) => {
      if (json.result !== "success") return cached;
      const rates: Rates = { rates: json.rates, updated: json.time_last_update_unix * 1000 };
      try {
        localStorage.setItem(RATES_KEY, JSON.stringify(rates));
      } catch {}
      return rates;
    })
    .catch(() => cached)
    .finally(() => (pending = null));
  return pending;
}

function currencyCode(text: string, rates: Rates) {
  const code = lookup(SYMBOLS, text) ?? text.toUpperCase();
  return lookup(rates.rates, code) !== undefined ? code : null;
}

const CURRENCY = /^([$€£¥￥₩₹₽])?\s*([\d.,]+(?:\s*[-+*/^×÷]\s*[\d.,]+)*)\s*([a-zA-Z]{3}|[$€£¥￥元₩₹₽])?\s+(?:in|to|as|=)\s+([a-zA-Z]{3}|[$€£¥￥元₩₹₽])$/i;

function convertCurrency(query: string, rates: Rates | null): Calculation | null {
  const m = CURRENCY.exec(query);
  if (!m || !rates || !(m[1] || m[3])) return null;
  const amount = evaluate(m[2]);
  const from = currencyCode(m[1] ?? m[3], rates);
  const to = currencyCode(m[4], rates);
  if (amount === null || !from || !to) return null;
  const result = (amount / rates.rates[from]) * rates.rates[to];
  const money = (n: number, code: string) =>
    `${n.toLocaleString("en-US", { maximumFractionDigits: n < 1 ? 6 : 2, minimumFractionDigits: n < 1 ? 0 : 2 })} ${code}`;
  const date = new Date(rates.updated).toLocaleDateString(undefined, { dateStyle: "medium" });
  return {
    question: money(amount, from),
    answer: money(result, to),
    // As many digits as the answer shows: two would make a small amount 0.00.
    value: Math.abs(result) < 1 ? String(Number(result.toFixed(6))) : result.toFixed(2),
    label: `Currency · rates of ${date}`,
  };
}

// --- Time zones --------------------------------------------------------------

const ZONES: Record<string, string> = {
  utc: "UTC",
  gmt: "UTC",
  pst: "America/Los_Angeles",
  pdt: "America/Los_Angeles",
  pt: "America/Los_Angeles",
  "los angeles": "America/Los_Angeles",
  la: "America/Los_Angeles",
  "san francisco": "America/Los_Angeles",
  sf: "America/Los_Angeles",
  seattle: "America/Los_Angeles",
  vancouver: "America/Vancouver",
  mst: "America/Denver",
  mdt: "America/Denver",
  denver: "America/Denver",
  cst: "America/Chicago",
  cdt: "America/Chicago",
  chicago: "America/Chicago",
  est: "America/New_York",
  edt: "America/New_York",
  et: "America/New_York",
  "new york": "America/New_York",
  ny: "America/New_York",
  nyc: "America/New_York",
  boston: "America/New_York",
  toronto: "America/Toronto",
  "mexico city": "America/Mexico_City",
  "sao paulo": "America/Sao_Paulo",
  "são paulo": "America/Sao_Paulo",
  london: "Europe/London",
  uk: "Europe/London",
  bst: "Europe/London",
  dublin: "Europe/Dublin",
  cet: "Europe/Paris",
  cest: "Europe/Paris",
  paris: "Europe/Paris",
  berlin: "Europe/Berlin",
  germany: "Europe/Berlin",
  amsterdam: "Europe/Amsterdam",
  madrid: "Europe/Madrid",
  rome: "Europe/Rome",
  stockholm: "Europe/Stockholm",
  zurich: "Europe/Zurich",
  istanbul: "Europe/Istanbul",
  moscow: "Europe/Moscow",
  msk: "Europe/Moscow",
  cairo: "Africa/Cairo",
  johannesburg: "Africa/Johannesburg",
  dubai: "Asia/Dubai",
  india: "Asia/Kolkata",
  ist: "Asia/Kolkata",
  mumbai: "Asia/Kolkata",
  delhi: "Asia/Kolkata",
  bangalore: "Asia/Kolkata",
  bangkok: "Asia/Bangkok",
  jakarta: "Asia/Jakarta",
  singapore: "Asia/Singapore",
  sgt: "Asia/Singapore",
  "kuala lumpur": "Asia/Kuala_Lumpur",
  manila: "Asia/Manila",
  china: "Asia/Shanghai",
  beijing: "Asia/Shanghai",
  shanghai: "Asia/Shanghai",
  shenzhen: "Asia/Shanghai",
  guangzhou: "Asia/Shanghai",
  "hong kong": "Asia/Hong_Kong",
  hk: "Asia/Hong_Kong",
  taipei: "Asia/Taipei",
  taiwan: "Asia/Taipei",
  北京: "Asia/Shanghai",
  上海: "Asia/Shanghai",
  深圳: "Asia/Shanghai",
  香港: "Asia/Hong_Kong",
  台北: "Asia/Taipei",
  东京: "Asia/Tokyo",
  纽约: "America/New_York",
  伦敦: "Europe/London",
  tokyo: "Asia/Tokyo",
  japan: "Asia/Tokyo",
  jst: "Asia/Tokyo",
  seoul: "Asia/Seoul",
  korea: "Asia/Seoul",
  kst: "Asia/Seoul",
  sydney: "Australia/Sydney",
  melbourne: "Australia/Melbourne",
  aest: "Australia/Sydney",
  auckland: "Pacific/Auckland",
  nz: "Pacific/Auckland",
};

function zone(name: string): { id: string; label: string } | null {
  const key = name.trim().toLowerCase();
  if (key === "local" || key === "here") return { id: Intl.DateTimeFormat().resolvedOptions().timeZone, label: "Local time" };
  const known = lookup(ZONES, key);
  const id = known ?? name.trim();
  try {
    new Intl.DateTimeFormat("en-US", { timeZone: id });
  } catch {
    return null;
  }
  const label = known && key.length > 3 ? name.trim().replace(/\b\w/g, (c) => c.toUpperCase()) : id;
  return { id, label };
}

/// Offset of a time zone from UTC at `date`, in milliseconds.
function offset(date: Date, timeZone: string) {
  const parts = Object.fromEntries(
    new Intl.DateTimeFormat("en-US", {
      timeZone,
      hourCycle: "h23",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    })
      .formatToParts(date)
      .map((p) => [p.type, p.value]),
  );
  const asUtc = Date.UTC(+parts.year, +parts.month - 1, +parts.day, +parts.hour, +parts.minute, +parts.second);
  return asUtc - Math.floor(date.getTime() / 1000) * 1000;
}

function showTime(date: Date, timeZone: string) {
  const time = date.toLocaleTimeString("en-US", { timeZone, hour: "numeric", minute: "2-digit" });
  const day = date.toLocaleDateString("en-US", { timeZone, weekday: "short", month: "short", day: "numeric" });
  return { time, day };
}

/// `3pm`, `15:30`, `9:05 am` → [hours, minutes].
function parseClock(text: string): [number, number] | null {
  const m = /^(\d{1,2})(?::(\d{2}))?\s*(am|pm)?$/i.exec(text.trim());
  if (!m) return null;
  let hours = +m[1];
  const minutes = +(m[2] ?? 0);
  const meridiem = m[3]?.toLowerCase();
  if (!m[2] && !meridiem) return null;
  // "13pm" and "0am" are not times.
  if (meridiem && (hours < 1 || hours > 12)) return null;
  if (meridiem === "pm" && hours < 12) hours += 12;
  if (meridiem === "am" && hours === 12) hours = 0;
  return hours < 24 && minutes < 60 ? [hours, minutes] : null;
}

function convertTime(query: string): Calculation | null {
  // "time in tokyo", "now in london", "tokyo time"
  const now = /^(?:time|now)\s+in\s+(.+)$/i.exec(query) ?? /^(.+?)\s+time$/i.exec(query);
  if (now) {
    const target = zone(now[1]);
    if (!target) return null;
    const { time, day } = showTime(new Date(), target.id);
    return { question: `Now in ${target.label}`, answer: time, value: time, label: `${day} · ${target.id}` };
  }
  // "3pm pst in beijing", "15:00 in tokyo" (from local time)
  const conversion = /^(\d{1,2}(?::\d{2})?\s*(?:am|pm)?)\s*(.*?)\s+(?:in|to)\s+(.+)$/i.exec(query);
  if (!conversion) return null;
  const clock = parseClock(conversion[1]);
  const source = conversion[2] ? zone(conversion[2]) : zone("local");
  const target = zone(conversion[3]);
  if (!clock || !source || !target) return null;
  // That wall-clock time today in the source zone, as an instant. "Today"
  // is the source zone's: it may be tomorrow there already.
  const moment = new Date();
  const there = new Date(moment.getTime() + offset(moment, source.id));
  const wall = Date.UTC(there.getUTCFullYear(), there.getUTCMonth(), there.getUTCDate(), clock[0], clock[1]);
  const instant = new Date(wall - offset(new Date(wall), source.id));
  const from = showTime(instant, source.id);
  const to = showTime(instant, target.id);
  return {
    question: `${from.time} ${source.label}`,
    answer: `${to.time} ${target.label}`,
    value: to.time,
    label: `${to.day} · ${target.id}`,
  };
}

// --- Entry point -------------------------------------------------------------

/// The calculator result for a root search query, if it is a calculation.
export function calculate(query: string, rates: Rates | null): Calculation | null {
  const q = query.trim();
  if (!q) return null;
  const converted = convertTime(q) ?? convertCurrency(q, rates) ?? convertUnits(q);
  if (converted) return converted;
  // Plain numbers and words are not calculations.
  if (!/[-+*/^%!×÷()]|\d\s*x\s*[\d(]|\b(sqrt|cbrt|abs|round|floor|ceil|sin|cos|tan|asin|acos|atan|log|lg|ln|log2|exp|pi|tau|mod)\b|π/i.test(q)) {
    return null;
  }
  if (!/[\dπ]|pi|tau|\be\b/i.test(q)) return null;
  const result = evaluate(q);
  if (result === null) return null;
  return { question: q, answer: pretty(result), value: plain(result), label: "Calculator" };
}
