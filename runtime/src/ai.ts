//! The model behind `AI.ask` (`heyo/ai.ts`), and so behind Heyo's own AI
//! commands: Claude, or one running locally.

import Anthropic from "@anthropic-ai/sdk";

import { type AiSettings, LocalModelError, streamLocal } from "./llm";
import { request } from "./protocol";
import { t } from "../../shared/i18n";

/// Why a request to the model failed, in words for the user.
export function describeClaudeError(error: unknown) {
  if (error instanceof LocalModelError) return error.message;
  if (error instanceof Anthropic.AuthenticationError) return t("The Claude API key is invalid. Check it in Heyo Settings → Advanced.");
  if (error instanceof Anthropic.RateLimitError) return t("Claude's rate limit was reached. Try again shortly.");
  if (error instanceof Anthropic.APIConnectionError) return t("Could not reach the Claude API.");
  if (error instanceof Anthropic.APIError) return t("Claude API error {0}: {1}", error.status, error.message);
  // The SDK's own (non-HTTP) errors here mean no credentials could be resolved.
  if (error instanceof Anthropic.AnthropicError) return t("No Claude API key is set. Add one in Heyo Settings → Advanced.");
  return error instanceof Error ? error.message : String(error);
}

/// An earlier question or answer of a conversation.
export interface Turn {
  role: "user" | "assistant";
  content: string;
}

/// The Claude model that answers when Claude is the provider.
export const CLAUDE_MODEL = "claude-opus-5-5";

/// A picture to go with a question.
export interface PromptImage {
  mediaType: "image/png" | "image/jpeg" | "image/gif" | "image/webp";
  /// Base64.
  data: string;
}

/// Streams the model's answer, calling `onText` with the text so far.
/// Without `history` the prompt is text to work on (translate, rewrite, …);
/// with it, the prompt is the user's next message in a conversation.
/// `images` go with the prompt, to Claude only; `temperature` is for the
/// local model only (Claude is asked without one).
export async function streamClaude(options: {
  system: string;
  prompt: string;
  history?: Turn[];
  images?: PromptImage[];
  temperature?: number;
  onText(text: string): void;
  signal?: AbortSignal;
}): Promise<string> {
  const settings = await request<AiSettings>("host.ai");
  if (settings.provider === "local") {
    if (options.images?.length) throw new LocalModelError("The local model is not given images.");
    return streamLocal({ ...options, settings });
  }
  const prompt = options.history ? options.prompt : `<text>\n${options.prompt}\n</text>`;
  const pictures = (options.images ?? []).map((image) => ({
    type: "image" as const,
    source: { type: "base64" as const, media_type: image.mediaType, data: image.data },
  }));
  const { apiKey } = settings;
  // Without a key from the settings, the SDK resolves ANTHROPIC_API_KEY or an `ant auth login` profile.
  const client = apiKey ? new Anthropic({ apiKey }) : new Anthropic();
  const stream = client.beta.messages.stream(
    {
      model: CLAUDE_MODEL,
      max_tokens: 64000,
      // These are short rewriting and explaining tasks; answer quickly.
      output_config: { effort: "low" },
      // If a safety classifier declines, retry on the recommended fallback model.
      betas: ["server-side-fallback-2026-07-01"],
      fallbacks: "default",
      system: options.system,
      // Text to work on is fenced, so that it reads as material and not as a
      // message; a conversation is sent as it is.
      messages: [...(options.history ?? []), { role: "user", content: pictures.length ? [...pictures, { type: "text", text: prompt }] : prompt }],
    } as Anthropic.Beta.MessageCreateParamsStreaming,
    { signal: options.signal },
  );
  let text = "";
  stream.on("text", (delta) => {
    text += delta;
    options.onText(text);
  });
  const message = await stream.finalMessage().catch((error: unknown) => {
    // The SDK's own words for this ("Could not resolve authentication
    // method…") tell a user nothing about what to do.
    if (!apiKey && /authentication method|api[_ ]?key/i.test(String(error))) {
      throw new Error(t("AI is not set up. Add an Anthropic API key, or choose a local model, in Heyo Settings → Advanced."));
    }
    throw error;
  });
  if (message.stop_reason === "refusal") throw new Error(t("Claude declined this request."));
  return text;
}
