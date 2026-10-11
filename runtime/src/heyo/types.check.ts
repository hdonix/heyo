// Holds the types extension authors see (`packages/api/index.d.ts`) to what
// the runtime provides. Compiled with the runtime (`tsc`), never run: where
// the two part, this file does not compile.
//
// Of Raycast's part only what Heyo adds to it is compared here; how much of
// `@raycast/api` itself is provided is `../api/conformance.test.ts`'s to say.

import type * as declared from "../../../packages/api";
import type * as raycastDeclared from "@raycast/api";
import type { needs, permissions } from "../../../shared/bindings/needs";
import type { Permission } from "../../../shared/bindings/Permission";
import type { ExtMethod } from "../../../shared/wire";
import type raycast from "../api";
import api from "./index";
import type { modules } from "./members";

type Declared = typeof declared;
type Provided = typeof api;

/// `true` only where both have the same members by name.
type SameNames<A, B> = [keyof A] extends [keyof B] ? ([keyof B] extends [keyof A] ? true : false) : false;

/// What each adds to Raycast's API, by name.
type DeclaredOwn = Exclude<keyof Declared, keyof typeof raycastDeclared>;
type ProvidedOwn = Exclude<keyof Provided, keyof typeof raycast>;

// Every name Heyo adds is declared, and nothing is declared that is not there.
export const sameExports: [DeclaredOwn] extends [ProvidedOwn] ? ([ProvidedOwn] extends [DeclaredOwn] ? true : never) : never = true;

// What is provided under those names is what is declared.
export const own: Pick<Declared, DeclaredOwn> = api;

// Module by module, the functions that are declared (`members.ts`), which
// those provided are made from, are the ones the package declares: none
// undeclared, none declared and missing.
type Modules = typeof modules;
type Differing = {
  [M in keyof Modules]: M extends keyof Declared & keyof Provided ? ([SameNames<Declared[M], Modules[M]>, SameNames<Provided[M], Modules[M]>] extends [true, true] ? never : M) : M;
}[keyof Modules];
export const sameFunctions: [Differing] extends [never] ? true : never = true;

// What the host wrote of its requests (`shared/bindings/needs.ts`) is of
// the requests and the permissions there are now.
export const everyRequest: SameNames<typeof needs, Record<ExtMethod, unknown>> = true;
export const everyPermission: [Permission] extends [(typeof permissions)[number]] ? true : never = true;

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
