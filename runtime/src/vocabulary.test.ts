import { describe, expect, it } from "vitest";

import { corrector, parseVocabulary, vocabularyPrompt } from "./vocabulary";

const correct = (lines: string[], text: string) => corrector(parseVocabulary(lines))(text);

describe("parseVocabulary", () => {
  it("reads terms and what they are heard as", () => {
    expect(parseVocabulary(["TypeScript", "  Heyo  ", "", "jason -> JSON", "杰森 → JSON", "k8s=>Kubernetes"])).toEqual({
      terms: ["TypeScript", "Heyo", "JSON", "Kubernetes"],
      replacements: [
        ["jason", "JSON"],
        ["杰森", "JSON"],
        ["k8s", "Kubernetes"],
      ],
    });
  });

  it("drops what says nothing", () => {
    expect(parseVocabulary(["   ", "heard ->", "->"])).toEqual({ terms: [], replacements: [] });
    expect(parseVocabulary(undefined)).toEqual({ terms: [], replacements: [] });
    // Only a way of writing it: a term.
    expect(parseVocabulary(["-> Heyo"]).terms).toEqual(["Heyo"]);
  });
});

describe("corrector, in English", () => {
  const lines = ["TypeScript", "Heyo", "Visual Studio Code", "Visual Studio", "Node.js", "Go", "jason -> JSON", "cube control -> kubectl"];

  it("writes a term as listed however it was cased and spaced", () => {
    expect(correct(lines, "I write type script every day.")).toBe("I write TypeScript every day.");
    expect(correct(lines, "Typescript, typescript and TYPE-SCRIPT.")).toBe("TypeScript, TypeScript and TypeScript.");
    expect(correct(lines, "open heyo")).toBe("open Heyo");
    expect(correct(lines, "Use node js or node.js or NodeJS.")).toBe("Use Node.js or Node.js or Node.js.");
  });

  it("takes the longest term", () => {
    expect(correct(lines, "visual studio code and visual studio")).toBe("Visual Studio Code and Visual Studio");
  });

  it("replaces what was heard with what was meant", () => {
    expect(correct(lines, "Send me the Jason file.")).toBe("Send me the JSON file.");
    expect(correct(lines, "Run Cube Control get pods.")).toBe("Run kubectl get pods.");
    // What it is written as is found too.
    expect(correct(lines, "a json file")).toBe("a JSON file");
  });

  it("does not replace inside longer words", () => {
    expect(correct(lines, "Going to Chicago, long ago.")).toBe("Going to Chicago, long ago.");
    expect(correct(lines, "heyoing and theyo")).toBe("heyoing and theyo");
    expect(correct(lines, "typescripts")).toBe("typescripts");
    expect(correct(lines, "Jasons")).toBe("Jasons");
    expect(correct(lines, "go2market")).toBe("go2market");
  });

  it("does not join words across punctuation", () => {
    expect(correct(lines, "I like that type. Script it.")).toBe("I like that type. Script it.");
    expect(correct(lines, "type, script")).toBe("type, script");
    // But a sentence may end right after a term.
    expect(correct(lines, "It is type script.")).toBe("It is TypeScript.");
  });

  it("leaves everything else exactly as it was", () => {
    const text = "  Nothing here —  to  fix…\n";
    expect(correct(lines, text)).toBe(text);
    expect(correct([], "type script")).toBe("type script");
    expect(correct(lines, "")).toBe("");
  });
});

describe("corrector, in Chinese and mixed text", () => {
  const lines = ["TypeScript", "B站", "小红书", "杰森 -> JSON", "哈克 -> Heyo", "GitHub"];

  it("finds a term in text without spaces", () => {
    expect(correct(lines, "我用typescript写代码。")).toBe("我用TypeScript写代码。");
    expect(correct(lines, "代码在 git hub 上，用 type script 写的。")).toBe("代码在 GitHub 上，用 TypeScript 写的。");
  });

  it("joins a mixed term that came out in pieces", () => {
    expect(correct(lines, "我在 b 站看视频")).toBe("我在 B站看视频");
    expect(correct(lines, "我在b站和小 红 书都有账号")).toBe("我在B站和小红书都有账号");
  });

  it("replaces Chinese that was heard for a name", () => {
    expect(correct(lines, "把杰森文件发给我，然后打开哈克。")).toBe("把JSON文件发给我，然后打开Heyo。");
  });

  it("does not replace Latin terms inside longer Latin words in Chinese text", () => {
    expect(correct(lines, "这个typescripts不是词")).toBe("这个typescripts不是词");
    expect(correct(lines, "用mygithub账号")).toBe("用mygithub账号");
  });

  it("cannot tell a homophone that is not listed", () => {
    // Same sound, other characters: nothing here knows they sound alike.
    expect(correct(lines, "把结森文件发给我")).toBe("把结森文件发给我");
  });

  it("copes with characters outside the basic plane and ones that grow when lowercased", () => {
    expect(correct(["TypeScript"], "😀 type script 😀")).toBe("😀 TypeScript 😀");
    expect(correct(["TypeScript"], "İ type script")).toBe("İ TypeScript");
    expect(correct(["𠮷野家"], "去𠮷 野家吃饭")).toBe("去𠮷野家吃饭");
  });
});

describe("corrector, at size", () => {
  it("is fast for a few hundred terms", () => {
    const lines = Array.from({ length: 400 }, (_, i) => `Term${i} Name${i % 7}`).concat(["TypeScript", "杰森 -> JSON"]);
    const started = performance.now();
    const fix = corrector(parseVocabulary(lines));
    const built = performance.now() - started;
    const sentence = "今天我们用 type script 写了一个新的功能，然后把杰森文件发给了大家，term 399 name 0 也在里面。 ".repeat(8);
    const before = performance.now();
    let out = "";
    for (let i = 0; i < 100; i++) out = fix(sentence);
    const each = (performance.now() - before) / 100;
    expect(out).toContain("TypeScript");
    expect(out).toContain("JSON");
    expect(out).toContain("Term399 Name0");
    // A partial result is corrected every 300 ms; this must be nothing beside that.
    expect(built).toBeLessThan(100);
    expect(each).toBeLessThan(5);
  });
});

describe("vocabularyPrompt", () => {
  it("lists the terms and what they may be heard as", () => {
    const prompt = vocabularyPrompt(parseVocabulary(["TypeScript", "jason -> JSON", "杰森 -> JSON"]));
    expect(prompt).toContain("- TypeScript\n");
    expect(prompt).toContain("- JSON (may appear as: jason, 杰森)");
  });

  it("says nothing without a vocabulary, and not too much with a long one", () => {
    expect(vocabularyPrompt(parseVocabulary([]))).toBe("");
    const long = vocabularyPrompt(parseVocabulary(Array.from({ length: 1000 }, (_, i) => `Word${i}`)), 50);
    expect(long.match(/^- /gm)).toHaveLength(50);
  });
});
