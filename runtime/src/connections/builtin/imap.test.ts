import { EventEmitter } from "node:events";
import { describe, expect, it, vi } from "vitest";

import { ConnectorError, type Item, type SyncIo } from "../../heyo/connector";
import { loader } from "../load";
import { RECONNECT } from "./common";
import { attachmentsOf, imap, itemOf, setOf, syncMail, textPart, uidsOf, watchInbox, type Mail, type Session } from "./imap";
import { recorder } from "./testing";

/// A server in memory: folders of mails by UID, each folder numbered by
/// its UIDVALIDITY.
class Server implements Session {
  folders = new Map<string, { validity: string; mails: Map<number, Mail & { arrived: Date }> }>();
  private opened?: string;
  fetched: number[][] = [];

  add(folder: string, uid: number, arrived: string, subject = `Mail ${uid}`) {
    const box = this.folders.get(folder) ?? { validity: "1", mails: new Map() };
    this.folders.set(folder, box);
    box.mails.set(uid, { uid, subject, date: new Date(arrived), arrived: new Date(arrived), from: [{ name: "Ann", address: "ann@example.com" }], flags: [], attachments: [] });
  }

  private box() {
    return this.folders.get(this.opened!)!;
  }

  async open(folder: string) {
    this.opened = folder;
    const box = this.folders.get(folder);
    if (!box) throw new Error(`no folder ${folder}`);
    return { validity: box.validity, next: Math.max(0, ...box.mails.keys()) + 1 };
  }
  async since(date: Date) {
    return [...this.box().mails.values()].filter((mail) => mail.arrived >= date).map((mail) => mail.uid);
  }
  async after(uid: number) {
    const all = [...this.box().mails.keys()];
    // As a server answers `n:*`: the highest is there even when it is below n.
    return [...new Set([...all.filter((each) => each > uid), Math.max(...all)])];
  }
  async present(uids: number[]) {
    return uids.filter((uid) => this.box().mails.has(uid));
  }
  async fetch(uids: number[]) {
    this.fetched.push(uids);
    return uids.map((uid) => this.box().mails.get(uid)!).filter(Boolean);
  }
  async close() {}
}

/// What a sync put and removed.
function io() {
  const kept = recorder();
  return { ...kept, sync: kept.io };
}

const now = new Date("2026-11-03T12:00:00Z");
const asked = { folders: ["INBOX"], days: 30, now };

describe("IMAP sync", () => {
  it("brings the recent days first, then only what is new, and removes what went", async () => {
    const server = new Server();
    server.add("INBOX", 1, "2026-01-01T00:00:00Z");
    server.add("INBOX", 2, "2026-10-20T00:00:00Z");
    server.add("INBOX", 3, "2026-11-02T00:00:00Z");

    const first = io();
    const cursor = await syncMail(server, asked, undefined, first.sync);
    expect(first.put.map((item) => item.id).sort()).toEqual(["INBOX/1/2", "INBOX/1/3"]);
    expect(first.put[0]).toMatchObject({ kind: "email", folder: "INBOX", flags: ["unread"], people: [{ name: "Ann", address: "ann@example.com", role: "from" }] });
    expect(cursor.folders.INBOX).toEqual({ validity: "1", top: 3, have: "2:3" });

    server.add("INBOX", 4, "2026-11-03T08:00:00Z");
    server.folders.get("INBOX")!.mails.delete(2);
    server.fetched = [];
    const second = io();
    const next = await syncMail(server, asked, JSON.parse(JSON.stringify(cursor)), second.sync);
    expect(server.fetched).toEqual([[4]]);
    expect(second.put.map((item) => item.id)).toEqual(["INBOX/1/4"]);
    expect(second.removed).toEqual(["INBOX/1/2"]);
    expect(next.folders.INBOX).toEqual({ validity: "1", top: 4, have: "3:4" });

    // Nothing new: the highest UID the server names for `5:*` is not fetched again.
    server.fetched = [];
    const third = io();
    await syncMail(server, asked, next, third.sync);
    expect(server.fetched).toEqual([]);
    expect(third.put).toEqual([]);
  });

  it("fetches a folder again whole when its UIDVALIDITY changes", async () => {
    const server = new Server();
    server.add("INBOX", 7, "2026-11-01T00:00:00Z");
    const first = io();
    const cursor = await syncMail(server, asked, undefined, first.sync);

    server.folders.set("INBOX", { validity: "2", mails: new Map() });
    server.add("INBOX", 1, "2026-11-01T00:00:00Z", "Renumbered");
    const second = io();
    const next = await syncMail(server, asked, cursor, second.sync);
    expect(second.removed).toEqual(["INBOX/1/7"]);
    expect(second.put.map((item) => [item.id, item.title])).toEqual([["INBOX/2/1", "Renumbered"]]);
    expect(next.folders.INBOX.validity).toBe("2");
  });

  it("syncs each folder on its own, and a new folder from nothing", async () => {
    const server = new Server();
    server.add("INBOX", 1, "2026-11-01T00:00:00Z");
    server.add("Sent", 1, "2026-11-01T00:00:00Z");
    const first = io();
    const cursor = await syncMail(server, { ...asked, folders: ["INBOX", "Sent"] }, undefined, first.sync);
    expect(first.put.map((item) => item.id)).toEqual(["INBOX/1/1", "Sent/1/1"]);
    expect(Object.keys(cursor.folders)).toEqual(["INBOX", "Sent"]);
  });

  it("stops between batches when Heyo stops it", async () => {
    const server = new Server();
    for (let uid = 1; uid <= 120; uid++) server.add("INBOX", uid, "2026-11-01T00:00:00Z");
    const controller = new AbortController();
    const put: Item[] = [];
    const sync: SyncIo = {
      ...recorder(controller.signal).io,
      put: async (items) => {
        put.push(...items);
        controller.abort(new Error("stopped"));
      },
    };
    await expect(syncMail(server, asked, undefined, sync)).rejects.toThrow("stopped");
    expect(put).toHaveLength(50);
  });
});

