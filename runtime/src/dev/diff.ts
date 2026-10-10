//! A unified diff of two texts, for showing what `heyo migrate` changes.

type Edit = { kind: " " | "-" | "+"; line: string };

/// The lines of both texts in order, each kept, removed or added.
function edits(before: string[], after: string[]): Edit[] {
  // What both ends share needs no comparing; the middle is small.
  let head = 0;
  while (head < before.length && head < after.length && before[head] === after[head]) head++;
  let tail = 0;
  while (tail < before.length - head && tail < after.length - head && before[before.length - 1 - tail] === after[after.length - 1 - tail]) tail++;
  const a = before.slice(head, before.length - tail);
  const b = after.slice(head, after.length - tail);

  // Longest common subsequence, by the table.
  const width = b.length + 1;
  const table = new Uint32Array((a.length + 1) * width);
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      table[i * width + j] = a[i] === b[j] ? table[(i + 1) * width + j + 1] + 1 : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  const out: Edit[] = before.slice(0, head).map((line) => ({ kind: " ", line }));
  let i = 0;
  let j = 0;
  while (i < a.length || j < b.length) {
    if (i < a.length && j < b.length && a[i] === b[j]) out.push({ kind: " ", line: a[i++] }), j++;
    else if (j >= b.length || (i < a.length && table[(i + 1) * width + j] >= table[i * width + j + 1])) out.push({ kind: "-", line: a[i++] });
    else out.push({ kind: "+", line: b[j++] });
  }
  for (const line of before.slice(before.length - tail)) out.push({ kind: " ", line });
  return out;
}

/// The diff as `diff -u` writes it, or "" when the texts are the same.
export function unifiedDiff(file: string, before: string, after: string, context = 3): string {
  if (before === after) return "";
  const all = edits(before.split("\n"), after.split("\n"));
  const changed = all.flatMap((edit, index) => (edit.kind === " " ? [] : [index]));
  const out = [`--- a/${file}`, `+++ b/${file}`];
  let at = 0;
  while (at < changed.length) {
    // A hunk takes in every change whose context touches the one before.
    let last = at;
    while (last + 1 < changed.length && changed[last + 1] - changed[last] <= context * 2) last++;
    const start = Math.max(0, changed[at] - context);
    const end = Math.min(all.length, changed[last] + context + 1);
    const lines = all.slice(start, end);
    const oldStart = all.slice(0, start).filter((e) => e.kind !== "+").length;
    const newStart = all.slice(0, start).filter((e) => e.kind !== "-").length;
    const oldCount = lines.filter((e) => e.kind !== "+").length;
    const newCount = lines.filter((e) => e.kind !== "-").length;
    out.push(`@@ -${oldCount ? oldStart + 1 : oldStart},${oldCount} +${newCount ? newStart + 1 : newStart},${newCount} @@`);
    for (const edit of lines) out.push(edit.kind + edit.line);
    at = last + 1;
  }
  return out.join("\n") + "\n";
}
