import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { calculate, evaluate, type Rates } from "./calculator";

const answer = (query: string, rates: Rates | null = null) => calculate(query, rates)?.answer;
const value = (query: string, rates: Rates | null = null) => calculate(query, rates)?.value;

describe("arithmetic", () => {
  it("does what the header says it does", () => {
    expect(answer("2^10")).toBe("1,024");
    expect(answer("15% of 80")).toBe("12");
  });

  it("keeps the usual order of operations", () => {
    expect(evaluate("1 + 2 * 3")).toBe(7);
    expect(evaluate("(1 + 2) * 3")).toBe(9);
    expect(evaluate("10 - 4 - 3")).toBe(3);
    expect(evaluate("100 / 10 / 5")).toBe(2);
    expect(evaluate("2 ^ 3 ^ 2")).toBe(512);
    expect(evaluate("-2 ^ 2")).toBe(-4);
    expect(evaluate("2 ^ -1")).toBe(0.5);
    expect(evaluate("2 ** 8")).toBe(256);
    expect(evaluate("--3")).toBe(3);
    expect(evaluate("10 mod 3")).toBe(1);
  });

  it("reads the other ways of writing an operator", () => {
    expect(evaluate("6 × 7")).toBe(42);
    expect(evaluate("6 x 7")).toBe(42);
    expect(evaluate("42 ÷ 6")).toBe(7);
    expect(evaluate("5 − 8")).toBe(-3);
  });

  it("reads numbers as people write them", () => {
    expect(evaluate("1,000,000 / 4")).toBe(250000);
    expect(evaluate("1,234.5 + 1")).toBe(1235.5);
    expect(evaluate(".5 + .25")).toBe(0.75);
    expect(evaluate("1e3 + 1")).toBe(1001);
    expect(evaluate("2.5E-1 * 4")).toBe(1);
    // A comma that separates no thousands is not part of a number.
    expect(evaluate("1,5 + 1")).toBeNull();
  });

  it("takes a percentage of what it is added to", () => {
    expect(answer("80 + 15%")).toBe("92");
    expect(answer("80 - 25%")).toBe("60");
    expect(answer("50% of 30")).toBe("15");
    expect(answer("200 * 10%")).toBe("20");
    expect(answer("15% of (40 + 40)")).toBe("12");
  });

  it("knows functions, constants and factorials", () => {
    expect(evaluate("sqrt(16)")).toBe(4);
    expect(evaluate("sqrt 16 + 1")).toBe(5);
    expect(evaluate("abs(-3) + round(2.5) + floor(2.9) + ceil(2.1)")).toBe(11);
    expect(evaluate("log(1000)")).toBeCloseTo(3);
    expect(evaluate("ln(e)")).toBeCloseTo(1);
    expect(evaluate("log2(8)")).toBe(3);
    expect(evaluate("sin(0) + cos(0)")).toBe(1);
    expect(evaluate("2 * pi")).toBeCloseTo(2 * Math.PI);
    expect(evaluate("2 * π")).toBeCloseTo(2 * Math.PI);
    expect(evaluate("tau / 2")).toBeCloseTo(Math.PI);
    expect(evaluate("5!")).toBe(120);
    expect(evaluate("0!")).toBe(1);
    expect(evaluate("3! + 1")).toBe(7);
    expect(evaluate("SQRT(16)")).toBe(4);
  });

  it("answers nothing rather than something wrong", () => {
    for (const broken of ["", "2 +", "(2 + 3", "2 + 3)", "2 3", "1 / 0", "0 / 0", "sqrt(-1)", "2.5!", "(-1)!", "171!", "foo(2)", "2 + bar", "2 $ 3", "3 # 4"]) {
      expect(evaluate(broken), broken).toBeNull();
    }
  });

  it("does not take a property of every object for a function or a constant", () => {
    for (const name of ["constructor", "toString", "valueOf", "hasOwnProperty", "__proto__"]) {
      expect(evaluate(`${name}(2)`), name).toBeNull();
      expect(evaluate(`${name} + 1`), name).toBeNull();
      expect(calculate(`${name}(2)`, null), name).toBeNull();
    }
  });

  it("shows a result without the noise of binary fractions", () => {
    expect(answer("0.1 + 0.2")).toBe("0.3");
    expect(answer("1 / 3")).toBe("0.3333333333");
    expect(value("1 / 3")).toBe("0.333333333333");
    expect(answer("1000 * 1000")).toBe("1,000,000");
    expect(value("1000 * 1000")).toBe("1000000");
    expect(answer("2^70")).toBe("1.1805916207e+21");
    expect(answer("1 / 10^12")).toBe("1e-12");
    expect(answer("5 - 5")).toBe("0");
    // A constant by itself is worth showing.
    expect(answer("pi")).toBe("3.1415926536");
    expect(calculate("3 * 4", null)).toEqual({ question: "3 * 4", answer: "12", value: "12", label: "Calculator" });
  });

  it("leaves alone what is not a calculation", () => {
    for (const text of ["", "   ", "42", "firefox", "e", "log", "x", "a - b", "c++", "50/50 chance", "sin", "3 apples", "!", "()", "time", "node --version"]) {
      expect(calculate(text, null), text).toBeNull();
    }
  });
});