describe("IMAP pieces", () => {
  it("is what an account of @heyo:imap is synced by", () => {
    expect(loader()({ extension: "@heyo", command: "imap" })).toBe(imap);
  });

  it("writes UIDs as IMAP sets and reads them back", () => {
    expect(setOf([5, 1, 2, 3, 9, 10, 3])).toBe("1:3,5,9:10");
    expect(uidsOf("1:3,5,9:10")).toEqual([1, 2, 3, 5, 9, 10]);
    expect(setOf([])).toBe("");
    expect(uidsOf("")).toEqual([]);
  });

  it("reads the text from the first plain part, else the HTML, and lists the attachments", () => {
    const mixed = {
      type: "multipart/mixed",
      childNodes: [
        { part: "1", type: "multipart/alternative", childNodes: [{ part: "1.1", type: "text/plain", size: 10 }, { part: "1.2", type: "text/html", size: 20 }] },
        { part: "2", type: "application/pdf", disposition: "attachment", dispositionParameters: { filename: "plan.pdf" }, encoding: "base64", size: 400 },
        { part: "3", type: "text/plain", disposition: "attachment", dispositionParameters: { filename: "notes.txt" }, size: 9 },
      ],
    };
    expect(textPart(mixed as any)?.part).toBe("1.1");
    expect(attachmentsOf(mixed as any, textPart(mixed as any))).toEqual([
      { name: "plan.pdf", size: 300, type: "application/pdf" },
      { name: "notes.txt", size: 9, type: "text/plain" },
    ]);
    const html = { type: "multipart/alternative", childNodes: [{ part: "1", type: "text/html" }] };
    expect(textPart(html as any)?.part).toBe("1");
    expect(textPart({ type: "image/png" } as any)).toBeUndefined();
  });

  it("makes an item of a mail, numbered by its folder, UIDVALIDITY and UID", () => {
    const mail: Mail = { uid: 4, date: new Date("2026-11-01T08:00:00Z"), flags: ["\\Seen", "\\Flagged"], to: [{ address: "me@example.com" }], attachments: [] };
    expect(itemOf("INBOX", "9", mail)).toEqual({
      id: "INBOX/9/4",
      kind: "email",
      title: "(no subject)",
      time: "2026-11-01T08:00:00.000Z",
      people: [{ address: "me@example.com", role: "to" }],
      folder: "INBOX",
      flags: ["flagged"],
    });
  });
});

/// A connection whose IDLE lasts until the test drops it.
class FakeClient extends EventEmitter {
  usable = true;
  private drop?: () => void;
  async mailboxOpen() {}
  idle() {
    return new Promise<boolean>((done) => (this.drop = () => done(false)));
  }
  close() {
    this.usable = false;
    this.drop?.();
  }
}

describe("IMAP watch", () => {
  const account = { id: "m", name: "m", config: {}, secret: async () => "x" };

  it("says when mail comes, connects again when the connection drops, and stops when told", async () => {
    vi.useFakeTimers();
    try {
      const clients: FakeClient[] = [];
      const controller = new AbortController();
      const changed = vi.fn();
      const open = async () => {
        const client = new FakeClient();
        clients.push(client);
        return client as any;
      };
      const watching = watchInbox(account, { changed, signal: controller.signal }, open);
      await vi.waitFor(() => expect(clients).toHaveLength(1));
      clients[0].emit("exists");
      expect(changed).toHaveBeenCalledTimes(1);

      clients[0].close();
      await vi.advanceTimersByTimeAsync(RECONNECT.first);
      await vi.waitFor(() => expect(clients).toHaveLength(2));
      // Connected again: what came meanwhile is synced.
      await vi.waitFor(() => expect(changed).toHaveBeenCalledTimes(2));

      controller.abort();
      await watching;
      expect(clients[1].usable).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("gives up when the password is refused", async () => {
    const open = async () => {
      throw new ConnectorError("auth", "refused");
    };
    await expect(watchInbox(account, { changed: () => {}, signal: new AbortController().signal }, open)).resolves.toBeUndefined();
  });
});
