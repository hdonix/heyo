// Generated from src-tauri/src/wire.rs by `cargo test`. Do not edit.

/** The events the host emits to its windows. */
export const event = {
  shown: "heyo://shown",
  indexReady: "heyo://index-ready",
  settingsChanged: "heyo://settings-changed",
  openItem: "heyo://open-item",
  runtime: "heyo://runtime",
  dictationInstall: "heyo://dictation-install",
  ocrInstall: "heyo://ocr-install",
  expandSnippet: "heyo://expand-snippet",
  clipboardMeaningInstall: "heyo://clipboard-meaning-install",
  activities: "heyo://activities",
} as const;
