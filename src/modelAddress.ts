// Where the "local model" setting really points. Selected text, questions
// and dictation go to that address, so the settings page says when it is
// not this computer. Only loopback is: an address in a private range
// (192.168.…, 10.…) is another machine, however near.

export type ModelAddress =
  /// A loopback address: text stays on this computer.
  | { where: "this-computer" }
  /// Another machine. `encrypted` is false for plain http, which anyone on
  /// the way can read.
  | { where: "elsewhere"; host: string; encrypted: boolean }
  /// Not an http(s) URL at all.
  | { where: "unknown" };

/// Whether a host name, as `URL` normalizes it, is this computer.
function isLoopback(hostname: string) {
  // "localhost." is the same name, fully qualified.
  const host = hostname.replace(/\.$/, "");
  if (host === "localhost" || host.endsWith(".localhost")) return true;
  // URL writes every form of an IPv4 address (127.1, 0x7f.0.0.1, 2130706433) as four decimals.
  if (/^127(\.\d{1,3}){3}$/.test(host)) return true;
  if (host === "[::1]") return true;
  // IPv4 loopback written as IPv6: URL prints ::ffff:127.0.0.1 as [::ffff:7f00:1].
  return /^\[::ffff:7f[0-9a-f]{2}:[0-9a-f]{1,4}\]$/.test(host);
}

export function classifyModelAddress(address: string): ModelAddress {
  let url: URL;
  try {
    url = new URL(address.trim());
  } catch {
    return { where: "unknown" };
  }
  if ((url.protocol !== "http:" && url.protocol !== "https:") || !url.hostname) return { where: "unknown" };
  if (isLoopback(url.hostname)) return { where: "this-computer" };
  return { where: "elsewhere", host: url.host, encrypted: url.protocol === "https:" };
}
