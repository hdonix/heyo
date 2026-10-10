//! What `AI` gains over Raycast's (permission `ai`): which model answers,
//! a system prompt, and pictures to go with the question.

import fs from "node:fs";
import path from "node:path";

import type { AiProvider } from "../../../shared/bindings/AiProvider";
import { CLAUDE_MODEL, type PromptImage, streamClaude } from "../ai";
import raycast from "../api";
import { ask } from "../protocol";
import { HeyoError } from "./error";

export interface AskOptions {
  /// Accepted for Raycast's sake; the model set up in Heyo answers.
  model?: unknown;
  /// The temperature, for a local model: a name of Raycast's or a number
  /// from 0 to 2. Claude is asked without one.
  creativity?: "none" | "low" | "medium" | "high" | "maximum" | number;
  signal?: AbortSignal;
  /// Instead of the instructions Heyo gives the model for extensions.
  system?: string;
  /// Image files (PNG, JPEG, GIF, WebP) to go with the prompt. Fails with
  /// `unsupported` unless the model can be given images.
  images?: string[];
}

const INSTRUCTIONS = "You are a helpful assistant inside a desktop launcher extension. Follow the request exactly and answer concisely.";

const CREATIVITY = { none: 0, low: 0.5, medium: 1, high: 1.5, maximum: 2 };

/// The temperature a creativity stands for, as Raycast counts it.
export function temperatureOf(creativity: AskOptions["creativity"]): number | undefined {
  if (typeof creativity === "number") return Number.isFinite(creativity) ? Math.min(2, Math.max(0, creativity)) : undefined;
  return creativity === undefined ? undefined : CREATIVITY[creativity];
}

const MEDIA_TYPES: Record<string, PromptImage["mediaType"]> = {
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
};

/// An image file as the model takes it.
export function readImage(file: string): PromptImage {
  const mediaType = MEDIA_TYPES[path.extname(file).toLowerCase()];
  if (!mediaType) throw new HeyoError("invalid", `${file} is not a PNG, JPEG, GIF or WebP image.`);
  try {
    return { mediaType, data: fs.readFileSync(file).toString("base64") };
  } catch (e) {
    throw new HeyoError("invalid", `${file} could not be read: ${e instanceof Error ? e.message : e}`);
  }
}

/// Which model answers, with Claude's name filled in.
async function provider() {
  const set = await ask<AiProvider>("ext.ai.provider");
  return set.kind === "claude" ? { ...set, model: CLAUDE_MODEL } : set;
}

export const AI = {
  ...raycast.AI,

  /// Which model answers and whether what is asked stays on this computer,
  /// for deciding what to send it.
  provider,

  /// As Raycast's: a promise of the answer that also emits it piece by
  /// piece (`on("data", …)`).
  ask(prompt: string, options: AskOptions = {}) {
    const { system, images, creativity, signal } = options;
    // Asked the way Raycast's API asks: answered the way it always was.
    if (system === undefined && !images?.length && creativity === undefined) return raycast.AI.ask(prompt, { signal });

    const listeners: ((chunk: string) => void)[] = [];
    let seen = "";
    const answer = (async () => {
      let pictures: PromptImage[] | undefined;
      if (system !== undefined || images?.length) {
        // What Heyo adds is the extension's to use once its manifest says so.
        const model = await provider();
        if (images?.length) {
          if (model.kind !== "claude") throw new HeyoError("unsupported", "The local model set up in Heyo is not given images.");
          pictures = images.map(readImage);
        }
      }
      return streamClaude({
        system: system ?? INSTRUCTIONS,
        prompt,
        history: [],
        images: pictures,
        temperature: temperatureOf(creativity),
        signal,
        onText: (text) => {
          const chunk = text.slice(seen.length);
          seen = text;
          if (chunk) listeners.forEach((listener) => listener(chunk));
        },
      });
    })();
    return Object.assign(answer, {
      on(event: string, listener: (chunk: string) => void) {
        if (event === "data") listeners.push(listener);
        return this;
      },
    });
  },
};
