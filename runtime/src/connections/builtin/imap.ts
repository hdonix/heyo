//! Mail over IMAP (`@heyo:imap`): imapflow speaks to the server and
//! mailparser decodes what it sends (any charset, GBK among them).
//!
//! - Each folder is synced from its UIDVALIDITY and the highest UID seen:
//!   only what is newer is fetched. A folder whose UIDVALIDITY changed is
//!   fetched again whole, its old items removed. The first sync brings the
//!   last `days` days only.
//! - A mail is its headers and its first text/plain part (or its HTML made
//!   text), at most 8000 characters; of its attachments, names and sizes.
//! - What was deleted or moved away is found by asking the server which of
//!   the UIDs synced are still there (`UID SEARCH UID …`), and removed.
//! - The watch keeps INBOX open in IDLE and says something changed when
//!   mail comes or goes; it connects again when the connection drops.
//!
//! The syncing is written against `Session`, what is asked of a server, so
//! that tests give it a fake one; `connect` is the real one.

import { ImapFlow, type FetchMessageObject, type MessageStructureObject } from "imapflow";
import { simpleParser } from "mailparser";

import type { ItemPerson } from "../../../../shared/bindings/ItemPerson";
import { t } from "../../../../shared/i18n";
import type { Account, Connector, Item, SyncIo, WatchIo } from "../../heyo/connector";
import { cursorOf, failure, keepWatching, list, number, plain, required, secret, stopped, text } from "./common";

// --- What is asked of a server -------------------------------------------------

/// A mail as the server describes it, before it is an item.
export interface Mail {
  uid: number;
  subject?: string;
  date?: Date;
  from?: { name?: string; address?: string }[];
  to?: { name?: string; address?: string }[];
  cc?: { name?: string; address?: string }[];
  flags: string[];
  text?: string;
  attachments: { name: string; size?: number; type?: string }[];
}

/// A folder, opened.
export interface Opened {
  /// Its UIDVALIDITY, as text (it is a 32-bit number, kept exactly).
  validity: string;
  /// The UID the next mail will have.
  next: number;
}

/// An IMAP connection, signed in.
export interface Session {
  open(folder: string): Promise<Opened>;
  /// The UIDs of the open folder's mails that arrived on or after `date`.
  since(date: Date): Promise<number[]>;
  /// The UIDs of the open folder's mails above `uid`.
  after(uid: number): Promise<number[]>;
  /// Which of `uids` the open folder still has.
  present(uids: number[]): Promise<number[]>;
  /// The mails of the open folder of these UIDs.
  fetch(uids: number[]): Promise<Mail[]>;
  close(): Promise<void>;
}

// --- Syncing -------------------------------------------------------------------

/// Where a folder's sync goes on from.
interface Folder {
  validity: string;
  /// The highest UID seen.
  top: number;
  /// The UIDs synced, as an IMAP set (`1:40,42`).
  have: string;
}

export interface Cursor {
  folders: Record<string, Folder>;
}

/// Mails fetched at once.
const CHUNK = 50;

/// `uids` as an IMAP set: runs written `a:b`.
export function setOf(uids: number[]): string {
  const sorted = [...new Set(uids)].sort((a, b) => a - b);
  const runs: string[] = [];
  for (let i = 0; i < sorted.length; ) {
    let j = i;
    while (j + 1 < sorted.length && sorted[j + 1] === sorted[j] + 1) j++;
    runs.push(i === j ? `${sorted[i]}` : `${sorted[i]}:${sorted[j]}`);
    i = j + 1;
  }
  return runs.join(",");
}

/// The UIDs an IMAP set of numbers names.
export function uidsOf(set: string): number[] {
  if (!set) return [];
  return set.split(",").flatMap((run) => {
    const [from, to] = run.split(":").map(Number);
    if (to === undefined) return [from];
    const all: number[] = [];
    for (let uid = from; uid <= to; uid++) all.push(uid);
    return all;
  });
}

/// The highest of `uids`, 0 for none (without spreading a long list into arguments).
const highest = (uids: number[]) => uids.reduce((top, uid) => (uid > top ? uid : top), 0);

