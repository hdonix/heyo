import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterAll, expect, test } from "vitest";

import { listDirectory, startDirectory, toggled } from "./files";
import { looksLikeText, preview, previewMarkdown, resolvePath } from "./quicklook";

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "heyo-quicklook-"));
afterAll(() => fs.rmSync(dir, { recursive: true, force: true }));
const file = (name: string, content: string | Buffer) => {
  const full = path.join(dir, name);
  fs.writeFileSync(full, content);
  return full;
};

test("a picture is sent whole, by its kind", () => {
  const png = Buffer.from("iVBORw0KGgo=", "base64");
  expect(preview(file("dot.PNG", png))).toEqual({ kind: "image", url: "data:image/png;base64,iVBORw0KGgo=" });
  expect(preview(file("drawing.svg", "<svg/>"))).toMatchObject({ kind: "image", url: expect.stringMatching(/^data:image\/svg\+xml;base64,/) });
});

test("a picture too large to send is left to an image viewer", () => {
  expect(preview(file("huge.jpg", Buffer.alloc(6 * 1024 * 1024 + 1)))).toEqual({ kind: "other" });
});

test("text is shown, the beginning of it when it is long", () => {
  expect(preview(file("notes.md", "# héllo\n"))).toEqual({ kind: "text", text: "# héllo\n", truncated: false });
  expect(preview(file("empty", ""))).toEqual({ kind: "text", text: "", truncated: false });
  // Cut in the middle of a character: the half is dropped, not shown broken.
  const long = preview(file("long.log", "é".repeat(40_000) + "x"));
  expect(long).toMatchObject({ kind: "text", truncated: true });
  expect(long.kind === "text" && /^é+$/.test(long.text)).toBe(true);
});

test("what is not text or a picture is for another application", () => {
  expect(preview(file("archive.zip", Buffer.from([0x50, 0x4b, 3, 4, 0, 0, 0])))).toEqual({ kind: "other" });
  expect(preview(file("latin1.txt", Buffer.from([0x63, 0x61, 0x66, 0xe9, 0x20, 0x61, 0x75, 0x20, 0x6c, 0x61, 0x69, 0x74])))).toEqual({ kind: "other" });
  expect(preview(dir)).toEqual({ kind: "other" });
  expect(() => preview(path.join(dir, "missing"))).toThrow();
  expect(looksLikeText(new TextEncoder().encode("plain"))).toBe(true);
  expect(looksLikeText(new Uint8Array([104, 0, 105]))).toBe(false);
});

test("text is fenced so that nothing in it ends the fence", () => {
  expect(previewMarkdown({ kind: "text", text: "a ```` b", truncated: false }, "x")).toBe("`````\na ```` b\n`````");
  expect(previewMarkdown({ kind: "text", text: "plain", truncated: true }, "x")).toBe("```\nplain\n```\n\n*Only the beginning of the file is shown.*");
  expect(previewMarkdown({ kind: "image", url: "data:image/png;base64,AA" }, "a [b].png")).toBe("![a b.png](data:image/png;base64,AA)");
});

test("a path may start at home or be a file address", () => {
  expect(resolvePath("~/Downloads/a.dmg")).toBe(path.join(os.homedir(), "Downloads/a.dmg"));
  expect(resolvePath("file:///tmp/a%20b.txt")).toBe("/tmp/a b.txt");
  expect(resolvePath(new URL("file:///tmp/c.txt"))).toBe("/tmp/c.txt");
  expect(resolvePath("/tmp/d.txt")).toBe("/tmp/d.txt");
});

test("a folder is listed folders first, by name, numbers in order", () => {
  const folder = fs.mkdtempSync(path.join(dir, "listing-"));
  for (const name of ["b10.txt", "b9.txt", "A.txt", ".secret"]) fs.writeFileSync(path.join(folder, name), "");
  for (const name of ["zeta", "Alpha", ".git"]) fs.mkdirSync(path.join(folder, name));
  fs.symlinkSync(path.join(folder, "zeta"), path.join(folder, "link"));
  fs.symlinkSync(path.join(folder, "gone"), path.join(folder, "broken"));

  const names = (options: { hidden: boolean; files: boolean }) => listDirectory(folder, options).map((e) => (e.directory ? `${e.name}/` : e.name));
  expect(names({ hidden: false, files: true })).toEqual(["Alpha/", "link/", "zeta/", "A.txt", "b9.txt", "b10.txt", "broken"]);
  expect(names({ hidden: true, files: true })).toEqual([".git/", "Alpha/", "link/", "zeta/", ".secret", "A.txt", "b9.txt", "b10.txt", "broken"]);
  expect(names({ hidden: false, files: false })).toEqual(["Alpha/", "link/", "zeta/"]);
  expect(listDirectory(path.join(folder, "nowhere"), { hidden: true, files: true })).toEqual([]);
});

test("browsing starts beside what is chosen, or at home", () => {
  const chosen = file("chosen.txt", "");
  expect(startDirectory([path.join(dir, "gone.txt"), chosen])).toBe(dir);
  expect(startDirectory([])).toBe(os.homedir());
  expect(toggled(["a"], "b")).toEqual(["a", "b"]);
  expect(toggled(["a", "b"], "a")).toEqual(["b"]);
});
