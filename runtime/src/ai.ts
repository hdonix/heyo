//! The model behind `AI.ask` (`heyo/ai.ts`), and so behind Heyo's own AI
//! commands: the one the settings name, asked by the host (`host.ai.ask`,
//! `src-tauri/src/ai.rs`). The runtime has neither the key nor the address
//! of any model; it sends the question and hears the answer as it is
//! written.

import type { Question } from "../../shared/bindings/Question";
import { requestOverTime } from "./protocol";

export type { PromptImage } from "../../shared/bindings/PromptImage";
export type { Turn } from "../../shared/bindings/Turn";

export interface Asking extends Question {
  /// The answer so far, as it is written.
  onText?(text: string): void;
  /// Calls the question off; it then fails with the signal's reason.
  signal?: AbortSignal;
}

/// The answer of the model the settings name. Fails with what went wrong,
/// in words for the user.
export function askModel({ onText, signal, ...question }: Asking): Promise<string> {
  let text = "";
  const progress = (data: unknown) => {
    const told = data as { more?: string; text?: string };
    text = told.text ?? text + (told.more ?? "");
    onText?.(text);
  };
  return requestOverTime<string>(["host.ai.ask", question], progress, signal);
}
