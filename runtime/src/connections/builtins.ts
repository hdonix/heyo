//! Heyo's own connectors, by command name: what an account of
//! `@heyo:<name>` is synced by. The host declares them beside its other
//! connectors (`src-tauri/src/connections/builtins.rs`): their fields,
//! presets and what they bring. Each is in `builtin/`.

import type { Connector } from "../heyo/connector";
import { caldav, ics } from "./builtin/calendar";
import { imap } from "./builtin/imap";

export const builtinConnectors: Record<string, Connector> = { imap, caldav, ics };
