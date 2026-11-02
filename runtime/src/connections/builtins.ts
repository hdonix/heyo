//! Heyo's own connectors, by command name: what an account of
//! `@heyo:<name>` is synced by. The host declares them beside its other
//! connectors (`connections::accounts::kinds`); there are none yet.

import type { Connector } from "../heyo/connector";

export const builtinConnectors: Record<string, Connector> = {};
