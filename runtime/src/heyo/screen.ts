//! `Screen`: pictures of the screen and the text in them (permissions
//! `screen.capture` and `screen.text`).

import fs from "node:fs";

import type { Capture } from "../../../shared/bindings/Capture";
import type { CaptureTarget } from "../../../shared/bindings/CaptureTarget";
import type { RecognizedText } from "../../../shared/bindings/RecognizedText";
import type { Support } from "../../../shared/bindings/Support";
import { current, whenEnded } from "../context";
import { ask } from "../protocol";

export type { Capture, CaptureTarget, RecognizedText };

export const Screen = {
  supported: () => ask<Support>("ext.supported", { module: "screen" }),

  /// `"selection"` has the user select an area (and fails with `cancelled`
  /// if they give up); the other targets are taken as they are on screen.
  /// The file is the command's for as long as it runs.
  async capture(target: CaptureTarget): Promise<Capture> {
    const command = current.value;
    const capture = await ask<Capture>("ext.screen.capture", { target });
    // It shows the user's screen: not kept once the command is over.
    if (command) whenEnded(command, () => fs.rm(capture.path, { force: true }, () => {}));
    return capture;
  },

  /// The text in an image file, or in what `capture` would take.
  async recognizeText(image: string | CaptureTarget): Promise<RecognizedText> {
    const path = typeof image === "string" ? image : (await Screen.capture(image)).path;
    return ask<RecognizedText>("ext.screen.recognizeText", { path });
  },
};
