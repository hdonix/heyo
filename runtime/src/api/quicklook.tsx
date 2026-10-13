//! Quick Look: `Action.ToggleQuickLook` shows the file an item names in its
//! `quickLook` prop. A picture is shown, text is shown; anything else is
//! opened with the application the desktop has for it.
//!
//! The runtime reads the file and sends its content, because the webview
//! may only load files from a few places (the asset scope), and a file to
//! look at can be anywhere.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { useState } from "react";

import { Action, ActionPanel, Detail } from "./components";
import { Icon } from "./icon";
import { closeMainWindow, open, showToast, Toast } from "./system";
import { locale, t } from "../../../shared/i18n";

const IMAGES: Record<string, string> = {
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".bmp": "image/bmp",
  ".ico": "image/x-icon",
  ".avif": "image/avif",
  ".svg": "image/svg+xml",
};

/// A picture travels whole, as text, through two processes: beyond this it
/// is better looked at in an image viewer.
const IMAGE_LIMIT = 6 * 1024 * 1024;
/// How much of a text file is shown.
const TEXT_LIMIT = 64 * 1024;

export type Preview = { kind: "image"; url: string } | { kind: "text"; text: string; truncated: boolean } | { kind: "other" };

/// Text has no NUL bytes and decodes as UTF-8, apart from a character the
/// cut may have split at the end.
export function looksLikeText(bytes: Uint8Array): boolean {
  if (bytes.includes(0)) return false;
  const text = new TextDecoder("utf-8").decode(bytes);
  const broken = text.indexOf("�");
  return broken === -1 || broken >= text.length - 3;
}

/// The `quickLook.path` an extension gave, as a path: it may start at home
/// (`~/…`) or be a `file:` address.
export function resolvePath(given: unknown): string {
  const text = given instanceof URL ? given.href : Buffer.isBuffer(given) ? given.toString() : String(given ?? "");
  if (text.startsWith("file:")) return fileURLToPath(text);
  if (text === "~" || text.startsWith("~/")) return path.join(os.homedir(), text.slice(1));
  return path.resolve(text);
}

/// How `file` can be looked at. Throws when it cannot be read.
export function preview(file: string): Preview {
  const stat = fs.statSync(file);
  if (!stat.isFile()) return { kind: "other" };
  const type = IMAGES[path.extname(file).toLowerCase()];
  if (type) {
    if (stat.size > IMAGE_LIMIT) return { kind: "other" };
    return { kind: "image", url: `data:${type};base64,${fs.readFileSync(file).toString("base64")}` };
  }
  const handle = fs.openSync(file, "r");
  try {
    const buffer = Buffer.alloc(Math.min(stat.size, TEXT_LIMIT));
    const read = fs.readSync(handle, buffer, 0, buffer.length, 0);
    const bytes = buffer.subarray(0, read);
    if (!looksLikeText(bytes)) return { kind: "other" };
    return { kind: "text", text: bytes.toString("utf8").replace(/�+$/, ""), truncated: stat.size > read };
  } finally {
    fs.closeSync(handle);
  }
}

function size(bytes: number) {
  const units = ["", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return unit ? `${value.toFixed(1)} ${units[unit]}` : t("{0} bytes", value);
}

/// The markdown that shows a preview.
export function previewMarkdown(shown: Exclude<Preview, { kind: "other" }>, name: string): string {
  if (shown.kind === "image") return `![${name.replace(/[[\]]/g, "")}](${shown.url})`;
  // A fence longer than any run of backticks in the text cannot be closed by it.
  const fence = "`".repeat(Math.max(3, ...(shown.text.match(/`+/g) ?? []).map((run) => run.length + 1)));
  return `${fence}\n${shown.text}\n${fence}${shown.truncated ? `\n\n*${t("Only the beginning of the file is shown.")}*` : ""}`;
}

/// What `Action.ToggleQuickLook` uses when it is given no shortcut: Raycast's.
export const QUICK_LOOK_SHORTCUT = { modifiers: ["cmd"], key: "y" };

function QuickLook(props: { file: string; name: string; shown: Exclude<Preview, { kind: "other" }>; shortcut: unknown; onClose(): void }) {
  const [stat] = useState(() => fs.statSync(props.file));
  return (
    <Detail
      navigationTitle={props.name}
      markdown={previewMarkdown(props.shown, props.name)}
      metadata={
        <Detail.Metadata>
          <Detail.Metadata.Label title={t("Name")} text={props.name} />
          <Detail.Metadata.Label title={t("Where")} text={path.dirname(props.file)} />
          <Detail.Metadata.Label title={t("Size")} text={size(stat.size)} />
          <Detail.Metadata.Label title={t("Modified")} text={stat.mtime.toLocaleString(locale())} />
        </Detail.Metadata>
      }
      actions={
        <ActionPanel>
          {/* The same key that opened it closes it: a toggle. */}
          <Action title={t("Close Quick Look")} icon={Icon.Eye} shortcut={props.shortcut} onAction={props.onClose} />
          <Action.Open target={props.file} />
          <Action.ShowInFinder path={props.file} />
          <Action.CopyToClipboard title={t("Copy Path")} content={props.file} />
        </ActionPanel>
      }
    />
  );
}

/// Shows the file of the selected item (the webview says which).
export async function quickLook(
  target: { path?: unknown; name?: string | null } | undefined,
  navigation: { push(element: React.ReactNode): void; pop(): void },
  shortcut: unknown,
) {
  if (!target?.path) {
    await showToast({ style: Toast.Style.Failure, title: t("Nothing to preview"), message: t("This item has no file to look at.") });
    return;
  }
  const file = resolvePath(target.path);
  let shown: Preview;
  try {
    shown = preview(file);
  } catch (e) {
    await showToast({ style: Toast.Style.Failure, title: t("Could not preview the file"), message: e instanceof Error ? e.message : String(e) });
    return;
  }
  if (shown.kind === "other") {
    await open(file);
    await closeMainWindow();
    return;
  }
  navigation.push(<QuickLook file={file} name={target.name || path.basename(file)} shown={shown} shortcut={shortcut} onClose={navigation.pop} />);
}
