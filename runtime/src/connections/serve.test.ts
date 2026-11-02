import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ConnectorAccount } from "../../../shared/bindings/ConnectorAccount";
import type { ConnectionItem } from "../../../shared/bindings/ConnectionItem";
import { ConnectorError, type Connector } from "../heyo/connector";
import { speak, t } from "../../../shared/i18n";
import { pair } from "../transport";
import { loader } from "./load";
import { limits, serve } from "./serve";

const item = (id: number): ConnectionItem => ({ id: String(id), kind: "email", title: `Mail ${id}`, time: "2026-11-03T09:00:00Z" });

const account: ConnectorAccount = { id: "work", name: "Work", connector: { extension: "@heyo", command: "test" }, config: { host: "h" } };

/// The runtime with one connector, and the test in the host's place: it
/// answers the runtime's requests as `answers` says, and keeps them.
function bench(connector: Connector, answers: Record<string, (params: any) => unknown> = {}) {
  const [host, runtime] = pair();
  const asked: { method: string; params: any }[] = [];
  const replies = new Map<number, { result?: any; error?: any }>();
  let next = 0;
  host.receive(
    (line) => {
      const message = JSON.parse(line);
      if (message.type === "reply") return void replies.set(message.id, message);
      asked.push({ method: message.method, params: message.params });
      const answer = answers[message.method];
      if (answer === undefined) return void host.send(JSON.stringify({ type: "reply", id: message.id, result: null }));
      Promise.resolve()
        .then(() => answer(message.params))
        .then(
          (result) => result !== "never" && host.send(JSON.stringify({ type: "reply", id: message.id, result: result ?? null })),
          (error) => host.send(JSON.stringify({ type: "reply", id: message.id, error: String(error) })),
        );
    },
    () => {},
  );
  const over = serve(runtime, loader({ test: connector }));
  return {
    asked,
    over,
    host,
    /// Asks the runtime; resolves with its reply.
    async ask(method: string, params: unknown) {
      const id = ++next;
      host.send(JSON.stringify({ type: "request", id, method, params }));
      await vi.waitFor(() => expect(replies.has(id)).toBe(true));
      return replies.get(id)!;
    },
    send(method: string, params: unknown) {
      const id = ++next;
      host.send(JSON.stringify({ type: "request", id, method, params }));
      return id;
    },
    replies,
  };
}

afterEach(() => {
  vi.useRealTimers();
  limits.request = 60_000;
});

