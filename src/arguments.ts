//! What an item of the root search asks for before it opens, shown as fields
//! in the search bar once the item is selected.
//!
//! A command declares its arguments in its manifest. Quicklinks and snippets
//! with a `{query}` / `{argument}` placeholder ask for one text, and are
//! treated as commands with that one argument: the same fields, the same
//! keys.

import type { Argument } from "../shared/bindings/Argument";
import type { Item } from "../shared/bindings/Item";
import { t } from "../shared/i18n";

export type { Argument };

const PLACEHOLDER = /\{(?:query|argument)((?:\s+\w+="[^"]*")*)\s*\}/g;

/// The one argument of a template (of quicklinks and snippets), if it asks for text.
/// A placeholder with a `default` can be left empty.
function ofTemplate(template: string, placeholder: string): Argument[] {
  const asked = [...template.matchAll(PLACEHOLDER)];
  if (!asked.length) return [];
  const required = asked.some((m) => !/\sdefault="[^"]+"/.test(m[1]));
  return [{ name: "query", type: "text", placeholder, required }];
}

/// The fields an item shows in the search bar; none for most.
export function argumentsOf(item: Item): Argument[] {
  if (item.kind === "Quicklink") return ofTemplate(item.link ?? "", t("Query"));
  if (item.kind === "Snippet") return ofTemplate(item.text ?? "", t("Text"));
  return item.arguments ?? [];
}

/// The text after an item's alias, e.g. "heyo" in "gh heyo".
export function afterAlias(alias: string | undefined, query: string): string | null {
  const q = query.trim();
  const a = alias?.toLowerCase();
  return a && q.toLowerCase().startsWith(`${a} `) ? q.slice(a.length + 1).trim() : null;
}

/// A value for every field: what was typed in it, else for the first field
/// the text after the item's alias, else nothing. A dropdown only ever has
/// one of its choices.
export function filled(fields: Argument[], typed: Record<string, string>, aliasText: string | null): Record<string, string> {
  return Object.fromEntries(
    fields.map((field, i) => {
      const value = typed[field.name] ?? (i === 0 ? aliasText : null) ?? "";
      if (field.type !== "dropdown") return [field.name, value];
      const choice = (field.data ?? []).find((d) => d.value === value || d.title.toLowerCase() === value.toLowerCase());
      return [field.name, choice?.value ?? ""];
    }),
  );
}

/// The required fields still empty, in order: what keeps Enter from launching.
export function missing(fields: Argument[], values: Record<string, string>): string[] {
  return fields.filter((f) => f.required && !values[f.name]?.trim()).map((f) => f.name);
}

/// Where Tab goes from `from` among the search field (0) and the `count`
/// argument fields after it (1…count): round, in either direction.
export function tabStop(from: number, backwards: boolean, count: number): number {
  const stops = count + 1;
  return (from + (backwards ? -1 : 1) + stops) % stops;
}
