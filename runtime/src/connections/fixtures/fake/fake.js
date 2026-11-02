"use strict";
// A connector for tests, as an extension's build would leave it. What it
// syncs is what the file `config.source` says:
//
//   { "version": 2, "password": "…", "items": [ConnectorItem…], "removed": ["id"…],
//     "fail": "auth" | "unavailable" | "failed", "hang": true | "deaf" }
//
// Its cursor is the version: a sync from the version there is brings
// nothing. `hang` makes a sync wait until it is stopped (`"deaf"`: wait
// for ever, stopped or not). Its watch reports each change of the file.

const fs = require("node:fs");
const { ConnectorError } = require("@heyo/api");

function read(account) {
  return JSON.parse(fs.readFileSync(String(account.config.source), "utf8"));
}

async function signedIn(account, source) {
  if ((await account.secret("password")) !== source.password) throw new ConnectorError("auth", "The password was refused.");
}

module.exports.default = {
  async check(account) {
    await signedIn(account, read(account));
  },

  async sync(account, cursor, io) {
    const source = read(account);
    if (source.hang === "deaf") await new Promise(() => {});
    if (source.hang) {
      await new Promise((_, stopped) => io.signal.addEventListener("abort", () => stopped(io.signal.reason)));
    }
    if (source.fail) throw new ConnectorError(source.fail, `Told to fail (${source.fail}).`);
    await signedIn(account, source);
    const version = String(source.version ?? 1);
    if (cursor === version) return version;
    io.progress(0, "Reading");
    await io.put(source.items ?? []);
    if (cursor !== undefined) await io.remove(source.removed ?? []);
    io.progress(1);
    return version;
  },

  async watch(account, io) {
    await new Promise((done) => {
      const watcher = fs.watch(String(account.config.source), () => io.changed());
      io.signal.addEventListener("abort", () => {
        watcher.close();
        done();
      });
    });
  },
};
