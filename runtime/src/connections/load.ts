//! Finds the connector a command names: one of Heyo's own (`builtins.ts`),
//! or the default export of an extension's built command file, loaded once
//! for the life of the process.

import Module from "node:module";
import path from "node:path";

import { ConnectorError, type Connector } from "../heyo/connector";
import { builtinConnectors } from "./builtins";
import type { Loader } from "./serve";

/// What a connector's file gets for `@heyo/api`: what a connector uses of
/// it. The rest of the API is for commands, which run elsewhere.
const api = { isHeyo: true as const, ConnectorError };

let provided = false;

/// Makes `@heyo/api` (and `@raycast/api`, which libraries ask for by that
/// name) resolve to what connectors are given.
function provide() {
  if (provided) return;
  provided = true;
  const load = (Module as any)._load;
  (Module as any)._load = function (request: string, ...rest: unknown[]) {
    if (request === "@heyo/api" || request === "@raycast/api") return api;
    return load.call(this, request, ...rest);
  };
}

export function loader(builtins: Record<string, Connector> = builtinConnectors): Loader {
  const loaded = new Map<string, Connector>();
  return ({ extension, command }) => {
    if (extension === "@heyo") {
      const builtin = builtins[command];
      if (!builtin) throw new ConnectorError("failed", `Heyo has no connector ${command}.`);
      return builtin;
    }
    const file = path.join(extension, `${command}.js`);
    let connector = loaded.get(file);
    if (!connector) {
      provide();
      const exported = Module.createRequire(file)(file);
      connector = (exported?.default ?? exported) as Connector;
      if (typeof connector?.check !== "function" || typeof connector?.sync !== "function") {
        throw new ConnectorError("failed", `${file} does not export a connector (check and sync).`);
      }
      loaded.set(file, connector);
    }
    return connector;
  };
}
