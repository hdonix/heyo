import { existsSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import path from "node:path";

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { InMemoryTransport } from "@modelcontextprotocol/sdk/inMemory.js";
import { StreamableHTTPServerTransport } from "@modelcontextprotocol/sdk/server/streamableHttp.js";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import type { Account } from "../../heyo/connector";
import { loader } from "../load";
import { serve, type FixtureResource } from "./fixtures/mcp-server.mjs";
import { mcp, syncResources, variables } from "./mcp";
import { recorder } from "./testing";

const FIXTURE = path.join(import.meta.dirname, "fixtures", "mcp-server.mjs");

const alpha: FixtureResource = { uri: "note://alpha", name: "alpha", title: "Alpha", mimeType: "text/plain", text: "The first note" };
const page: FixtureResource = { uri: "https://example.com/page", name: "page", mimeType: "text/html", text: "<h1>Page</h1><p>With <b>words</b></p>" };

/// A client of a server in this process.
async function connected(resources: FixtureResource[]) {
  const fixture = serve(resources);
  const [ours, theirs] = InMemoryTransport.createLinkedPair();
  await fixture.server.connect(theirs);
  const client = new Client({ name: "test", version: "1" });
  await client.connect(ours);
  return { client, set: fixture.set };
}

/// Whether the process of `pid` is running.
const alive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

describe("MCP resources", () => {
  it("brings each resource as a document, lists the tools, and then only what changed", async () => {
    const { client, set } = await connected([alpha, page]);
    const first = recorder();
    const cursor = await syncResources(client, undefined, first.io, new Date("2026-11-03T12:00:00Z"));
    expect(first.tools).toEqual([[{ name: "echo", title: "Echo", description: "Says it back" }]]);
    expect(first.put).toEqual([
      { id: "note://alpha", kind: "document", title: "Alpha", time: "2026-11-03T12:00:00.000Z", text: "The first note" },
      { id: "https://example.com/page", kind: "document", title: "page", time: "2026-11-03T12:00:00.000Z", text: "PAGE\n\nWith words", url: "https://example.com/page" },
    ]);

    const second = recorder();
    const same = await syncResources(client, cursor, second.io, new Date("2026-11-04T12:00:00Z"));
    expect([second.put, second.removed]).toEqual([[], []]);
    expect(same).toEqual(cursor);

    set([{ ...alpha, text: "The first note, edited" }]);
    const third = recorder();
    await syncResources(client, same, third.io, new Date("2026-11-05T12:00:00Z"));
    // Still the time it was first seen.
    expect(third.put).toEqual([{ ...first.put[0], text: "The first note, edited" }]);
    expect(third.removed).toEqual(["https://example.com/page"]);
    await client.close();
  });

  it("reads variables as NAME=value", () => {
    expect(variables(["A=1", "B=x=y", "bad", "=no", ""])).toEqual({ A: "1", B: "x=y" });
  });

  it("is what an account of @heyo:mcp is synced by", () => {
    expect(loader()({ extension: "@heyo", command: "mcp" })).toBe(mcp);
  });
});

describe("an MCP server run as a command", () => {
  let dir: string;
  const source = () => path.join(dir, "resources.json");
  const says = (resources: FixtureResource[]) => {
    writeFileSync(`${source()}.tmp`, JSON.stringify({ resources }));
    renameSync(`${source()}.tmp`, source());
  };
  const account = (more: Record<string, unknown> = {}): Account => ({
    id: "mcp",
    name: "MCP",
    config: { transport: "command", command: process.execPath, args: [FIXTURE, source()], ...more },
    secret: async () => undefined,
  });
  const pid = () => Number(readFileSync(`${source()}.pid`, "utf8"));

  beforeAll(() => void (dir = mkdtempSync(path.join(tmpdir(), "heyo-mcp-"))));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  afterEach(() => rmSync(`${source()}.pid`, { force: true }));

  it("is started for a sync and ended after it", async () => {
    says([alpha]);
    await expect(mcp.check(account())).resolves.toBeUndefined();
    const run = recorder();
    await mcp.sync(account(), undefined, run.io);
    expect(run.put.map((item) => item.title)).toEqual(["Alpha"]);
    expect(run.tools[0].map((tool) => tool.name)).toEqual(["echo"]);
    await vi.waitFor(() => expect(alive(pid())).toBe(false));
  });

  it("is watched for changes, and ended when the watch is stopped", async () => {
    says([alpha]);
    const controller = new AbortController();
    const changed = vi.fn();
    const watching = mcp.watch!(account(), { changed, signal: controller.signal });
    await vi.waitFor(() => expect(existsSync(`${source()}.pid`)).toBe(true));
    const started = pid();
    // Let it connect before the change.
    await new Promise((done) => setTimeout(done, 300));
    says([alpha, page]);
    await vi.waitFor(() => expect(changed).toHaveBeenCalled());
    controller.abort();
    await watching;
    await vi.waitFor(() => expect(alive(started)).toBe(false));
  });

  it("says what is wrong with how it is set up", async () => {
    await expect(mcp.check(account({ command: "" }))).rejects.toMatchObject({ kind: "failed", message: "A server on this computer needs a command." });
    await expect(mcp.check(account({ command: path.join(dir, "missing") }))).rejects.toMatchObject({ kind: "failed" });
  });
});

describe("an MCP server at an address", () => {
  let server: http.Server;
  let url: string;

  beforeAll(async () => {
    server = http.createServer(async (request, response) => {
      if (request.headers.authorization !== "Bearer right") return void response.writeHead(401).end();
      // Without sessions: a server of its own for each request.
      const { server: mcpServer } = serve([alpha]);
      const transport = new StreamableHTTPServerTransport({ sessionIdGenerator: undefined, enableJsonResponse: true });
      response.on("close", () => void transport.close());
      await mcpServer.connect(transport);
      await transport.handleRequest(request, response);
    });
    await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
    url = `http://127.0.0.1:${(server.address() as AddressInfo).port}/mcp`;
  });
  afterAll(() => void server.close());

  const account = (token: string): Account => ({ id: "mcp", name: "MCP", config: { transport: "http", url }, secret: async (name) => (name === "token" ? token : undefined) });

  it("is synced with its token, and refuses without the right one", async () => {
    const run = recorder();
    await mcp.sync(account("right"), undefined, run.io);
    expect(run.put.map((item) => item.title)).toEqual(["Alpha"]);
    await expect(mcp.check(account("wrong"))).rejects.toMatchObject({ kind: "auth" });
    await expect(mcp.check({ ...account("right"), config: { transport: "http", url: "http://127.0.0.1:1/mcp" } })).rejects.toMatchObject({ kind: "unavailable" });
  });
});
