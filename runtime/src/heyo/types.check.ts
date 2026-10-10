// Holds the types extension authors see (`packages/api/index.d.ts`) to what
// the runtime provides. Compiled with the runtime (`tsc`), never run: where
// the two part, this file does not compile.
//
// Of Raycast's part only what Heyo adds to it is compared here; how much of
// `@raycast/api` itself is provided is `../api/conformance.test.ts`'s to say.

import type * as declared from "../../../packages/api";
import type * as raycastDeclared from "@raycast/api";
import type raycast from "../api";
import api from "./index";

type Declared = typeof declared;
type Provided = typeof api;

/// `true` only where both have the same members by name.
type SameNames<A, B> = [keyof A] extends [keyof B] ? ([keyof B] extends [keyof A] ? true : never) : never;

/// What each adds to Raycast's API, by name.
type DeclaredOwn = Exclude<keyof Declared, keyof typeof raycastDeclared>;
type ProvidedOwn = Exclude<keyof Provided, keyof typeof raycast>;

// Every name Heyo adds is declared, and nothing is declared that is not there.
export const sameExports: [DeclaredOwn] extends [ProvidedOwn] ? ([ProvidedOwn] extends [DeclaredOwn] ? true : never) : never = true;

// What is provided under those names is what is declared.
export const own: Pick<Declared, DeclaredOwn> = api;

// Module by module, the same functions: none undeclared, none declared and missing.
export const sameFunctions: [
  SameNames<Declared["Windows"], Provided["Windows"]>,
  SameNames<Declared["Screen"], Provided["Screen"]>,
  SameNames<Declared["Dictation"], Provided["Dictation"]>,
  SameNames<Declared["Memory"], Provided["Memory"]>,
  SameNames<Declared["ClipboardHistory"], Provided["ClipboardHistory"]>,
  SameNames<Declared["Files"], Provided["Files"]>,
  SameNames<Declared["System"], Provided["System"]>,
  SameNames<Declared["LiveActivity"], Provided["LiveActivity"]>,
] = [true, true, true, true, true, true, true, true];

// What Raycast's own modules gain.
type Added<Module, Theirs> = Pick<Module, Exclude<keyof Module, keyof Theirs>>;
type HeyoEnvironment = Added<Declared["environment"], raycastDeclared.Environment>;
export const environment: HeyoEnvironment = api.environment;
export const sameEnvironment: SameNames<HeyoEnvironment, Added<Provided["environment"], typeof raycast.environment>> = true;

// Raycast's `AI` is the same object in the runtime, so what Heyo adds to it
// is counted against Raycast's own declarations; `Creativity` is a value
// here where Raycast declares only a type, and not an addition.
type RaycastAi = typeof raycastDeclared.AI & { Creativity: unknown };
export const ai: Pick<Declared["AI"], "provider" | "ask"> = api.AI;
export const sameAi: SameNames<Added<Declared["AI"], RaycastAi>, Added<Provided["AI"], RaycastAi>> = true;

/// The options of `Clipboard.paste`, which is where it differs from Raycast's.
type PasteOptions<Paste> = Paste extends (content: never, options?: infer Options) => unknown ? Options : never;
export const pasteOptions: PasteOptions<Provided["Clipboard"]["paste"]> = {} as NonNullable<PasteOptions<Declared["Clipboard"]["paste"]>>;
export const declaredPasteOptions: PasteOptions<Declared["Clipboard"]["paste"]> = {} as NonNullable<PasteOptions<Provided["Clipboard"]["paste"]>>;

// The options an extension may pass are ones the runtime takes.
export const askOptions: NonNullable<Parameters<Provided["AI"]["ask"]>[1]> = {} as declared.AI.AskOptions;
export const listenOptions: NonNullable<Parameters<Provided["Dictation"]["listen"]>[0]> = {} as declared.ListenOptions;
export const memoryQuery: NonNullable<Parameters<Provided["Memory"]["search"]>[0]> = {} as declared.MemoryQuery;
export const activityAttributes: Parameters<Provided["LiveActivity"]["start"]>[0] = {} as declared.LiveActivity.Attributes;
export const activityContent: Parameters<Provided["LiveActivity"]["start"]>[1] = {} as declared.LiveActivity.Content;
export const activityEvent: Parameters<Parameters<Provided["LiveActivity"]["onEvent"]>[0]>[0] = {} as declared.LiveActivity.Event;
// And back: what the runtime gives a listener is what is declared.
export const declaredEvent: declared.LiveActivity.Event = {} as Parameters<Parameters<Provided["LiveActivity"]["onEvent"]>[0]>[0];
