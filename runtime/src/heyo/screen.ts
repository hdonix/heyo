//! `Screen`: pictures of the screen and the text in them (permissions
//! `screen.capture` and `screen.text`).

import fs from "node:fs";

import type { Capture } from "../../../shared/bindings/Capture";
import type { CaptureTarget } from "../../../shared/bindings/CaptureTarget";
import type { RecognizedText } from "../../../shared/bindings/RecognizedText";
import { current, whenEnded } from "../context";
import { ask, implement } from "./asking";
import { modules } from "./members";

/// `"selection"` has the user select an area (and fails with `cancelled`
/// if they give up); the other targets are taken as they are on screen.
/// The file is the command's for as long as it runs.
async function capture(target: CaptureTarget): Promise<Capture> {
  const command = current.value;
  const capture = await ask("ext.screen.capture", { target });
  // It shows the user's screen: not kept once the command is over.
  if (command) whenEnded(command, () => fs.rm(capture.path, { force: true }, () => {}));
  return capture;
}

/// The text in an image file, or in what `capture` would take.
async function recognizeText(image: string | CaptureTarget): Promise<RecognizedText> {
  const path = typeof image === "string" ? image : (await capture(image)).path;
  return ask("ext.screen.recognizeText", { path });
}

export const Screen = implement(modules.Screen, { capture, recognizeText });
