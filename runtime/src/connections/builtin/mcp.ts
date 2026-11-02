//! MCP servers (`@heyo:mcp`), through the official SDK: one run here as a
//! command, spoken to over its input and output, or one elsewhere at a
//! Streamable HTTP address.
//!
//! - Each resource the server lists is a document item, its text what
//!   `resources/read` gives (HTML made text); one whose text is the same
//!   as the last sync's is not put again. What is no longer listed is
//!   removed.
//! - The server's tools are listed for the settings (`io.tools`), never
//!   called.
//! - The watch stays connected and says something changed when the server
//!   says its resources did (`list_changed`, or `updated` for those it
//!   subscribed to).
//! - A server run as a command lives only as long as the sync or the watch
//!   that started it: it is ended when that ends, when the account is
//!   removed (which stops them), and when this process exits.

import { createHash } from "node:crypto";

import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import type { Transport } from "@modelcontextprotocol/sdk/shared/transport.js";
import { ResourceListChangedNotificationSchema, ResourceUpdatedNotificationSchema, type Resource } from "@modelcontextprotocol/sdk/types.js";

import { t } from "../../../../shared/i18n";
import { ConnectorError, type Account, type Connector, type Item, type SyncIo, type Tool } from "../../heyo/connector";
import { cursorOf, failure, keepWatching, list, plain, stopped, text } from "./common";

/// The most resources brought, and subscribed to.
const MOST_RESOURCES = 2000;
const MOST_SUBSCRIBED = 200;

// --- Connecting ----------------------------------------------------------------

/// The servers this process started, by their process ids: ended with it.
const children = new Set<number>();
process.once("exit", () => {
  for (const pid of children) {
    try {
      process.kill(pid, "SIGTERM");
    } catch {
      // Gone already.
    }
  }
});

/// `NAME=value` pairs, from a list or from text apart by spaces.
export function variables(pairs: string[]): Record<string, string> {
  const found: Record<string, string> = {};
  for (const pair of pairs) {
    const at = pair.indexOf("=");
    if (at > 0) found[pair.slice(0, at).trim()] = pair.slice(at + 1);
  }
  return found;
}

/// How the account's server is reached.
export async function transportOf(account: Account): Promise<Transport> {
  if ((text(account, "transport") || "command") === "http") {
    const address = text(account, "url");
    if (!address) throw new ConnectorError("failed", t("A server elsewhere needs an address."));
    let url: URL;
    try {
      url = new URL(address);
    } catch {
      throw new ConnectorError("failed", t("{0} is not an address.", address));
    }
    const token = await account.secret("token");
    return new StreamableHTTPClientTransport(url, token ? { requestInit: { headers: { Authorization: `Bearer ${token}` } } } : undefined);
  }
  const command = text(account, "command");
  if (!command) throw new ConnectorError("failed", t("A server on this computer needs a command."));
  const secrets = (await account.secret("secretEnv")) ?? "";
  const env = { ...variables(list(account, "env")), ...variables(secrets.split(/\s+/)) };
  const transport = new StdioClientTransport({ command, args: list(account, "args"), env, stderr: "ignore" });
  // Known by its pid once it is started, and forgotten once it ended.
  const start = transport.start.bind(transport);
  transport.start = async () => {
    await start();
    const pid = transport.pid;
    if (pid) {
      children.add(pid);
      const onclose = transport.onclose;
      transport.onclose = () => {
        children.delete(pid);
        onclose?.();
      };
    }
  };
  return transport;
}

/// A client connected to the account's server, closed (and the server
/// ended) when `signal` aborts.
export async function connect(account: Account, signal?: AbortSignal, open: (account: Account) => Promise<Transport> = transportOf): Promise<Client> {
  const client = new Client({ name: "Heyo", version: "1" }, { capabilities: {} });
  const transport = await open(account);
  try {
    await client.connect(transport, { signal });
  } catch (error) {
    await client.close().catch(() => {});
    throw failure(error);
  }
  signal?.addEventListener("abort", () => void client.close(), { once: true });
  return client;
}

// --- Syncing -------------------------------------------------------------------

export interface Cursor {
  /// Each resource brought, by its URI: a hash of its item, and the time
  /// it was first seen (a resource that says no time of its own keeps it).
  resources: Record<string, { hash: string; time: string }>;
}

async function resources(client: Client, signal: AbortSignal): Promise<Resource[]> {
  if (!client.getServerCapabilities()?.resources) return [];
  const all: Resource[] = [];
  let cursor: string | undefined;
  do {
    const page = await client.listResources(cursor ? { cursor } : undefined, { signal });
    all.push(...page.resources);
    cursor = page.nextCursor;
  } while (cursor && all.length < MOST_RESOURCES);
  return all.slice(0, MOST_RESOURCES);
}