describe("unit conversion", () => {
  it("converts within a kind of unit", () => {
    expect(answer("5 km in mi")).toBe("3.1068559612 mi");
    expect(answer("1 mile to km")).toBe("1.609344 km");
    expect(answer("12 in in cm")).toBe("30.48 cm");
    expect(answer("6 ft as m")).toBe("1.8288 m");
    expect(answer("1 kg in lb")).toBe("2.2046226219 lb");
    expect(answer("2 hours in minutes")).toBe("120 min");
    expect(answer("1 day in s")).toBe("86,400 s");
    expect(answer("1 GiB in MiB")).toBe("1,024 MiB");
    expect(answer("1 gb in mb")).toBe("1,000 MB");
    expect(answer("1 B in bits")).toBe("8 bits");
    expect(answer("100 km/h in mph")).toBe("62.1371192237 mph");
    expect(answer("1 acre in m2")).toBe("4,046.8564224 m²");
    expect(answer("2 l in ml")).toBe("2,000 ml");
    expect(answer("1 斤 in g")).toBe("500 g");
    expect(calculate("5 km in mi", null)).toEqual({ question: "5 km", answer: "3.1068559612 mi", value: "3.10685596119", label: "Length" });
  });

  it("takes a calculation for the amount", () => {
    expect(answer("(2 + 3) km in m")).toBe("5,000 m");
    expect(answer("1.5 * 2 h in min")).toBe("180 min");
    expect(answer("1,500 m in km")).toBe("1.5 km");
  });

  it("is not particular about capitals or the space before a unit", () => {
    expect(answer("5 KM in MI")).toBe("3.1068559612 mi");
    expect(answer("5km in mi")).toBe("3.1068559612 mi");
    expect(answer("5 Miles To Km")).toBe("8.04672 km");
  });

  it("converts temperatures, which are not a matter of scale", () => {
    expect(answer("100 c in f")).toBe("212 °F");
    expect(answer("32 f to c")).toBe("0 °C");
    expect(answer("-40 f in c")).toBe("-40 °C");
    expect(answer("0 celsius in kelvin")).toBe("273.15 K");
    expect(answer("300 K in °C")).toBe("26.85 °C");
    expect(calculate("100 c in f", null)?.label).toBe("Temperature");
  });

  it("does not convert between kinds, or units it does not know", () => {
    for (const text of ["5 km in kg", "5 parsecs in km", "5 km in", "km in mi", "5 km mi", "1 constructor in constructor", "1 toString in valueOf"]) {
      expect(calculate(text, null), text).toBeNull();
    }
  });
});

describe("currency conversion", () => {
  const rates: Rates = { rates: { USD: 1, CNY: 7.2, EUR: 0.9, JPY: 150, GBP: 0.8, BTC: 0.00001 }, updated: Date.UTC(2026, 9, 10) };

  it("converts by the rates it is given", () => {
    expect(answer("100 usd in cny", rates)).toBe("720.00 CNY");
    expect(answer("100 USD to EUR", rates)).toBe("90.00 EUR");
    expect(answer("100 USD TO EUR", rates)).toBe("90.00 EUR");
    expect(answer("72 cny in usd", rates)).toBe("10.00 USD");
    expect(answer("90 eur in cny", rates)).toBe("720.00 CNY");
    expect(answer("1,000 usd in jpy", rates)).toBe("150,000.00 JPY");
    const result = calculate("100 usd in cny", rates);
    expect(result?.question).toBe("100.00 USD");
    expect(result?.value).toBe("720.00");
    expect(result?.label).toMatch(/^Currency · rates of .*2026/);
  });

  it("knows the signs", () => {
    expect(answer("$100 in €", rates)).toBe("90.00 EUR");
    expect(answer("100 $ in ¥", rates)).toBe("720.00 CNY");
    expect(answer("€90 to usd", rates)).toBe("100.00 USD");
    expect(answer("£8 in usd", rates)).toBe("10.00 USD");
    expect(answer("72 元 in usd", rates)).toBe("10.00 USD");
  });

  it("takes a sum for the amount", () => {
    expect(answer("50 + 50 usd in cny", rates)).toBe("720.00 CNY");
    expect(answer("$10 * 3 in eur", rates)).toBe("27.00 EUR");
  });

  it("keeps the digits of a small amount, in the answer and in what is copied", () => {
    expect(answer("1 usd in btc", rates)).toBe("0.00001 BTC");
    expect(value("1 usd in btc", rates)).toBe("0.00001");
    expect(value("0.5 usd in eur", rates)).toBe("0.45");
    expect(value("100 usd in eur", rates)).toBe("90.00");
  });

  it("answers nothing without rates, a source currency, or a currency it has a rate for", () => {
    expect(calculate("100 usd in cny", null)).toBeNull();
    expect(calculate("100 in cny", rates)).toBeNull();
    expect(calculate("100 usd in xyz", rates)).toBeNull();
    expect(calculate("100 abc in usd", rates)).toBeNull();
    expect(calculate("usd in cny", rates)).toBeNull();
  });
});

