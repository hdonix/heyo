import { afterEach, describe, expect, it, vi } from "vitest";

import { acceptable, casesFor, localOnly, parse, prompt } from "./cases";
import type { AiSettings } from "./model";

const local: AiSettings = { provider: "local", apiKey: "", localUrl: "http://127.0.0.1:8000/v1", localModel: "tiny" };
const entry = { id: 7, text: "ssh deploy@10.0.3.17 -p 2222", app: "org.gnome.Terminal", copied: "3 days ago" };

/// A server that streams `answer` as an OpenAI-compatible one would, and
/// remembers what it was asked.
function model(answer: string) {
  const asked: { url: string; body: any }[] = [];
  const fetcher = vi.fn(async (url: string, init?: RequestInit) => {
    asked.push({ url, body: init?.body ? JSON.parse(String(init.body)) : undefined });
    const chunks = answer.match(/[\s\S]{1,20}/g) ?? [];
    const lines = chunks.map((content) => `data: ${JSON.stringify({ choices: [{ delta: { content } }] })}\n\n`).join("") + "data: [DONE]\n\n";
    return new Response(lines);
  });
  vi.stubGlobal("fetch", fetcher);
  return asked;
}

afterEach(() => vi.unstubAllGlobals());

describe("cases a model writes", () => {
  it("are only ever asked of a model on this computer", async () => {
    const asked = model("");
    for (const localUrl of ["https://api.example.com/v1", "http://192.168.1.20:8000/v1", "", "not a url"]) {
      expect(() => localOnly({ ...local, localUrl })).toThrow("only a model on this computer");
      await expect(casesFor(entry, { ...local, localUrl })).rejects.toThrow("only a model on this computer");
    }
    expect(asked).toEqual([]);
    expect(() => localOnly(local)).not.toThrow();
    expect(() => localOnly({ ...local, localUrl: "http://localhost:11434/v1" })).not.toThrow();
  });

  it("come from the local server, whichever provider the settings name", async () => {
    const asked = model(
      [
        '{"type": "keyword", "query": "ssh deploy"}',
        '{"type": "description", "query": "server login command"}',
        '{"type": "cross-language", "query": "服务器登录命令"}',
        '{"type": "exact", "query": "10.0.3.17"}',
        '{"type": "hinted", "query": "ssh from terminal"}',
      ].join("\n"),
    );
    const cases = await casesFor(entry, { ...local, provider: "claude", apiKey: "sk-secret" });
    expect(cases.map((c) => c.type)).toEqual(["keyword", "description", "cross-language", "exact", "hinted"]);
    expect(asked).toHaveLength(1);
    expect(asked[0].url).toBe("http://127.0.0.1:8000/v1/chat/completions");
    expect(asked[0].body.model).toBe("tiny");
    // The entry is given with when and where it was copied, and the key goes nowhere.
    const sent = JSON.stringify(asked[0].body);
    expect(sent).toContain("ssh deploy@10.0.3.17 -p 2222");
    expect(sent).toContain("Copied 3 days ago in org.gnome.Terminal");
    expect(sent).not.toContain("sk-secret");
  });

  it("gives a long entry to the model only as far as it needs", () => {
    expect(prompt({ id: 1, text: "x".repeat(5000), copied: "today" }).length).toBeLessThan(1600);
    expect(prompt({ id: 1, text: "note", copied: "today" })).toBe("(Copied today.)\nnote");
  });
});

describe("what a model wrote", () => {
  const text = "The quarterly report is due on Friday; please send me your numbers by Wednesday noon.";

  it("is read line by line, whatever is around it", () => {
    const answer = [
      "Sure! Here are the searches:",
      '```json',
      '{"type": "keyword", "query": "quarterly report"}',
      '- {"type": "paraphrase", "query": "three-month summary deadline"}',
      '{"type": "cross-language", "query": "“季度报告截止”"}',
      '{"type": "keyword", "query": "report"}',
      '{"type": "poem", "query": "o report"}',
      '{"type": "partial", "query": 5}',
      "{not json}",
      "```",
    ].join("\n");
    expect(parse(answer, text)).toEqual([
      { type: "keyword", query: "quarterly report" },
      { type: "paraphrase", query: "three-month summary deadline" },
      { type: "cross-language", query: "季度报告截止" },
    ]);
  });

  it("is checked against the entry", () => {
    // A paraphrase that repeats the entry's words measures the word search again.
    expect(acceptable({ type: "paraphrase", query: "quarterly numbers due" }, text)).toBe(false);
    expect(acceptable({ type: "paraphrase", query: "three-month summary deadline" }, text)).toBe(true);
    expect(acceptable({ type: "description", query: "那个发票的事" }, "上个月的发票已经寄出，请查收")).toBe(false);
    expect(acceptable({ type: "description", query: "账单邮寄了" }, "上个月的发票已经寄出，请查收")).toBe(true);
    // A fragment that is not in the entry finds nothing by design.
    expect(acceptable({ type: "exact", query: "10.0.3.17" }, entry.text)).toBe(true);
    expect(acceptable({ type: "exact", query: "10.0.3.71" }, entry.text)).toBe(false);
    expect(acceptable({ type: "keyword", query: "SSH deploy" }, entry.text)).toBe(true);
    expect(acceptable({ type: "keyword", query: "ssh server" }, entry.text)).toBe(false);
    // Nobody types a sentence into a search field.
    expect(acceptable({ type: "partial", query: "please send me all of your numbers by Wednesday at noon" }, text)).toBe(false);
    expect(acceptable({ type: "hinted", query: "" }, text)).toBe(false);
  });
});
