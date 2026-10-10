// The host's commands, each with its arguments and its answer typed.
//
// One method for each command the web view can invoke
// (`generate_handler!` in src-tauri/src/lib.rs); a test holds this list to
// the host's (commands.g.dart, which `cargo test` writes). Which window may
// invoke which is the host's to enforce (commands/access.rs): a command
// that is not this window's fails with a [HostError].

import 'host.dart';
import 'models.dart';

extension HostCommands on Host {
  // --- The palette --------------------------------------------------------

  Future<List<Item>> search(String query) async => Item.listFromJson(await invoke('search', {'query': query}));

  Future<Item?> getItem(String id) async {
    final item = await invoke<Object?>('get_item', {'id': id});
    return item == null ? null : Item.fromJson((item as Map).cast());
  }

  /// Opens an application, or a command of an extension's or of Heyo's
  /// own. `arguments` are those typed in the search bar, by name.
  Future<void> launch(String id, {String? fallbackText, Map<String, String>? arguments}) =>
      invoke('launch', {'id': id, 'fallbackText': fallbackText, 'arguments': arguments});

  Future<List<Item>> listQuicklinks() async => Item.listFromJson(await invoke('list_quicklinks'));
  Future<void> recordUsage(String id) => invoke('record_usage', {'id': id});
  Future<void> setFavorite(String id, bool favorite) => invoke('set_favorite', {'id': id, 'favorite': favorite});

  /// Moves a favorite up (`offset` < 0) or down.
  Future<void> moveFavorite(String id, int offset) => invoke('move_favorite', {'id': id, 'offset': offset});

  /// Sets an item's alias; an empty one removes it.
  Future<void> setAlias(String id, String alias) => invoke('set_alias', {'id': id, 'alias': alias});
  Future<void> resetRanking(String id) => invoke('reset_ranking', {'id': id});
  Future<String> readClipboard() => invoke('read_clipboard');

  /// Copies text and hides the palette; with `paste`, also pastes it into
  /// the window that regains focus.
  Future<void> copyText(String text, {bool paste = false}) => invoke('copy_text', {'text': text, 'paste': paste});

  /// Opens a URL or path with the default application and hides the palette.
  Future<void> openTarget(String target) => invoke('open_target', {'target': target});

  /// Tells the extension runtime something (see [WebviewMessage]).
  Future<void> runtimeSend(Json message) => invoke('runtime_send', {'message': message});
  Future<void> showWindow() => invoke('show_window');

  /// Hides the window that asks.
  Future<void> hide() => invoke('hide');
  Future<void> openSettings() => invoke('open_settings');

  /// The palette's answer to `heyo://expand-snippet`.
  Future<void> snippetExpanded(int request, String text) => invoke('snippet_expanded', {'request': request, 'text': text});

  // --- The settings -------------------------------------------------------

  Future<SettingsView> getSettings() async => SettingsView.fromJson(await _object('get_settings'));

  /// Changes some settings: the new values of the ones that change, by
  /// name (`compactMode`), and nothing else.
  Future<SettingsView> changeSettings(Json changes) async => SettingsView.fromJson(await _object('change_settings', {'changes': changes}));

  /// Stores the Claude API key; an empty one removes it.
  Future<SettingsView> setAnthropicApiKey(String key) async => SettingsView.fromJson(await _object('set_anthropic_api_key', {'key': key}));
  Future<void> openExtensionsFolder() => invoke('open_extensions_folder');
  Future<List<ExtensionInfo>> listExtensions() async => [for (final item in await invoke<List<Object?>>('list_extensions')) ExtensionInfo.fromJson((item as Map).cast())];
  Future<void> setExtensionPreferences(String extension, Json values) => invoke('set_extension_preferences', {'extension': extension, 'values': values});
  Future<void> uninstallExtension(String id) => invoke('uninstall_extension', {'id': id});
  Future<void> clearClipboardHistory() => invoke('clear_clipboard_history');
  Future<Hotkeys> getHotkeys() async => Hotkeys.fromJson(await _object('get_hotkeys'));

  /// Binds one of Heyo's own hotkeys (`toggle`, `clipboard`, `dictation`).
  Future<Hotkeys> setHotkey(String name, String accelerator) async => Hotkeys.fromJson(await _object('set_hotkey', {'name': name, 'accelerator': accelerator}));

  /// Binds a global hotkey to a root search item; an empty accelerator removes it.
  Future<void> setItemHotkey(String id, String title, String accelerator) => invoke('set_item_hotkey', {'id': id, 'title': title, 'accelerator': accelerator});
  Future<Map<String, String>> itemHotkeys() async => (await _object('item_hotkeys')).cast();
  Future<List<Item>> listApplications() async => Item.listFromJson(await invoke('list_applications'));
  Future<Capabilities> getCapabilities() async => Capabilities.fromJson(await _object('get_capabilities'));
  Future<Capabilities> installShellExtension() async => Capabilities.fromJson(await _object('install_shell_extension'));

  /// The models an OpenAI-compatible server on this computer offers.
  Future<List<String>> localModels(String url) async => (await invoke<List<Object?>>('local_models', {'url': url})).cast();
  Future<OcrStatus> ocrStatus() async => OcrStatus.fromJson(await _object('ocr_status'));
  Future<void> ocrInstall() => invoke('ocr_install');
  Future<DictationStatus> dictationStatus() async => DictationStatus.fromJson(await _object('dictation_status'));
  Future<void> dictationInstall() => invoke('dictation_install');
  Future<void> copyDiagnostics() => invoke('copy_diagnostics');
  Future<List<ClipboardSource>> clipboardSources() async =>
      [for (final item in await invoke<List<Object?>>('clipboard_sources')) ClipboardSource.fromJson((item as Map).cast())];
  Future<ClipboardMeaningStatus> clipboardMeaningStatus() async => ClipboardMeaningStatus.fromJson(await _object('clipboard_meaning_status'));
  Future<void> clipboardMeaningInstall() => invoke('clipboard_meaning_install');

  /// The version that can be installed, if there is a newer one.
  Future<String?> checkForUpdate() => invoke('check_for_update');
  Future<void> installUpdate() => invoke('install_update');
  Future<List<PermissionRow>> extensionPermissions(String id) async =>
      [for (final item in await invoke<List<Object?>>('extension_permissions', {'id': id})) PermissionRow.fromJson((item as Map).cast())];
  Future<void> revokeExtensionPermission(String extension, String permission) =>
      invoke('revoke_extension_permission', {'extension': extension, 'permission': permission});

  // --- Dictation and screenshots ------------------------------------------

  Future<void> dictationToggle() => invoke('dictation_toggle');
  Future<void> dictationCancel() => invoke('dictation_cancel');

  /// Completes a capture; answers with the recognized text for `text`.
  Future<String> screenshotFinish(ScreenshotFinish request) => invoke('screenshot_finish', {'request': request.toJson()});

  Future<Json> _object(String command, [Json args = const {}]) async => (await invoke<Map<Object?, Object?>>(command, args)).cast();
}
