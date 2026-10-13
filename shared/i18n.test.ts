import { afterEach, describe, expect, it } from "vitest";

import { language, locale, onLanguage, speak, t, tc, tn, translate } from "./i18n";

const english = { language: "en", plural: [], messages: {} };
// Russian's rule, written out as the host writes it: a form for each
// number up to 199.
const russian = Array.from({ length: 200 }, (_, n) => (n % 10 === 1 && n % 100 !== 11 ? 0 : n % 10 >= 2 && n % 10 <= 4 && (n % 100 < 10 || n % 100 >= 20) ? 1 : 2));

afterEach(() => speak(english));

describe("a text", () => {
  it("is itself until another language is spoken, and where that language lacks it", () => {
    expect(t("Actions")).toBe("Actions");
    speak({ language: "zh_CN", plural: [0], messages: { Actions: ["操作"] } });
    expect([language(), t("Actions"), t("No actions")]).toEqual(["zh_CN", "操作", "No actions"]);
  });

  it("has its placeholders filled: the next, one by its place, and what is missing left to be seen", () => {
    expect(t("{} of {}", 1, 2)).toBe("1 of 2");
    speak({ language: "zh_CN", plural: [0], messages: { "{0} from {1}": ["来自 {1} 的 {0}"] } });
    expect(t("{0} from {1}", "a", "b")).toBe("来自 b 的 a");
    expect(t("{2} and {n} and {")).toBe("{2} and {n} and {");
  });

  it("is looked up as it stands when it was marked elsewhere", () => {
    speak({ language: "zh_CN", plural: [0], messages: { "Clipboard History": ["剪贴板历史"] } });
    expect(["Clipboard History", "Another Command"].map(translate)).toEqual(["剪贴板历史", "Another Command"]);
  });

  it("is another message under a context", () => {
    speak({ language: "zh_CN", plural: [0], messages: { Open: ["打开"], "state\u0004Open": ["已打开"] } });
    expect([t("Open"), tc("state", "Open"), tc("no such context", "Open")]).toEqual(["打开", "已打开", "Open"]);
  });
});

describe("the language", () => {
  it("is named as dates and numbers take it", () => {
    expect(locale()).toBe("en");
    speak({ language: "zh_CN", plural: [0], messages: {} });
    expect([locale(), new Date(2026, 9, 13).toLocaleDateString(locale(), { month: "short", day: "numeric" })]).toEqual(["zh-CN", "10月13日"]);
  });
});

describe("a number of things", () => {
  it("is one or many in English", () => {
    expect([0, 1, 2].map((n) => tn("{n} entry", "{n} entries", n))).toEqual(["0 entries", "1 entry", "2 entries"]);
    expect(tn("{0} entry", "{0} entries", 1234, "1,234")).toBe("1,234 entries");
  });

  it("is written with its thousands apart, by what the language parts them with", () => {
    expect([999, 1000, 1234567].map((n) => tn("{n} entry", "{n} entries", n))).toEqual(["999 entries", "1,000 entries", "1,234,567 entries"]);
    speak({ language: "de", plural: [1, 0], messages: { "between a number's thousands\u0004,": ["."] } });
    expect(tn("{n} entry", "{n} entries", 12345)).toBe("12.345 entries");
  });

  it("takes the form its language has for it, past what the host wrote out too", () => {
    speak({ language: "ru", plural: russian, messages: { "{n} entry": ["{n} запись", "{n} записи", "{n} записей"] } });
    expect([1, 3, 5, 21, 111, 1001, 1002, 1011].map((n) => tn("{n} entry", "{n} entries", n))).toEqual(["1 запись", "3 записи", "5 записей", "21 запись", "111 записей", "1,001 запись", "1,002 записи", "1,011 записей"]);
    speak({ language: "zh_CN", plural: Array(200).fill(0), messages: { "{n} entry": ["{n} 个条目"] } });
    expect(tn("{n} entry", "{n} entries", 1)).toBe("1 个条目");
  });
});

it("tells whoever listens that another language is spoken, until they stop", () => {
  let heard = 0;
  const stop = onLanguage(() => heard++);
  speak(english);
  stop();
  speak(english);
  expect(heard).toBe(1);
});
