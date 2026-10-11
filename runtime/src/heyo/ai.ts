//! `AI`: Raycast's `ask`, answered by the model set up in Heyo, and what it
//! gains (permission `ai`): which model answers, a system prompt, the turns
//! of a conversation so far, pictures to go with the question, and the
//! answer as it grows. Heyo's own Ask AI, AI Chat and Selected Text Actions
//! ask through this like any extension.

import fs from "node:fs";
import path from "node:path";

import type { AiProvider } from "../../../shared/bindings/AiProvider";
import { CLAUDE_MODEL, describeClaudeError, type PromptImage, streamClaude } from "../ai";
import { ask, implement } from "./asking";
import { HeyoError } from "./error";
import { gained } from "./members";

/// An earlier question or answer of a conversation.
export interface Message {
  role: "user" | "assistant";
  content: string;
}

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
  /// The conversation so far, oldest first; the prompt is the next message.
  messages?: Message[];
  /// The prompt is text to work on (translate, rewrite, …), not a message
  /// to answer: it is fenced so that the model treats it as material and
  /// follows `system` rather than anything the text says. Not with `messages`.
  material?: boolean;
  /// Called with the whole answer so far, as it grows.
  onText?: (text: string) => void;
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

/// The turns of a conversation as the model is given them: alternating,
/// which some models insist on, so a turn out of order is a wrong call.
export function turns(messages: Message[]): Message[] {
  messages.forEach((message, i) => {
    if (message.role !== (i % 2 ? "assistant" : "user") || typeof message.content !== "string") {
      throw new HeyoError("invalid", "messages must alternate, a user's then the assistant's, each with its content.");
    }
  });
  if (messages.length % 2) throw new HeyoError("invalid", "messages must end with the assistant's answer; the prompt is the user's next message.");
  return messages.map(({ role, content }) => ({ role, content }));
}

/// Why asking failed, as a `HeyoError`: withdrawn, or failed with the
/// reason in words for the user.
function failure(error: unknown, signal?: AbortSignal): HeyoError {
  if (error instanceof HeyoError) return error;
  if (signal?.aborted) return new HeyoError("cancelled", "The question was withdrawn.");
  return new HeyoError("failed", describeClaudeError(error));
}

/// Which model answers, with Claude's name filled in.
async function provider(): Promise<AiProvider> {
  const set = await ask("ext.ai.provider");
  return set.kind === "claude" ? { ...set, model: CLAUDE_MODEL } : set;
}

const added = implement(gained.AI, {
  /// Which model answers and whether what is asked stays on this computer,
  /// for deciding what to send it.
  provider,

  /// As Raycast's: a promise of the answer that also emits it piece by
  /// piece (`on("data", …)`). Fails with a `HeyoError`: "cancelled" when
  /// `signal` aborts, "failed" with the reason otherwise.
  ask(prompt: string, options: AskOptions = {}) {
    const { system, images, messages, material, creativity, signal, onText } = options;
    const listeners: ((chunk: string) => void)[] = [];
    let seen = "";
    const answer = (async () => {
      if (material && messages) throw new HeyoError("invalid", "material is text to work on, which is not a conversation: not with messages.");
      const history = material ? undefined : messages ? turns(messages) : [];
      let pictures: PromptImage[] | undefined;
      if (system !== undefined || images?.length || messages || material) {
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
        history,
        images: pictures,
        temperature: temperatureOf(creativity),
        signal,
        onText: (text) => {
          onText?.(text);
          const chunk = text.slice(seen.length);
          seen = text;
          if (chunk) listeners.forEach((listener) => listener(chunk));
        },
      }).catch((error) => {
        throw failure(error, signal);
      });
    })();
    return Object.assign(answer, {
      on(event: string, listener: (chunk: string) => void) {
        if (event === "data") listeners.push(listener);
        return this;
      },
    });
  },
});

export const AI = {
  /// Any model named is answered by the one chosen in Heyo's settings.
  Model: new Proxy({}, { get: (_, name) => String(name) }) as Record<string, string>,
  Creativity: { None: "none", Low: "low", Medium: "medium", High: "high", Maximum: "maximum" },
  ...added,
};
