import { expect, test } from "vitest";

import { checkSource } from "./check";

test("a minified bundle, one line of megabytes, is read in the time its size takes", () => {
  const line = (strings: number) => '"use strict";\nvar a=[' + '"some text here",'.repeat(strings) + "0];";
  const time = (strings: number) => {
    const started = performance.now();
    checkSource("x.js", line(strings));
    return performance.now() - started;
  };
  time(1000);
  const small = time(4000);
  const large = time(16000);
  // Four times the text: four times the work, not sixteen. (Generous, for a busy machine.)
  expect(large).toBeLessThan(Math.max(small * 10, 200));
});

test("a macOS program is still found by the call around its name", () => {
  const findings = checkSource("x.js", 'const { execSync } = require("child_process");\nexecSync(\n  "pbcopy"\n);\n');
  expect(findings.map((f) => f.rule)).toContain("mac-program");
});
