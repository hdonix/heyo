import { describe, expect, it } from "vitest";

import { needs } from "../../shared/bindings/needs";
import { createHost, EFFECTS } from "./host.mjs";

describe("the stand-in host", () => {
  it("answers what the runtime asks about the desktop with small made-up data", () => {
    const host = createHost({ home: "/scratch/home" });
    const apps = host.answer("host.applications").result;
    expect(apps.length).toBeGreaterThan(2);
    for (const app of apps) expect(app).toMatchObject({ name: expect.any(String), path: expect.stringMatching(/\.desktop$/), bundleId: expect.any(String) });
    expect(host.answer("ext.frontmostApplication").result.name).toBe("Firefox");
    expect(host.answer("host.selection").result).toBe("selected text");
    expect(host.answer("ext.windows.list").result[0]).toMatchObject({ id: expect.any(Number), title: expect.any(String), class: "firefox", active: true, bounds: { width: 1280 } });
    expect(host.answer("ext.windows.monitors").result).toEqual([{ x: 0, y: 0, width: 1920, height: 1080 }]);
    expect(host.answer("ext.files.search", { query: "notes" }).result.files[1]).toEqual({ path: "/scratch/home/Documents/notes.txt", isDirectory: false });
    expect(host.answer("host.defaultApplication", { target: "https://example.com" }).result).toBe("firefox.desktop");
  });

  it("has no AI configured", () => {
    expect(createHost().answer("host.ai").result).toEqual({ provider: "claude", apiKey: "", localUrl: "", localModel: "" });
  });

  it("writes down what a command asks to have done, and says it was done", () => {
    const host = createHost();
    expect(host.answer("host.open", { target: "https://example.com/a" })).toEqual({ result: null });
    expect(host.answer("host.trash", { paths: ["/home/user/x"] })).toEqual({ result: null });
    expect(host.answer("host.paste")).toEqual({ result: null });
    expect(host.answer("host.processes.end", { pid: 1, force: true })).toEqual({ result: null });
    expect(host.effects.map((e) => e.method)).toEqual(["host.open", "host.trash", "host.paste", "host.processes.end"]);
    expect(host.effects[0].params).toContain("https://example.com/a");
    expect(host.used).toMatchObject({ "host.open": 1, "host.paste": 1 });
  });

  it("answers every request that does something without doing it", () => {
    const host = createHost();
    // Done, as far as the command is told: null, or what the host answers with (an activity's id).
    for (const method of EFFECTS) expect(host.answer(method, {}), method).toEqual({ result: method === "ext.activity.start" ? { id: "" } : null });
    expect(host.unknown).toEqual([]);
    expect(host.effects.map((e) => e.method)).toEqual([...EFFECTS]);
  });

  it("answers everything an extension can ask through @heyo/api, by the host's own list of it", () => {
    const host = createHost();
    const refused: string[] = [];
    for (const method of Object.keys(needs)) {
      const reply = host.answer(method, {});
      if (!("error" in reply)) expect(reply, method).toHaveProperty("result");
      // Refused as the host refuses what it cannot do: with a code.
      else expect(refused.push(method) && reply.error, method).toEqual({ code: expect.stringMatching(/^un(supported|available)$/), message: expect.any(String) });
    }
    expect(host.unknown).toEqual([]);
    // There is no screen, and neither engine.
    expect(refused).toEqual(["ext.dictation.listen", "ext.screen.capture", "ext.screen.recognizeText"]);
    expect(host.answer("ext.supported", { module: "screen" }).result).toBe("none");
    expect(host.answer("ext.supported", { module: "windows" }).result).toBe("full");
    expect(host.answer("ext.activity.start", { attributes: { id: "build" } }).result).toEqual({ id: "build" });
    expect(host.answer("ext.activity.onEvent").result).not.toEqual(host.answer("ext.activity.onEvent").result);
    expect(host.effects.map((e) => e.method)).toContain("ext.activity.start");
  });

  it("gives back from the clipboard what was copied to it", () => {
    const host = createHost();
    expect(host.answer("host.clipboard.read").result).toBe("Heyo compatibility test");
    host.answer("host.clipboard.write", { text: "copied" });
    expect(host.answer("host.clipboard.read").result).toBe("copied");
  });

  it("declines an alert", () => {
    expect(createHost().answer("ui.confirm", { title: "Delete everything?" }).result).toBe(false);
  });

  it("refuses a request it does not know, as the host does", () => {
    const host = createHost();
    expect(host.answer("host.format-disk").error).toMatch(/unknown request/);
    expect(host.unknown).toEqual(["host.format-disk"]);
  });

  it("keeps long parameters short", () => {
    const host = createHost();
    host.answer("host.clipboard.write", { text: "x".repeat(5000) });
    expect(host.effects[0].params.length).toBeLessThan(220);
  });
});