const idOf = (folder: string, validity: string, uid: number) => `${folder}/${validity}/${uid}`;

function people(mail: Mail): ItemPerson[] {
  const of = (role: ItemPerson["role"], all: Mail["from"] = []) =>
    all.filter((who) => who.address || who.name).map((who) => ({ ...(who.name ? { name: who.name } : {}), ...(who.address ? { address: who.address } : {}), role }));
  return [...of("from", mail.from), ...of("to", mail.to), ...of("cc", mail.cc)];
}

export function itemOf(folder: string, validity: string, mail: Mail): Item {
  const flags = [...(mail.flags.includes("\\Seen") ? [] : ["unread"]), ...(mail.flags.includes("\\Flagged") ? ["flagged"] : [])];
  return {
    id: idOf(folder, validity, mail.uid),
    kind: "email",
    title: mail.subject?.trim() || t("(no subject)"),
    time: (mail.date && !Number.isNaN(mail.date.getTime()) ? mail.date : new Date(0)).toISOString(),
    people: people(mail),
    ...(mail.text ? { text: mail.text } : {}),
    ...(mail.attachments.length ? { attachments: mail.attachments } : {}),
    folder,
    ...(flags.length ? { flags } : {}),
  };
}

/// What a sync is told to do.
export interface Asked {
  folders: string[];
  /// How far back the first sync of a folder goes.
  days: number;
  now: Date;
}

/// Brings what changed in each folder since `cursor`, and says where to
/// go on from.
export async function syncMail(session: Session, asked: Asked, cursor: Cursor | undefined, io: SyncIo): Promise<Cursor> {
  const next: Cursor = { folders: {} };
  const share = 1 / asked.folders.length;
  for (const [index, folder] of asked.folders.entries()) {
    stopped(io.signal);
    const done = index * share;
    io.progress(done, folder);
    const opened = await session.open(folder);
    const before = cursor?.folders[folder];
    const fetch = async (uids: number[]) => {
      for (let from = 0; from < uids.length; from += CHUNK) {
        stopped(io.signal);
        const mails = await session.fetch(uids.slice(from, from + CHUNK));
        await io.put(mails.map((mail) => itemOf(folder, opened.validity, mail)));
        io.progress(done + share * Math.min(1, (from + CHUNK) / uids.length), folder);
      }
    };
    if (!before || before.validity !== opened.validity) {
      // From nothing, or the server numbered the folder anew: what was synced under the old numbers is gone.
      if (before) await io.remove(uidsOf(before.have).map((uid) => idOf(folder, before.validity, uid)));
      const uids = (await session.since(new Date(asked.now.getTime() - asked.days * 86_400_000))).sort((a, b) => b - a);
      // The newest first: they are what is looked for.
      await fetch(uids);
      next.folders[folder] = { validity: opened.validity, top: highest([opened.next - 1, ...uids]), have: setOf(uids) };
      continue;
    }
    const fresh = (await session.after(before.top)).filter((uid) => uid > before.top);
    await fetch(fresh.sort((a, b) => b - a));
    const had = uidsOf(before.have);
    const kept = had.length ? await session.present(had) : [];
    const still = new Set(kept);
    const gone = had.filter((uid) => !still.has(uid));
    if (gone.length) await io.remove(gone.map((uid) => idOf(folder, opened.validity, uid)));
    next.folders[folder] = { validity: opened.validity, top: highest([before.top, opened.next - 1, ...fresh]), have: setOf([...kept, ...fresh]) };
  }
  io.progress(1);
  return next;
}

// --- The server, through imapflow ----------------------------------------------

/// What Heyo says it is (the IMAP ID command, which NetEase's servers want
/// before they let a mailbox be read).
const CLIENT = { name: "Heyo", vendor: "Heyo", "support-url": "https://github.com/hdonix/heyo" };

/// The most of a text part fetched: enough for 8000 characters in any
/// charset and transfer encoding.
const PART_LIMIT = 64 * 1024;

type Structure = MessageStructureObject & { parameters?: Record<string, string>; dispositionParameters?: Record<string, string> };

