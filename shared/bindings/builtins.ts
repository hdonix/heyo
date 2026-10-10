// Generated from src-tauri/src/extensions.rs by `cargo test`. Do not edit.

/** Heyo's built-in commands, as the runtime's manifest of them. */
export const builtins = [
  { name: "store", title: "Search Raycast Store", mode: "view", icon: "store-16" },
  { name: "extensions", title: "Manage Extensions", mode: "view", icon: "box-16" },
  { name: "clipboard-history", title: "Clipboard History", mode: "view", icon: "clipboard-16" },
  { name: "selection", title: "Selected Text Actions", mode: "view", icon: "text-selection-16" },
  { name: "ask-ai", title: "Ask AI", mode: "view", icon: "stars-16" },
  { name: "ai-chat", title: "AI Chat", mode: "view", icon: "speech-bubble-16" },
  { name: "create-quicklink", title: "Create Quicklink", mode: "view", icon: "link-16" },
  { name: "search-quicklinks", title: "Search Quicklinks", mode: "view", icon: "link-16" },
  { name: "create-snippet", title: "Create Snippet", mode: "view", icon: "snippets-16" },
  { name: "search-snippets", title: "Search Snippets", mode: "view", icon: "snippets-16" },
  { name: "switch-windows", title: "Switch Windows", mode: "view", icon: "app-window-list-16" },
  { name: "emoji", title: "Search Emoji & Symbols", mode: "view", icon: "emoji-16" },
  { name: "search-files", title: "Search Files", mode: "view", icon: "magnifying-glass-16" },
  { name: "kill-process", title: "Kill Process", mode: "view", icon: "bolt-16" },
  { name: "audio-devices", title: "Switch Audio Device", mode: "view", icon: "speaker-on-16" },
  { name: "search-bookmarks", title: "Search Browser Bookmarks", mode: "view", icon: "bookmark-16" },
  { name: "search-history", title: "Search Browser History", mode: "view", icon: "clock-16" },
  { name: "lock-screen", title: "Lock Screen", mode: "no-view", icon: "lock-16" },
  { name: "sleep", title: "Sleep", mode: "no-view", icon: "moon-16" },
  { name: "restart", title: "Restart", mode: "no-view", icon: "arrow-clockwise-16" },
  { name: "shut-down", title: "Shut Down", mode: "no-view", icon: "power-16" },
  { name: "log-out", title: "Log Out", mode: "no-view", icon: "logout-16" },
  { name: "empty-trash", title: "Empty Trash", mode: "no-view", icon: "trash-16" },
  { name: "toggle-dark-mode", title: "Toggle Dark Mode", mode: "no-view", icon: "moon-16" },
  { name: "volume-up", title: "Volume Up", mode: "no-view", icon: "speaker-up-16" },
  { name: "volume-down", title: "Volume Down", mode: "no-view", icon: "speaker-down-16" },
  { name: "toggle-mute", title: "Toggle Mute", mode: "no-view", icon: "speaker-off-16" },
] as const;

export type BuiltinName = (typeof builtins)[number]["name"];
