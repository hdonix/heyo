import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { call, reported } from "./host";
import { followAppearance } from "./theme";
import "@fontsource-variable/inter";
import "./styles/index.scss";

// The palette is the one window that is a web view. (The settings, the
// dictation pill, the screenshot overlay and a pinned capture are not: the
// host draws them, src-tauri/src/slint_windows.)
const label = getCurrentWindow().label;
followAppearance();

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

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
