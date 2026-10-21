//! A model that answers: what Heyo's AI features ask of one, whichever
//! answers. Claude (`claude.ts`) and any model behind an OpenAI-compatible
//! server (`openai.ts`) are its two adapters; `models.ts` picks one from the
//! settings. Callers know this interface and nothing of either adapter.

/// Which model answers, from the settings (the host's `Settings::ai`).
export interface AiSettings {
  provider: "claude" | "local";
  /// For Claude; empty lets the SDK find ANTHROPIC_API_KEY or an `ant auth login` profile.
  apiKey: string;
  /// Base URL of the local server, e.g. http://127.0.0.1:8000/v1.
  localUrl: string;
  /// Model to ask for; empty means the first one the server lists.
  localModel: string;
}

/// An earlier question or answer of a conversation.
export interface Turn {
  role: "user" | "assistant";
  content: string;
}

/// A picture to go with a question.
export interface PromptImage {
  mediaType: "image/png" | "image/jpeg" | "image/gif" | "image/webp";
  /// Base64.
  data: string;
}

/// What is asked. Without `history` the prompt is text to work on
/// (translate, rewrite, clean up…), and it is given as material, never as
/// a request to the model; with `history` it is the user's next message in
/// a conversation.
export interface Question {
  system: string;
  prompt: string;
  history?: Turn[];
  /// Only for a model that `sees` them; asking another with images fails.
  images?: PromptImage[];
  /// How freely to answer, 0 to 1. A hint: a model that takes none ignores it.
  temperature?: number;
  /// The answer so far, as it is written.
  onText?(text: string): void;
  signal?: AbortSignal;
}

export interface Model {
  readonly provider: AiSettings["provider"];
  /// What it can be given besides text.
  readonly sees: { images: boolean };
  /// The whole answer, trimmed of anything the model says before it.
  /// Fails with a `ModelError` worded for the user, or with the adapter's
  /// own error (`describeModelError` words that).
  answer(question: Question): Promise<string>;
}

/// A failure already worded for the user.
export class ModelError extends Error {}