/// The part to read a mail's text from: the first text/plain that is not
/// an attachment, or else the first text/html.
export function textPart(root: Structure | undefined): Structure | undefined {
  const all: Structure[] = [];
  const walk = (node: Structure | undefined) => {
    if (!node) return;
    if (node.childNodes?.length) return node.childNodes.forEach((child) => walk(child as Structure));
    all.push(node);
  };
  walk(root);
  const body = all.filter((part) => part.disposition !== "attachment");
  return body.find((part) => part.type === "text/plain") ?? body.find((part) => part.type === "text/html");
}

/// The attachments of a mail: names, sizes (decoded, near enough) and types.
export function attachmentsOf(root: Structure | undefined, text?: Structure): Mail["attachments"] {
  const found: Mail["attachments"] = [];
  const walk = (node: Structure | undefined) => {
    if (!node) return;
    if (node.childNodes?.length) return node.childNodes.forEach((child) => walk(child as Structure));
    if (node === text) return;
    const name = node.dispositionParameters?.filename ?? node.parameters?.name;
    if (node.disposition !== "attachment" && !name) return;
    const size = node.size === undefined ? undefined : node.encoding?.toLowerCase() === "base64" ? Math.floor((node.size * 3) / 4) : node.size;
    found.push({ name: name ?? t("Attachment"), ...(size === undefined ? {} : { size }), type: node.type });
  };
  walk(root);
  return found;
}

/// The text of a part as it came (transfer-encoded, in its charset), decoded.
async function decode(part: Structure, raw: Buffer): Promise<string | undefined> {
  const charset = part.parameters?.charset;
  const head = [`Content-Type: ${part.type}${charset ? `; charset="${charset}"` : ""}`, `Content-Transfer-Encoding: ${part.encoding ?? "7bit"}`, "", ""].join("\r\n");
  const parsed = await simpleParser(Buffer.concat([Buffer.from(head), raw]), { skipImageLinks: true, skipTextToHtml: true, skipTextLinks: true });
  return part.type === "text/html" ? plain(typeof parsed.html === "string" ? parsed.html : parsed.text, true) : plain(parsed.text);
}

/// An account's server, signed in. Throws `auth` when the password is
/// refused, `unavailable` when the server is not there.
export async function connect(account: Account): Promise<ImapFlow> {
  const host = required(account, "host", t("Server"));
  const security = text(account, "security") || "tls";
  const client = new ImapFlow({
    host,
    port: number(account, "port", security === "tls" ? 993 : 143),
    secure: security === "tls",
    // `none` is for a server on this computer or network, which offers no TLS.
    doSTARTTLS: security === "starttls" ? true : security === "none" ? false : undefined,
    auth: { user: required(account, "username", t("User Name")), pass: await secret(account, "password") },
    clientInfo: CLIENT,
    logger: false,
    disableBinary: true,
    connectionTimeout: 30_000,
    greetingTimeout: 15_000,
    socketTimeout: 5 * 60_000,
  });
  // A dropped connection is the watch's and the sync's to see, not an uncaught error.
  client.on("error", () => {});
  try {
    await client.connect();
    // imapflow sends ID before signing in; NetEase only listens to it after.
    if (client.capabilities.has("ID")) await (client as any).run("ID", CLIENT).catch(() => {});
  } catch (error) {
    client.close();
    throw failure(error, (e) => e?.authenticationFailed || /auth|login|password|credential/i.test(String(e?.responseText ?? "")));
  }
  return client;
}

