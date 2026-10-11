// What carries the conversation: lines over two streams as the process
// has them, and the pair in memory that tests use in their place.
import { PassThrough } from "node:stream";
import { expect, test } from "vitest";

import { pair, streams } from "./transport";

/// A transport over streams, with the far side of each in hand.
function piped() {
  const [input, written] = [new PassThrough(), [] as string[]];
  const lines: string[] = [];
  let closed = false;
  const transport = streams(input, (text) => void written.push(text));
  const listen = () =>
    transport.receive(
      (line) => lines.push(line),
      () => (closed = true),
    );
  const settled = () => new Promise((resolve) => setTimeout(resolve, 10));
  return { input, written, lines, transport, listen, settled, closed: () => closed };
}

test("a line is written with its end, and read without it", async () => {
  const { input, written, lines, transport, listen, settled, closed } = piped();
  transport.send('{"type":"ready"}');
  expect(written).toEqual(['{"type":"ready"}\n']);

  // What came before anyone listened is not lost.
  input.write('{"a":1}\n{"b"');
  await settled();
  listen();
  await settled();
  expect(lines).toEqual(['{"a":1}']);
  // Lines arrive cut anywhere, a character of several bytes included.
  const rest = Buffer.from(':"é"}\n{"c":3}\n');
  input.write(rest.subarray(0, 3));
  input.write(rest.subarray(3));
  await settled();
  expect(lines).toEqual(['{"a":1}', '{"b":"é"}', '{"c":3}']);

  expect(closed()).toBe(false);
  input.end();
  await settled();
  expect(closed()).toBe(true);
});

test("only a line feed ends a line", async () => {
  const { input, lines, listen, settled } = piped();
  listen();
  // JSON leaves these two as they are inside a string; they are text the
  // user copied, not the end of a message.
  const message = JSON.stringify({ type: "reply", id: 1, result: "one two three" });
  input.write(message + "\n");
  await settled();
  expect(lines).toEqual([message]);
  expect(JSON.parse(lines[0]).result).toBe("one two three");
});

test("a pair in memory hands each end what the other sent, in order", () => {
  const [a, b] = pair();
  const [atA, atB]: string[][] = [[], []];
  let gone = 0;
  // Sent before the other end listens: it waits, as it would in a pipe.
  a.send("one");
  a.send("two");
  b.receive(
    (line) => atB.push(line),
    () => gone++,
  );
  a.receive(
    (line) => atA.push(line),
    () => {},
  );
  a.send("three");
  b.send("back");
  expect(atB).toEqual(["one", "two", "three"]);
  expect(atA).toEqual(["back"]);

  a.close();
  a.close();
  expect(gone).toBe(1);
  // Nothing crosses once one end hung up.
  a.send("late");
  b.send("later");
  expect(atB).toHaveLength(3);
  expect(atA).toHaveLength(1);
});

test("an end that listens after the other hung up is told at once", () => {
  const [a, b] = pair();
  a.send("last words");
  a.close();
  const heard: string[] = [];
  let gone = false;
  b.receive(
    (line) => heard.push(line),
    () => (gone = true),
  );
  expect(heard).toEqual(["last words"]);
  expect(gone).toBe(true);
});
