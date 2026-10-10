// Generated from src-tauri/src/wire.rs by `cargo test`. Do not edit.

/// The events the host emits to its windows.
abstract final class HeyoEvent {
  static const shown = 'heyo://shown';
  static const indexReady = 'heyo://index-ready';
  static const settingsChanged = 'heyo://settings-changed';
  static const openItem = 'heyo://open-item';
  static const runtime = 'heyo://runtime';
  static const dictation = 'heyo://dictation';
  static const dictationInstall = 'heyo://dictation-install';
  static const ocrInstall = 'heyo://ocr-install';
  static const screenshot = 'heyo://screenshot';
  static const expandSnippet = 'heyo://expand-snippet';
  static const clipboardMeaningInstall = 'heyo://clipboard-meaning-install';
}
