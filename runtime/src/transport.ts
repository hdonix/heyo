//! What carries the runtime's conversation with the host (`protocol.ts`):
//! lines of text, both ways and in order. In Heyo that is the process's
//! stdin and stdout; in a test it is a pair held in memory, with the test
//! in the host's place.

import { StringDecoder } from "node:string_decoder";

export interface Transport {
  /// Writes one line to the other end.
  send(line: string): void;
  /// Hands over each line the other end wrote, in order, and says when
  /// there will be no more. One reader at a time: a later one takes over.
  receive(line: (line: string) => void, closed: () => void): void;
}

interface Reader {
  line(line: string): void;
  closed(): void;
}

/// Lines read from `input` and written with `write`. A line ends at a line
/// feed and nowhere else: `readline` also ends one at U+2028 and U+2029,
/// which JSON leaves as they are inside a string, and a message carrying
/// text with either in it would arrive in pieces.
export function streams(input: NodeJS.ReadableStream, write: (text: string) => void): Transport {
  let reader: Reader | undefined;
  return {
    send: (line) => write(line + "\n"),
    receive(line, closed) {
      // Nothing is read before someone is there to hear it.
      const reading = reader !== undefined;
      reader = { line, closed };
      if (reading) return;
      const decoder = new StringDecoder("utf8");
      // The beginning of a line whose end has not come yet.
      let held = "";
      input.on("data", (chunk: Buffer | string) => {
        const text = typeof chunk === "string" ? chunk : decoder.write(chunk);
        let from = 0;
        for (let end = text.indexOf("\n"); end !== -1; end = text.indexOf("\n", from)) {
          const whole = held + text.slice(from, end);
          held = "";
          from = end + 1;
          reader!.line(whole);
        }
        held += text.slice(from);
      });
      input.once("end", () => reader!.closed());
    },
  };
}

/// The process's own pipes. From here on stdout is the host's alone:
/// extensions log freely, and what they print goes to stderr.
export function pipes(): Transport {
  const write = process.stdout.write.bind(process.stdout);
  process.stdout.write = process.stderr.write.bind(process.stderr) as typeof process.stdout.write;
  return streams(process.stdin, (text) => void write(text));
}

export interface End extends Transport {
  /// Hangs up: the other end is told there will be no more.
  close(): void;
}

/// One end's side of a pair: who reads there, and what waits to be read.
interface Half {
  reader?: Reader;
  waiting: string[];
  closed: boolean;
}

/// Two ends in memory: what one sends the other has read by the time
/// `send` returns. What is sent before the other end listens waits for it,
/// as it would in a pipe.
export function pair(): [End, End] {
  const end = (mine: Half, theirs: Half): End => ({
    send(line) {
      if (mine.closed || theirs.closed) return;
      if (theirs.reader) theirs.reader.line(line);
      else theirs.waiting.push(line);
    },
    receive(line, closed) {
      mine.reader = { line, closed };
      mine.waiting.splice(0).forEach(line);
      if (theirs.closed) closed();
    },
    close() {
      if (mine.closed) return;
      mine.closed = true;
      theirs.reader?.closed();
    },
  });
  const [a, b]: Half[] = [
    { waiting: [], closed: false },
    { waiting: [], closed: false },
  ];
  return [end(a, b), end(b, a)];
}
