//! Heyo's extension runtime: runs Raycast extensions (as built by `ray build`
//! or downloaded from the Raycast Store) in Node and streams their UI to the
//! webview through the Heyo host.

import { startBackground } from "./background";
import { explainMacPrograms } from "./compat";

import { open } from "./api/system";
import api from "./heyo";
import { builtinExtension } from "./builtins";
import { readManifest, runAs, scopedReact } from "./context";
import { refresh as refreshDevelopment, teeConsole } from "./dev/console";
import { on, send, start } from "./protocol";
import { dispatch, nodeById, publicInstanceHooks, type HostNode } from "./renderer";
import { activeSession, close, launch, provide, relaunch, reportError } from "./session";

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

process.on("unhandledRejection", reportError);
process.on("uncaughtException", reportError);
// An extension calling process.exit() must not take the runtime down.
process.exit = ((code?: number) => {
  console.error(`heyo-runtime: ignored process.exit(${code ?? ""}) from an extension`);
}) as typeof process.exit;

const isCurrent = (session: number) => activeSession()?.id === session;

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
  if (message.session === undefined || isCurrent(message.session)) close();
});

on("open", (message) => {
  open(message.target).catch(reportError);
});

// `heyo dev` built an extension: its command on screen starts again.
on("dev-reload", (message) => {
  refreshDevelopment();
  relaunch(message.extension);
});

explainMacPrograms();
teeConsole();

// The process for menu bar and background commands takes only those.
if (process.env.HEYO_BACKGROUND) startBackground();

start();
send({ type: "ready" });