/// A `Session` over an imapflow connection.
export function session(client: ImapFlow): Session {
  return {
    async open(folder) {
      const mailbox = await client.mailboxOpen(folder, { readOnly: true });
      return { validity: String(mailbox.uidValidity), next: mailbox.uidNext };
    },
    async since(date) {
      return (await client.search({ since: date }, { uid: true })) || [];
    },
    async after(uid) {
      return (await client.search({ uid: `${uid + 1}:*` }, { uid: true })) || [];
    },
    async present(uids) {
      return (await client.search({ uid: setOf(uids) }, { uid: true })) || [];
    },
    async fetch(uids) {
      const described: FetchMessageObject[] = await client.fetchAll(setOf(uids), { uid: true, flags: true, envelope: true, internalDate: true, bodyStructure: true }, { uid: true });
      // The text parts, fetched by part number: most mails share theirs.
      const parts = new Map<string, { part: Structure; uids: number[] }>();
      for (const mail of described) {
        const part = textPart(mail.bodyStructure as Structure);
        if (!part) continue;
        const key = part.part ?? "1";
        const group = parts.get(`${key} ${part.type}`) ?? { part, uids: [] };
        group.uids.push(mail.uid);
        parts.set(`${key} ${part.type}`, group);
      }
      const texts = new Map<number, string>();
      for (const { part, uids } of parts.values()) {
        const key = part.part ?? "1";
        const bodies = await client.fetchAll(setOf(uids), { uid: true, bodyParts: [{ key, maxLength: PART_LIMIT }] }, { uid: true });
        for (const body of bodies) {
          const raw = body.bodyParts?.get(key) ?? [...(body.bodyParts?.values() ?? [])][0];
          // The structure of this very mail, for its charset and encoding.
          const own = textPart(described.find((mail) => mail.uid === body.uid)?.bodyStructure as Structure) ?? part;
          const decoded = raw && (await decode(own, raw).catch(() => undefined));
          if (decoded) texts.set(body.uid, decoded);
        }
      }
      return described.map((mail) => {
        const structure = mail.bodyStructure as Structure;
        const envelope = mail.envelope ?? {};
        const date = envelope.date ? new Date(envelope.date) : mail.internalDate ? new Date(mail.internalDate) : undefined;
        return {
          uid: mail.uid,
          subject: envelope.subject,
          date,
          from: envelope.from,
          to: envelope.to,
          cc: envelope.cc,
          flags: [...(mail.flags ?? [])],
          text: texts.get(mail.uid),
          attachments: attachmentsOf(structure, textPart(structure)),
        };
      });
    },
    async close() {
      await client.logout().catch(() => client.close());
    },
  };
}

// --- The connector -------------------------------------------------------------

const asked = (account: Account): Asked => ({
  folders: list(account, "folders", ["INBOX"]),
  days: Math.max(1, number(account, "days", 90)),
  now: new Date(),
});

/// Closes the connection when Heyo stops the sync, so a command waiting on
/// the server gives up at once.
function closedOnAbort(client: ImapFlow, signal: AbortSignal) {
  const close = () => client.close();
  signal.addEventListener("abort", close, { once: true });
  return () => signal.removeEventListener("abort", close);
}

export const imap: Connector = {
  async check(account) {
    const client = await connect(account);
    const server = session(client);
    try {
      for (const folder of asked(account).folders) await server.open(folder);
    } catch (error) {
      throw failure(error);
    } finally {
      await server.close();
    }
  },

  async sync(account, cursor, io) {
    const client = await connect(account);
    const release = closedOnAbort(client, io.signal);
    const server = session(client);
    try {
      return JSON.stringify(await syncMail(server, asked(account), cursorOf<Cursor>(cursor), io));
    } catch (error) {
      stopped(io.signal);
      throw failure(error);
    } finally {
      release();
      await server.close();
    }
  },

  async watch(account, io) {
    await watchInbox(account, io);
  },
};

/// Keeps INBOX in IDLE until `io.signal` aborts, saying when mail comes or
/// goes, and connecting again when the connection drops (`keepWatching`).
export async function watchInbox(account: Account, io: WatchIo, open: (account: Account) => Promise<ImapFlow> = connect) {
  await keepWatching(io, async (connected) => {
    const client = await open(account);
    const release = closedOnAbort(client, io.signal);
    try {
      const changed = () => io.changed();
      client.on("exists", changed);
      client.on("expunge", changed);
      client.on("flags", changed);
      await client.mailboxOpen("INBOX", { readOnly: true });
      connected();
      // IDLE ends when the server or imapflow (renewing it) ends it, or the connection drops.
      while (client.usable && !io.signal.aborted) await client.idle();
    } finally {
      release();
      client.close();
    }
  });
}
