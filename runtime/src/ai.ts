//! The model behind `AI.ask` (`heyo/ai.ts`), and so behind Heyo's own AI
//! commands: the one the settings name, asked through `model.ts`.

import type { Question } from "./model";
import { modelFrom } from "./models";
import { request } from "./protocol";
import type { AiSettings } from "./model";

export { CLAUDE_MODEL } from "./claude";
export type { PromptImage, Turn } from "./model";
export { describeModelError } from "./models";

/// The answer of the model the settings name.
export async function askModel(question: Question): Promise<string> {
  return modelFrom(await request<AiSettings>("host.ai")).answer(question);
}