export async function tools(client: Client, signal: AbortSignal): Promise<Tool[]> {
  if (!client.getServerCapabilities()?.tools) return [];
  const all: Tool[] = [];
  let cursor: string | undefined;
  do {
    const page = await client.listTools(cursor ? { cursor } : undefined, { signal });
    for (const tool of page.tools) all.push({ name: tool.name, ...(tool.title ? { title: tool.title } : {}), ...(tool.description ? { description: tool.description } : {}) });
    cursor = page.nextCursor;
  } while (cursor);
  return all;
}

/// The text of a resource: what `resources/read` gives as text, HTML made
/// text; nothing of what is binary.
async function textOf(client: Client, resource: Resource, signal: AbortSignal): Promise<string | undefined> {
  const read = await client.readResource({ uri: resource.uri }, { signal });
  const parts = read.contents.flatMap((content) => ("text" in content && typeof content.text === "string" ? [{ text: content.text, html: /html/i.test(content.mimeType ?? resource.mimeType ?? "") }] : []));
  return plain(parts.map((part) => (part.html ? plain(part.text, true) : part.text)).join("\n\n"));
}

export async function syncResources(client: Client, cursor: Cursor | undefined, io: SyncIo, now = new Date()): Promise<Cursor> {
  io.tools(await tools(client, io.signal));
  const listed = await resources(client, io.signal);
  const next: Cursor = { resources: {} };
  const items: Item[] = [];
  for (const [index, resource] of listed.entries()) {
    stopped(io.signal);
    if (index % 20 === 0) io.progress(index / Math.max(1, listed.length), t("Reading the resources"));
    let body: string | undefined;
    try {
      body = await textOf(client, resource, io.signal);
    } catch (error) {
      // One resource that cannot be read does not stop the others.
      stopped(io.signal);
      console.error(`heyo-connections: ${resource.uri}: ${error}`);
    }
    const before = cursor?.resources[resource.uri];
    const modified = resource.annotations?.lastModified;
    const time = modified && !Number.isNaN(Date.parse(modified)) ? new Date(modified).toISOString() : (before?.time ?? now.toISOString());
    const text = body ?? plain(resource.description);
    const url = /^https?:\/\//i.test(resource.uri) ? resource.uri : undefined;
    const item: Item = { id: resource.uri, kind: "document", title: resource.title || resource.name || resource.uri, time, ...(text ? { text } : {}), ...(url ? { url } : {}) };
    const hash = createHash("sha256").update(JSON.stringify(item)).digest("base64url");
    next.resources[resource.uri] = { hash, time };
    if (before?.hash !== hash) items.push(item);
  }
  await io.put(items);
  const went = Object.keys(cursor?.resources ?? {}).filter((uri) => !next.resources[uri]);
  if (went.length) await io.remove(went);
  io.progress(1);
  return next;
}

// --- The connector -------------------------------------------------------------

/// Runs `work` with a client of the account's server, closed after.
async function using<T>(account: Account, signal: AbortSignal | undefined, work: (client: Client) => Promise<T>): Promise<T> {
  const client = await connect(account, signal);
  try {
    return await work(client);
  } catch (error) {
    if (signal) stopped(signal);
    throw failure(error);
  } finally {
    await client.close().catch(() => {});
  }
}

/// Watches the server's resources until `signal` aborts.
export async function watchResources(account: Account, io: Parameters<NonNullable<Connector["watch"]>>[1], open?: (account: Account) => Promise<Transport>) {
  await keepWatching(io, async (connected) => {
    const client = await connect(account, io.signal, open);
    try {
      const ended = new Promise<void>((over) => (client.onclose = over));
      const capabilities = client.getServerCapabilities()?.resources;
      client.setNotificationHandler(ResourceListChangedNotificationSchema, () => io.changed());
      client.setNotificationHandler(ResourceUpdatedNotificationSchema, () => io.changed());
      if (capabilities?.subscribe) {
        for (const resource of (await resources(client, io.signal)).slice(0, MOST_SUBSCRIBED)) {
          await client.subscribeResource({ uri: resource.uri }, { signal: io.signal }).catch(() => {});
        }
      }
      connected();
      if (!io.signal.aborted) await ended;
    } finally {
      await client.close().catch(() => {});
    }
  });
}

export const mcp: Connector = {
  async check(account) {
    await using(account, undefined, async (client) => {
      if (client.getServerCapabilities()?.resources) await client.listResources();
    });
  },
  async sync(account, cursor, io) {
    return JSON.stringify(await using(account, io.signal, (client) => syncResources(client, cursorOf<Cursor>(cursor), io)));
  },
  async watch(account, io) {
    await watchResources(account, io);
  },
};
