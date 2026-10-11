// Generated from src-tauri/src/permissions.rs by `cargo test`. Do not edit.

/** The permissions there are, as a manifest lists them. */
export const permissions = ["windows", "screen.capture", "screen.text", "microphone", "clipboard.history", "memory.read", "memory.write", "files.search", "system", "input.paste", "ai", "search.provider", "activity"] as const;

/**
 * What each request of an extension's needs before it is answered: the
 * permission, and the function of the API it is asked through, which is
 * what a refusal names. Null where it needs nothing.
 */
export const needs = {
  "ext.activity.end": { permission: "activity", member: "LiveActivity.Handle.end" },
  "ext.activity.get": { permission: "activity", member: "LiveActivity.get" },
  "ext.activity.list": { permission: "activity", member: "LiveActivity.list" },
  "ext.activity.onEvent": { permission: "activity", member: "LiveActivity.onEvent" },
  "ext.activity.presentation": null,
  "ext.activity.start": { permission: "activity", member: "LiveActivity.start" },
  "ext.activity.update": { permission: "activity", member: "LiveActivity.Handle.update" },
  "ext.ai.provider": { permission: "ai", member: "AI.provider" },
  "ext.clipboard.paste": { permission: "input.paste", member: "Clipboard.paste" },
  "ext.clipboardHistory.copy": { permission: "clipboard.history", member: "ClipboardHistory.copy" },
  "ext.dictation.installed": null,
  "ext.dictation.listen": { permission: "microphone", member: "Dictation.listen" },
  "ext.dictation.stop": null,
  "ext.files.search": { permission: "files.search", member: "Files.search" },
  "ext.frontmostApplication": { permission: "windows", member: "getFrontmostApplication" },
  "ext.memory.get": { permission: "memory.read", member: "Memory.get" },
  "ext.memory.search": { permission: "memory.read", member: "Memory.search" },
  "ext.screen.capture": { permission: "screen.capture", member: "Screen.capture" },
  "ext.screen.recognizeText": { permission: "screen.text", member: "Screen.recognizeText" },
  "ext.supported": null,
  "ext.system.appearance": { permission: "system", member: "System.appearance" },
  "ext.system.audioDevices": { permission: "system", member: "System.audioDevices" },
  "ext.system.notify": { permission: "system", member: "System.notify" },
  "ext.system.run": { permission: "system", member: "System.run" },
  "ext.system.useAudioDevice": { permission: "system", member: "System.useAudioDevice" },
  "ext.unsubscribe": null,
  "ext.windows.activate": { permission: "windows", member: "Windows.activate" },
  "ext.windows.active": { permission: "windows", member: "Windows.active" },
  "ext.windows.arrange": { permission: "windows", member: "Windows.arrange" },
  "ext.windows.close": { permission: "windows", member: "Windows.close" },
  "ext.windows.list": { permission: "windows", member: "Windows.list" },
  "ext.windows.monitors": { permission: "windows", member: "Windows.monitors" },
  "ext.windows.setBounds": { permission: "windows", member: "Windows.setBounds" },
} as const;
