//! For the tests of Heyo's own connectors: a `SyncIo` that keeps what a
//! sync did, and servers in containers to test against real ones
//! (GreenMail for IMAP, Radicale for CalDAV). Those run only when asked
//! (`HEYO_CONTAINER_TESTS=1`) and a container engine is there (podman or
//! docker); each container is started on ports of the loopback only, and
//! removed by its id when the tests are done.

import { execFileSync, spawnSync } from "node:child_process";
import net from "node:net";

import type { Item, SyncIo, Tool } from "../../heyo/connector";

/// The time the calendar tests are run at.
export const now = new Date("2026-11-03T12:00:00Z");

/// iCalendar of these lines, as servers write it: joined by CRLF.
export const calendar = (...lines: string[]) => ["BEGIN:VCALENDAR", "VERSION:2.0", "PRODID:-//Heyo//Test//EN", ...lines, "END:VCALENDAR"].join("\r\n");

/// A sync's `io`, keeping what it put, removed and said of its tools.
export function recorder(signal = new AbortController().signal) {
  const put: Item[] = [];
  const removed: string[] = [];
  const tools: Tool[][] = [];
  const io: SyncIo = {
    put: async (items) => void put.push(...items),
    remove: async (ids) => void removed.push(...ids),
    progress: () => {},
    tools: (said) => void tools.push(said),
    signal,
  };
  return { put, removed, tools, io };
}

/// The container engine there is, if the container tests are asked for.
export function engine(): string | undefined {
  if (process.env.HEYO_CONTAINER_TESTS !== "1") return undefined;
  for (const command of ["podman", "docker"]) {
    if (spawnSync(command, ["--version"], { stdio: "ignore" }).status === 0) return command;
  }
  return undefined;
}

export interface Container {
  /// The port on 127.0.0.1 the container's `port` is published on.
  port(port: number): number;
  stop(): void;
}

/// Starts `image` with `args` after it, its `ports` published on random
/// loopback ports, and waits until the first of them takes connections.
export async function start(command: string, image: string, ports: number[], options: { env?: Record<string, string>; volumes?: string[]; args?: string[]; probe?: string } = {}): Promise<Container> {
  const env = Object.entries(options.env ?? {}).flatMap(([name, value]) => ["-e", `${name}=${value}`]);
  const published = ports.flatMap((port) => ["-p", `127.0.0.1::${port}`]);
  const volumes = (options.volumes ?? []).flatMap((volume) => ["-v", volume]);
  const id = execFileSync(command, ["run", "-d", "--rm", ...env, ...volumes, ...published, image, ...(options.args ?? [])], { encoding: "utf8" }).trim();
  const mapped = new Map<number, number>();
  for (const port of ports) {
    const line = execFileSync(command, ["port", id, String(port)], { encoding: "utf8" }).trim().split("\n")[0];
    mapped.set(port, Number(line.slice(line.lastIndexOf(":") + 1)));
  }
  const container: Container = {
    port: (port) => mapped.get(port)!,
    stop: () => void spawnSync(command, ["rm", "-f", id], { stdio: "ignore" }),
  };
  try {
    await listening(mapped.get(ports[0])!, options.probe);
  } catch (error) {
    container.stop();
    throw error;
  }
  return container;
}

/// Resolves once 127.0.0.1:`port` answers with something, after `probe`
/// is sent if given (a published port takes connections before the server
/// behind it is up).
async function listening(port: number, probe?: string, within = 90_000) {
  const until = Date.now() + within;
  while (Date.now() < until) {
    const answered = await new Promise<boolean>((done) => {
      const socket = net.connect(port, "127.0.0.1");
      socket.setTimeout(2000);
      if (probe) socket.once("connect", () => socket.write(probe));
      socket.once("data", () => (socket.destroy(), done(true)));
      socket.once("timeout", () => (socket.destroy(), done(false)));
      socket.once("error", () => done(false));
      socket.once("close", () => done(false));
    });
    if (answered) return;
    await new Promise((done) => setTimeout(done, 500));
  }
  throw new Error(`nothing answered on port ${port}`);
}
