//! The runtime as the host knows it: what extensions are handed when they
//! load, and what each of the host's messages does. `main.ts` starts it on
//! the process's pipes; a test starts it with itself in the host's place.

import { setAppearance } from "./api";
import { open } from "./api/system";
import { startBackground } from "./background";
import { builtinExtension } from "./builtins";
import { readManifest, runAs, scopedReact } from "./context";
import { refresh as refreshDevelopment } from "./dev/console";
import api from "./heyo";
import { connect, on, send } from "./protocol";
import { dispatch, nodeById, publicInstanceHooks, type HostNode } from "./renderer";
import { activeSession, close, launch, provide, relaunch, reportError } from "./session";
import type { Transport } from "./transport";

/// Which of the two processes Heyo runs extensions in this one is.
export interface Identity {
  /// The one for menu bar and background commands; otherwise the palette's.
  background: boolean;
}

/// Starts the runtime in conversation with `host`, and says it is ready.
/// Resolves when the host hangs up.
export function start(host: Transport, identity: Identity): Promise<void> {
  // Sharing our React instance is required for hooks to work. The two API
  // names are one object: an extension written for Heyo and the libraries it
  // uses that were written for Raycast share navigation, toasts and caches.
  provide({
    "@raycast/api": api,
    "@heyo/api": api,
    react: scopedReact,
    "react/jsx-runtime": require("react/jsx-runtime"),
    "react/jsx-dev-runtime": require("react/jsx-runtime"),
  });

  // `ref` on a form field gives imperative focus()/reset().
  publicInstanceHooks.create = (node: HostNode) => ({
    focus: () => send({ type: "field", node: node.id, op: "focus" }),
    reset: () => send({ type: "field", node: node.id, op: "reset" }),
  });

  on("launch", (message) => {
    try {
      const extension = message.extension === "@heyo" ? builtinExtension : { dir: message.extension, manifest: readManifest(message.extension) };
      launch(extension, message.command, {
        fallbackText: message.fallbackText,
        arguments: message.arguments as Record<string, string> | undefined,
        launchContext: message.launchContext,
      });
    } catch (e) {
      reportError(e);
      send({ type: "closed" });
    }
  });

  on("dispatch", (message) => {
    const session = activeSession();
    if (!session || session.id !== message.session || !nodeById(message.node)) return;
    try {
      // The handler, and all it starts, is the command's.
      const result = runAs(session.context, () => dispatch(message.node, message.prop, message.args ?? []));
      if (result instanceof Promise) result.catch(reportError);
    } catch (e) {
      reportError(e);
    }
  });

  on("pop", (message) => {
    const session = activeSession();
    if (session?.id === message.session) runAs(session.context, () => session.navigation?.pop());
  });

  on("close", (message) => {
    // A close for an earlier session can arrive after the next one launched.
    if (message.session === undefined || activeSession()?.id === message.session) close();
  });

  on("open", (message) => {
    open(message.target).catch(reportError);
  });

  on("appearance", (message) => setAppearance(message.dark));

  // `heyo dev` built an extension: its command on screen starts again.
  on("dev-reload", (message) => {
    refreshDevelopment();
    relaunch(message.extension);
  });

  if (identity.background) startBackground();

  const over = connect(host);
  send({ type: "ready" });
  return over;
}
