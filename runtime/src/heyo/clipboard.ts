//! What `Clipboard.paste` gains (permission `input.paste`).

import { Clipboard as raycastClipboard, clipboardPayload, closeMainWindow, type ClipboardContent } from "../api/system";
import { ask, implement } from "./asking";
import { gained } from "./members";

/// As Raycast's, and with `restoreClipboard` the text that was on the
/// clipboard is put back once the paste is done.
async function paste(content: ClipboardContent, options?: { restoreClipboard?: boolean }): Promise<void> {
  if (!options?.restoreClipboard) return raycastClipboard.paste(content);
  await closeMainWindow();
  await ask("ext.clipboard.paste", clipboardPayload(content));
}

export const Clipboard = { ...raycastClipboard, ...implement(gained.Clipboard, { paste }) };
