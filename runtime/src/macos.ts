//! What tells something written for macOS: the programs only macOS has,
//! and a file that is one of its executables. The runtime (`compat.ts`),
//! the developer tools (`dev/check.ts`) and the compatibility run
//! (`scripts/compat/classify.mjs`, out of the developer tools as built) all
//! go by these.

import fs from "node:fs";

/// Programs that come with macOS and that nothing else has under that name.
/// (`open` is not one: with a plain file or address it has equivalents.)
export const MAC_PROGRAMS: ReadonlySet<string> = new Set([
  "osascript", "pbcopy", "pbpaste", "defaults", "mdfind", "mdls", "say", "pmset", "networksetup", "launchctl", "screencapture",
  "sips", "caffeinate", "diskutil", "system_profiler", "scutil", "plutil", "afplay", "security", "ditto", "hdiutil", "sw_vers",
  "softwareupdate", "shortcuts", "qlmanage", "tmutil", "airport", "automator", "dscl", "spctl", "codesign", "xattr", "osacompile",
  "PlistBuddy", "lsregister", "mdutil", "ioreg", "vm_stat", "mas", "xcrun", "xcodebuild",
]);

/// Whether a file is a macOS (Mach-O) executable.
export function isMacProgram(file: string) {
  try {
    const head = Buffer.alloc(4);
    const fd = fs.openSync(file, "r");
    try {
      fs.readSync(fd, head, 0, 4, 0);
    } finally {
      fs.closeSync(fd);
    }
    // Universal binaries and 32/64-bit ones, in either byte order.
    return ["cafebabe", "bebafeca", "feedface", "cefaedfe", "feedfacf", "cffaedfe"].includes(head.toString("hex"));
  } catch {
    return false;
  }
}