describe("the connections runtime", () => {
  it("puts what a sync brings in batches of 200, and answers the cursor once they are written", async () => {
    let written = 0;
    const connector: Connector = {
      check: async () => {},
      async sync(acct, cursor, io) {
        expect([acct.id, cursor]).toEqual(["work", undefined]);
        io.progress(0.5, "Half");
        // Not waited for: the cursor still goes back only after it.
        void io.put(Array.from({ length: 450 }, (_, n) => item(n)));
        await io.remove(["a", "b"]);
        return "c1";
      },
    };
    const run = bench(connector, {
      "host.items.put": async (params) => {
        await new Promise((resolve) => setTimeout(resolve, 5));
        written += params.items.length;
      },
    });
    const reply = await run.ask("connector.sync", { account });
    expect(reply.result).toEqual({ cursor: "c1" });
    expect(written).toBe(450);
    const puts = run.asked.filter((asked) => asked.method === "host.items.put").map((asked) => asked.params.items.length);
    expect(puts).toEqual([200, 200, 50]);
    // In the order they were given: the puts, then the removal.
    expect(run.asked.map((asked) => asked.method).filter((method) => method.startsWith("host.items"))).toEqual(["host.items.put", "host.items.put", "host.items.put", "host.items.remove"]);
    expect(run.asked[0]).toEqual({ method: "host.connector.progress", params: { account: "work", fraction: 0.5, text: "Half" } });
  });

  it("tells the host of the tools a service offers, and speaks the language it is told", async () => {
    const connector: Connector = {
      check: async () => {
        throw new ConnectorError("failed", t("The password is not set."));
      },
      async sync(_, __, io) {
        io.tools([{ name: "echo", description: "Says it back" }]);
        return "c";
      },
    };
    const run = bench(connector);
    await run.ask("connector.sync", { account });
    expect(run.asked).toContainEqual({ method: "host.connector.tools", params: { account: "work", tools: [{ name: "echo", description: "Says it back" }] } });

    const translations = { language: "zh_CN", plural: [], messages: { "The password is not set.": ["没有设置密码。"] } };
    expect(await run.ask("connector.language", { translations })).toMatchObject({ result: null });
    expect((await run.ask("connector.check", { account })).error).toEqual({ kind: "failed", message: "没有设置密码。" });
    speak({ language: "en", plural: [], messages: {} });
  });

  it("hands a connector its account, whose secrets it asks for", async () => {
    const connector: Connector = {
      async check(acct) {
        expect([acct.name, acct.config]).toEqual(["Work", { host: "h" }]);
        if ((await acct.secret("password")) !== "pw") throw new ConnectorError("auth", "No.");
        if ((await acct.secret("token")) !== undefined) throw new Error("a token");
      },
      sync: async () => "",
    };
    const run = bench(connector, { "host.connector.secret": (params) => (params.name === "password" ? "pw" : null) });
    expect(await run.ask("connector.check", { account })).toEqual({ type: "reply", id: 1, result: null });
    expect(run.asked).toEqual([
      { method: "host.connector.secret", params: { account: "work", name: "password" } },
      { method: "host.connector.secret", params: { account: "work", name: "token" } },
    ]);
  });

  it("says what went wrong by its kind", async () => {
    const failing = (error: unknown): Connector => ({ check: async () => Promise.reject(error), sync: async () => Promise.reject(error) });
    expect((await bench(failing(new ConnectorError("auth", "Refused."))).ask("connector.check", { account })).error).toEqual({ kind: "auth", message: "Refused." });
    expect((await bench(failing(new ConnectorError("unavailable"))).ask("connector.sync", { account })).error.kind).toBe("unavailable");
    expect((await bench(failing(new TypeError("x is undefined"))).ask("connector.sync", { account })).error).toEqual({ kind: "failed", message: "x is undefined" });
    // A copy of the class from `@heyo/api` outside Heyo is read the same.
    const copy = Object.assign(new Error("Gone."), { name: "ConnectorError", kind: "unavailable" });
    expect((await bench(failing(copy)).ask("connector.check", { account })).error).toEqual({ kind: "unavailable", message: "Gone." });
    // Neither is a sync that brings no cursor, nor a connector that is none.
    const odd = { check: async () => {}, sync: async () => 7 } as unknown as Connector;
    expect((await bench(odd).ask("connector.sync", { account })).error.kind).toBe("failed");
    const elsewhere = { ...account, connector: { extension: "@heyo", command: "none" } };
    expect((await bench(odd).ask("connector.check", { account: elsewhere })).error).toEqual({ kind: "failed", message: "Heyo has no connector none." });
  });

  it("stops a sync when asked, answers it cancelled at once, and hears nothing of it after", async () => {
    let signal: AbortSignal | undefined;
    const connector: Connector = {
      check: async () => {},
      async sync(_acct, _cursor, io) {
        signal = io.signal;
        await new Promise((resolve) => io.signal.addEventListener("abort", resolve));
        // What it tries after being stopped goes nowhere.
        await io.put([item(1)]);
        return "late";
      },
    };
    const run = bench(connector);
    const id = run.send("connector.sync", { account });
    await vi.waitFor(() => expect(signal).toBeDefined());
    expect(await run.ask("connector.stop", { request: id })).toMatchObject({ result: null });
    expect(run.replies.get(id)).toEqual({ type: "reply", id, error: { kind: "cancelled", message: "Heyo stopped it." } });
    expect(signal!.aborted).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(run.asked.filter((asked) => asked.method === "host.items.put")).toEqual([]);
  });

  it("watches until stopped, telling each change", async () => {
    const connector: Connector = {
      check: async () => {},
      sync: async () => "",
      async watch(_acct, io) {
        io.changed();
        io.changed();
        await new Promise((resolve) => io.signal.addEventListener("abort", resolve));
        io.changed();
      },
    };
    const run = bench(connector);
    const id = run.send("connector.watch", { account });
    await vi.waitFor(() => expect(run.asked.length).toBe(2));
    expect(run.asked[0]).toEqual({ method: "host.connector.changed", params: { account: "work" } });
    await run.ask("connector.stop", { request: id });
    expect(run.replies.get(id)?.error?.kind).toBe("cancelled");
    expect(run.asked.length).toBe(2);
    // One without a watch answers at once.
    expect((await bench({ check: async () => {}, sync: async () => "" }).ask("connector.watch", { account })).result).toBe(null);
  });

  it("fails a request the host does not answer in time", async () => {
    limits.request = 30;
    const connector: Connector = { check: async () => {}, sync: async (_a, _c, io) => (await io.put([item(1)]), "c") };
    const run = bench(connector, { "host.items.put": () => "never" });
    const reply = await run.ask("connector.sync", { account });
    expect(reply.error).toEqual({ kind: "failed", message: "Heyo did not answer host.items.put in time." });
  });

  it("loads an extension's connector from its built file, with @heyo/api", async () => {
    const fixture = path.join(__dirname, "fixtures", "fake");
    const connector = loader()({ extension: fixture, command: "fake" });
    expect(typeof connector.sync).toBe("function");
    // Once.
    expect(loader()({ extension: fixture, command: "fake" })).not.toBe(undefined);
    expect(() => loader()({ extension: fixture, command: "missing" })).toThrow();
  });

  it("ends what runs when the host hangs up", async () => {
    let signal: AbortSignal | undefined;
    const run = bench({ check: async () => {}, sync: async (_a, _c, io) => ((signal = io.signal), new Promise<string>(() => {})) });
    run.send("connector.sync", { account });
    await vi.waitFor(() => expect(signal).toBeDefined());
    (run.host as { close(): void }).close();
    await run.over;
    expect(signal!.aborted).toBe(true);
  });
});