describe("time zones", () => {
  // Noon UTC on a day when Los Angeles and London are on summer time.
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-07-15T12:00:00Z"));
  });
  afterEach(() => vi.useRealTimers());

  it("tells the time somewhere", () => {
    expect(calculate("time in tokyo", null)).toEqual({ question: "Now in Tokyo", answer: "9:00 PM", value: "9:00 PM", label: "Wed, Jul 15 · Asia/Tokyo" });
    expect(answer("now in london")).toBe("1:00 PM");
    expect(answer("Time in New York")).toBe("8:00 AM");
    expect(answer("beijing time")).toBe("8:00 PM");
    expect(answer("time in 东京")).toBe("9:00 PM");
    expect(answer("time in utc")).toBe("12:00 PM");
    expect(answer("time in Europe/Paris")).toBe("2:00 PM");
    // The day there, not the day here.
    expect(calculate("time in auckland", null)?.label).toBe("Thu, Jul 16 · Pacific/Auckland");
  });

  it("converts a time of day from one place to another", () => {
    expect(calculate("3pm pst in beijing", null)).toEqual({ question: "3:00 PM America/Los_Angeles", answer: "6:00 AM Beijing", value: "6:00 AM", label: "Thu, Jul 16 · Asia/Shanghai" });
    expect(answer("15:30 london in tokyo")).toBe("11:30 PM Tokyo");
    expect(answer("9am tokyo to utc")).toBe("12:00 AM UTC");
    expect(answer("12am utc in tokyo")).toBe("9:00 AM Tokyo");
    expect(answer("12pm utc in tokyo")).toBe("9:00 PM Tokyo");
    expect(answer("9:05 am new york in london")).toBe("2:05 PM London");
  });

  it("starts from the local time when no place is given", () => {
    const local = Intl.DateTimeFormat().resolvedOptions().timeZone;
    const result = calculate("3pm in utc", null);
    expect(result?.question).toBe("3:00 PM Local time");
    // The same instant, read in UTC.
    const expected = new Date(new Date("2026-07-15T12:00:00Z").toLocaleDateString("en-CA", { timeZone: local }) + "T15:00:00");
    expect(result?.answer).toBe(`${expected.toLocaleTimeString("en-US", { timeZone: "UTC", hour: "numeric", minute: "2-digit" })} UTC`);
  });

  it("takes the day from the place the time is given in", () => {
    // At noon UTC it is already the 16th in Auckland: 9am there is the 16th's.
    expect(calculate("9am auckland in utc", null)?.label).toBe("Wed, Jul 15 · UTC");
    expect(answer("9am auckland in utc")).toBe("9:00 PM UTC");
    // And still the 15th in Los Angeles: 11pm there is the 16th in UTC.
    expect(calculate("11pm pst in utc", null)?.label).toBe("Thu, Jul 16 · UTC");
  });

  it("answers nothing for a place or a time it does not know", () => {
    for (const text of ["time in atlantis", "what time", "3pm in atlantis", "25:00 in tokyo", "3pm narnia in tokyo", "13pm in tokyo", "9:75 in tokyo", "time in constructor"]) {
      expect(calculate(text, null), text).toBeNull();
    }
  });

  it("leaves a conversion of units to the units", () => {
    expect(answer("5 km in mi")).toBe("3.1068559612 mi");
    expect(answer("10 m in ft")).toBe("32.8083989501 ft");
    expect(answer("3 h in min")).toBe("180 min");
  });
});
