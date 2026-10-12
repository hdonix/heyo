import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { DictationWindow } from "./Dictation";
import { call, reported } from "./host";
import { PinWindow, ScreenshotOverlay } from "./Screenshot";
import { applyAppearance, followAppearance } from "./theme";
import "@fontsource-variable/inter";
import "./styles/index.scss";

// Each window renders its own root. Windows declared in tauri.conf.json are
// told apart by label; those created at run time (pinned screenshots)
// set `window.__HEYO_VIEW__` in an initialization script. (The settings
// are not a web view's: the host draws them, src-tauri/src/slint_settings.)
const label = getCurrentWindow().label;
const heyo = window as { __HEYO_VIEW__?: string; __HEYO_PIN__?: string };
if (heyo.__HEYO_VIEW__) document.documentElement.classList.add("standalone");

// What floats over the screen (the screenshot tools, the dictation pill, a
// pinned capture) is dark whatever the scheme; the rest follows the settings.
const floating = label === "dictation" || label === "screenshot" || heyo.__HEYO_VIEW__ === "pin";
if (floating) applyAppearance("dark");
else followAppearance();

// The palette's window is larger than the panel by a transparent margin,
// where the panel's shadow falls. A click there is a click outside.
if (label === "main") {
  const page = document.documentElement;
  page.classList.add("floating-window");
  window.addEventListener("mousedown", (event) => {
    const target = event.target as HTMLElement;
    if (target === page || target === document.body || target.id === "root") call("hide").catch(reported());
  });
}

// Measurements for development (scripts/gui-test/scroll.sh); not in a build.
if (import.meta.env.DEV || import.meta.env.VITE_HEYO_BENCH) void import("./dev/bench");

function root() {
  if (label === "dictation") return <DictationWindow />;
  if (label === "screenshot") return <ScreenshotOverlay />;
  if (heyo.__HEYO_VIEW__ === "pin") return <PinWindow path={heyo.__HEYO_PIN__ ?? ""} />;
  return <App />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(<React.StrictMode>{root()}</React.StrictMode>);
