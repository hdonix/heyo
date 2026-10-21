//! Claude, through Anthropic's SDK: one of the two adapters of `model.ts`.

import Anthropic from "@anthropic-ai/sdk";

import { type Model, ModelError, type Question } from "./model";
import { t } from "../../shared/i18n";

/// The Claude model that answers when Claude is the provider.
export const CLAUDE_MODEL = "claude-opus-5-5";

/// Claude, with the key from the settings (empty: the SDK finds
/// ANTHROPIC_API_KEY or an `ant auth login` profile).
export function claude(apiKey: string): Model {
  return {
    provider: "claude",
    sees: { images: true },
    answer: (question) => answer(apiKey, question),
  };
}

async function answer(apiKey: string, question: Question): Promise<string> {
  const prompt = question.history ? question.prompt : `<text>\n${question.prompt}\n</text>`;
  const pictures = (question.images ?? []).map((image) => ({
    type: "image" as const,
    source: { type: "base64" as const, media_type: image.mediaType, data: image.data },
  }));
  const client = apiKey ? new Anthropic({ apiKey }) : new Anthropic();
  const stream = client.beta.messages.stream(
    {
      model: CLAUDE_MODEL,
      max_tokens: 64000,
      // Heyo asks for short rewriting and explaining; answer quickly.
      output_config: { effort: "low" },
      // If a safety classifier declines, retry on the recommended fallback model.
      betas: ["server-side-fallback-2026-07-01"],
      fallbacks: "default",
      system: question.system,
      // Text to work on is fenced, so that it reads as material and not as a
      // message; a conversation is sent as it is.
      messages: [...(question.history ?? []), { role: "user", content: pictures.length ? [...pictures, { type: "text", text: prompt }] : prompt }],
    } as Anthropic.Beta.MessageCreateParamsStreaming,
    { signal: question.signal },
  );
  let text = "";
  stream.on("text", (delta) => {
    text += delta;
    question.onText?.(text);
  });
  const message = await stream.finalMessage().catch((error: unknown) => {
    // The SDK's own words for this ("Could not resolve authentication
    // method…") tell a user nothing about what to do.
    if (!apiKey && /authentication method|api[_ ]?key/i.test(String(error))) {
      throw new ModelError(t("AI is not set up. Add an Anthropic API key, or choose a local model, in Heyo Settings → Advanced."));
    }
    throw error;
  });
  if (message.stop_reason === "refusal") throw new ModelError(t("Claude declined this request."));
  if (!text.trim()) throw new ModelError(t("Claude returned no text."));
  return text.trim();
}

/// Why a request to Claude failed, in words for the user; undefined for an
/// error that is not the SDK's.
export function describeClaudeError(error: unknown): string | undefined {
  if (error instanceof Anthropic.AuthenticationError) return t("The Claude API key is invalid. Check it in Heyo Settings → Advanced.");
  if (error instanceof Anthropic.RateLimitError) return t("Claude's rate limit was reached. Try again shortly.");
  if (error instanceof Anthropic.APIConnectionError) return t("Could not reach the Claude API.");
  if (error instanceof Anthropic.APIError) return t("Claude API error {0}: {1}", error.status, error.message);
  // The SDK's own (non-HTTP) errors here mean no credentials could be resolved.
  if (error instanceof Anthropic.AnthropicError) return t("No Claude API key is set. Add one in Heyo Settings → Advanced.");
  return undefined;
}
