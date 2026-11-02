//! The IMAP connector against GreenMail, a mail server for tests, in a
//! container: run with `HEYO_CONTAINER_TESTS=1` where podman or docker is.

import { ImapFlow } from "imapflow";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

import type { Account } from "../../heyo/connector";
import { engine, recorder, start, type Container } from "./testing";
import { imap } from "./imap";

const command = engine();

/// "你好，世界" in GBK, as a mail from China would carry it.
const GBK_HELLO = Buffer.from([0xc4, 0xe3, 0xba, 0xc3, 0xa3, 0xac, 0xca, 0xc0, 0xbd, 0xe7]);

function mail(subject: string, body: string, date = new Date()) {
  return [`From: Ann <ann@example.com>`, `To: test@example.com`, `Subject: ${subject}`, `Date: ${date.toUTCString()}`, `Message-ID: <${Math.random()}@example.com>`, "MIME-Version: 1.0", body].join("\r\n");
}

const plainMail = (subject: string, text: string, date?: Date) => mail(subject, ["Content-Type: text/plain; charset=utf-8", "", text].join("\r\n"), date);

const chineseMail = () =>
  mail(
    `=?GBK?B?${GBK_HELLO.toString("base64")}?=`,
    [
      'Content-Type: multipart/mixed; boundary="b"',
      "",
      "--b",
      "Content-Type: text/html; charset=gbk",
      "Content-Transfer-Encoding: base64",
      "",
      Buffer.concat([Buffer.from("<p><b>"), GBK_HELLO, Buffer.from("</b></p>")]).toString("base64"),
      "--b",
      'Content-Type: application/pdf; name="plan.pdf"',
      'Content-Disposition: attachment; filename="plan.pdf"',
      "Content-Transfer-Encoding: base64",
      "",
      Buffer.alloc(300, 1).toString("base64"),
      "--b--",
    ].join("\r\n"),
  );

describe.skipIf(!command)("IMAP against GreenMail", () => {
  let server: Container;
  let account: Account;
  /// A connection of the test's own, to put mail in and take it out.
  let admin: ImapFlow;
  let port: number;

  beforeEach(async () => {
    admin = new ImapFlow({ host: "127.0.0.1", port, secure: false, doSTARTTLS: false, auth: { user: "test", pass: "secret" }, logger: false });
    admin.on("error", () => {});
    await admin.connect();
  });

  afterEach(async () => {
    await admin.logout().catch(() => admin.close());
  });

  beforeAll(async () => {
    server = await start(command!, "docker.io/greenmail/standalone:2.1.8", [3143], {
      env: { GREENMAIL_OPTS: "-Dgreenmail.setup.test.imap -Dgreenmail.hostname=0.0.0.0 -Dgreenmail.users=test:secret@example.com -Dgreenmail.users.login=local_part -Dgreenmail.verbose=false" },
    });
    port = server.port(3143);
    const config = { host: "127.0.0.1", port, security: "none", username: "test", folders: ["INBOX", "Work"], days: 30 };
    account = { id: "mail", name: "Mail", config, secret: async (name) => (name === "password" ? "secret" : undefined) };
    const setup = new ImapFlow({ host: "127.0.0.1", port, secure: false, doSTARTTLS: false, auth: { user: "test", pass: "secret" }, logger: false });
    await setup.connect();
    await setup.mailboxCreate("Work");
    await setup.logout();
  }, 120_000);

  afterAll(() => {
    server?.stop();
  });

  function io() {
    const kept = recorder();
    return { ...kept, sync: kept.io };
  }

  it("checks the password", async () => {
    await expect(imap.check(account)).resolves.toBeUndefined();
    const wrong = { ...account, secret: async () => "nope" };
    await expect(imap.check(wrong)).rejects.toMatchObject({ kind: "auth" });
    const gone = { ...account, config: { ...account.config, port: 1 } };
    await expect(imap.check(gone)).rejects.toMatchObject({ kind: "unavailable" });
  });

  it("syncs the recent mail, then what is new, and removes what went", async () => {
    await admin.append("INBOX", plainMail("Old", "From last year", new Date("2025-01-01T00:00:00Z")), [], new Date("2025-01-01T00:00:00Z"));
    await admin.append("INBOX", plainMail("Lunch", "Shall we eat at noon?"));
    await admin.append("INBOX", chineseMail(), ["\\Seen"]);
    await admin.append("Work", plainMail("Report", "Due Friday"));

    const first = io();
    const cursor = await imap.sync(account, undefined, first.sync);
    const titles = first.put.map((item) => item.title).sort();
    expect(titles).toEqual(["Lunch", "Report", "你好，世界"].sort());
    const chinese = first.put.find((item) => item.title === "你好，世界")!;
    expect(chinese.text).toBe("你好，世界");
    expect(chinese.attachments).toEqual([{ name: "plan.pdf", size: 300, type: "application/pdf" }]);
    expect(chinese.flags).toBeUndefined();
    expect(first.put.find((item) => item.title === "Lunch")).toMatchObject({ text: "Shall we eat at noon?", flags: ["unread"], folder: "INBOX", people: [{ name: "Ann", address: "ann@example.com", role: "from" }, { address: "test@example.com", role: "to" }] });

    await admin.append("INBOX", plainMail("Dinner", "At seven"));
    const lock = await admin.getMailboxLock("INBOX");
    try {
      await admin.messageDelete({ header: { subject: "Lunch" } } as any);
    } finally {
      lock.release();
    }
    const second = io();
    const next = await imap.sync(account, cursor, second.sync);
    expect(second.put.map((item) => item.title)).toEqual(["Dinner"]);
    expect(second.removed).toEqual([first.put.find((item) => item.title === "Lunch")!.id]);

    const third = io();
    await imap.sync(account, next, third.sync);
    expect(third.put).toEqual([]);
    expect(third.removed).toEqual([]);
  });

  it("fetches a folder again whole when its UIDVALIDITY changes", async () => {
    const first = io();
    const cursor = await imap.sync(account, undefined, first.sync);
    const report = first.put.find((item) => item.title === "Report")!;
    // A new folder of the same name is numbered anew. (Moved away, not
    // deleted: GreenMail fails to delete a folder another session read.)
    await admin.mailboxRename("Work", "Work-old");
    await admin.mailboxCreate("Work");
    await admin.append("Work", plainMail("Report", "Due Monday now"));
    const second = io();
    await imap.sync(account, cursor, second.sync);
    expect(second.removed).toContain(report.id);
    const renewed = second.put.find((item) => item.title === "Report")!;
    expect(renewed.id).not.toBe(report.id);
    expect(renewed.text).toBe("Due Monday now");
  });

  it("watches INBOX and says when mail comes", async () => {
    const controller = new AbortController();
    const changed = vi.fn();
    const watching = imap.watch!(account, { changed, signal: controller.signal });
    // Let it sign in and settle in IDLE.
    await new Promise((done) => setTimeout(done, 1500));
    await admin.append("INBOX", plainMail("Ping", "New"));
    await vi.waitFor(() => expect(changed).toHaveBeenCalled(), { timeout: 10_000 });
    controller.abort();
    await watching;
  }, 30_000);
});
