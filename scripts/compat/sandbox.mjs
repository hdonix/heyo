//! How a third-party extension is kept away from the user's things while
//! it is launched: an environment with nothing of the user's in it, and,
//! where bubblewrap can be used without privileges, a view of the machine
//! that has only the system's programs, the runtime and one scratch
//! directory, with or without the network.
//!
//! Without bubblewrap only the environment is changed. That is not a
//! sandbox: the extension is a Node program and can read whatever the
//! user can. The report says which of the two a run had.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

/// The directories of one launch, all under `scratch`.
export function layout(scratch) {
  const home = path.join(scratch, "home");
  return {
    scratch,
    home,
    data: path.join(home, ".local", "share"),
    config: path.join(home, ".config"),
    cache: path.join(home, ".cache"),
    state: path.join(home, ".local", "state"),
    runtime: path.join(scratch, "run"),
    tmp: path.join(scratch, "tmp"),
    heyoData: path.join(scratch, "heyo", "data"),
    heyoConfig: path.join(scratch, "heyo", "config"),
    heyoCache: path.join(scratch, "heyo", "cache"),
  };
}

/// Everything the launched process is told. Nothing is inherited: no
/// tokens, no SSH or GPG agent, no DISPLAY, no session bus.
export function environment(dirs, { extensionsDir, background = false } = {}) {
  return {
    HOME: dirs.home,
    USER: "user",
    LOGNAME: "user",
    SHELL: "/bin/sh",
    PATH: "/usr/local/bin:/usr/bin:/bin",
    LANG: "C.UTF-8",
    TZ: "UTC",
    TMPDIR: dirs.tmp,
    XDG_DATA_HOME: dirs.data,
    XDG_CONFIG_HOME: dirs.config,
    XDG_CACHE_HOME: dirs.cache,
    XDG_STATE_HOME: dirs.state,
    XDG_RUNTIME_DIR: dirs.runtime,
    HEYO_DATA_DIR: dirs.heyoData,
    HEYO_CONFIG_DIR: dirs.heyoConfig,
    HEYO_CACHE_DIR: dirs.heyoCache,
    HEYO_EXTENSIONS_DIR: extensionsDir ?? path.join(dirs.heyoData, "extensions"),
    ...(background ? { HEYO_BACKGROUND: "1" } : {}),
  };
}

/// The arguments for `bwrap` that run `command` seeing only: the system's
/// programs and libraries (read-only), `readOnly` paths, and `writable`
/// paths. Paths are the same inside as outside, so a stack trace reads
/// the same. `exists` is asked which of the optional system paths are there.
export function bwrapArgs({ command, readOnly = [], writable = [], network = false, chdir, passwd, exists = fs.existsSync }) {
  const args = ["--unshare-all", ...(network ? ["--share-net"] : []), "--die-with-parent", "--new-session", "--hostname", "heyo-compat"];
  args.push("--ro-bind", "/usr", "/usr");
  // Merged-/usr systems have these as links into /usr.
  for (const dir of ["bin", "sbin", "lib", "lib64", "lib32"]) {
    if (exists(`/usr/${dir}`) && exists(`/${dir}`)) args.push("--symlink", `usr/${dir}`, `/${dir}`);
  }
  args.push("--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--tmpfs", "/run");
  // What programs need to resolve names and trust certificates; nothing
  // of /etc that describes the user or the machine.
  for (const file of ["/etc/resolv.conf", "/etc/hosts", "/etc/nsswitch.conf", "/etc/ssl", "/etc/ca-certificates", "/etc/pki", "/etc/alternatives", "/etc/ld.so.cache", "/etc/localtime"]) {
    args.push("--ro-bind-try", file, file);
  }
  // A made-up account, so that asking who the user is has an answer.
  if (passwd) args.push("--ro-bind", passwd.passwd, "/etc/passwd", "--ro-bind", passwd.group, "/etc/group");
  for (const dir of readOnly) args.push("--ro-bind", dir, dir);
  for (const dir of writable) args.push("--bind", dir, dir);
  if (chdir) args.push("--chdir", chdir);
  return [...args, "--", ...command];
}

let probed;

/// Whether bubblewrap can make a sandbox here without privileges.
export function bwrapWorks() {
  if (probed === undefined) {
    const result = spawnSync("bwrap", ["--unshare-all", "--ro-bind", "/usr", "/usr", "--symlink", "usr/bin", "/bin", "--symlink", "usr/lib", "/lib", "--symlink", "usr/lib64", "/lib64", "--", "/usr/bin/true"], {
      stdio: "ignore",
      timeout: 10_000,
    });
    probed = result.status === 0;
  }
  return probed;
}

/// Files that stand for /etc/passwd and /etc/group inside the sandbox.
export function writeAccount(dir, home) {
  const uid = process.getuid?.() ?? 1000;
  const gid = process.getgid?.() ?? 1000;
  const passwd = path.join(dir, "passwd");
  const group = path.join(dir, "group");
  fs.writeFileSync(passwd, `root:x:0:0:root:/root:/bin/sh\nuser:x:${uid}:${gid}:Test User:${home}:/bin/sh\n`);
  fs.writeFileSync(group, `root:x:0:\nuser:x:${gid}:\n`);
  return { passwd, group };
}
