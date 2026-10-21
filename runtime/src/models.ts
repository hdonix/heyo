//! The model the settings name, as a `Model`, and a failure of any model in
//! words for the user. Every caller gets its model here.

import { claude, describeClaudeError } from "./claude";
import type { AiSettings, Model } from "./model";
import { ModelError } from "./model";
import { openAiCompatible } from "./openai";

export function modelFrom(settings: AiSettings): Model {
  return settings.provider === "local" ? openAiCompatible(settings.localUrl, settings.localModel) : claude(settings.apiKey);
}

/// Why asking a model failed, in words for the user.
export function describeModelError(error: unknown): string {
  if (error instanceof ModelError) return error.message;
  return describeClaudeError(error) ?? (error instanceof Error ? error.message : String(error));
}
