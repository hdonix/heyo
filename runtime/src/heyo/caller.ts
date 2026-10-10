//! Whose a request to the host is: every `ext.*` request names the
//! extension of the command that makes it (`ExtCaller` in wire.rs).

import type { ExtCaller } from "../../../shared/bindings/ExtCaller";
import type { Route } from "../../../shared/bindings/Route";
import type { Current } from "../context";

/// What a request made by `command` carries beside its method and
/// parameters. Nothing here comes from what an extension passes to the API:
/// the name is the one in the manifest the command was launched from.
export function envelope(command: Current | null, via?: Route): ExtCaller | undefined {
  if (!command) return undefined;
  const { dir, manifest } = command.extension;
  return { extension: manifest.name, dir, ...(via ? { via } : {}) };
}
