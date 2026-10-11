//! What of Raycast's API is there by name and does not do its work on
//! Heyo, said once: the modules that refuse are made from here
//! (`index.ts`), `has()` answers by it (`../heyo/index.ts`), and `heyo ext
//! check` tells an author who uses one (`../dev/check.ts`).
//!
//! This imports nothing of the runtime: the developer tools read it.

/// Modules whose functions all refuse: what the error calls the module, and
/// why it cannot be had.
export const REFUSING = {
  BrowserExtension: { called: "The browser extension API", why: "there is no browser extension to ask", functions: ["getContent", "getTabs"] },
} as const;

/// Functions that are taken and do nothing, with what happens instead.
export const IDLE = {
  updateCommandMetadata: "the root search shows the subtitle from the manifest",
} as const;
