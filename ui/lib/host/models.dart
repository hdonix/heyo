// What the host sends a window and what a window sends it, as Dart types.
//
// Each is the Rust type of the same name written by hand, held to it by
// example: `cargo test` writes one of each into `shared/fixtures/`
// (src-tauri/src/shell/fixtures.rs), and test/host/models_test.dart reads
// every file into its model here and must write the same JSON back. A model
// for a type that is one view's own belongs in that view's directory, with a
// test of the same kind.

typedef Json = Map<String, Object?>;

Json _map(Object? value) => (value as Map).cast<String, Object?>();
List<T> _list<T>(Object? value, T Function(Object? item) of) => [for (final item in (value as List?) ?? const []) of(item)];
List<String> _strings(Object? value) => _list(value, (item) => item as String);

/// Without the entries that hold nothing: how the host leaves out what an
/// item does not have.
Json _present(Json json) => json..removeWhere((_, value) => value == null);

/// A choice of a dropdown argument.
class Choice {
  Choice({required this.title, required this.value});
  final String title;
  final String value;

  factory Choice.fromJson(Json json) => Choice(title: json['title'] as String, value: json['value'] as String);
  Json toJson() => {'title': title, 'value': value};
}

/// One thing a command asks for before it runs: a field in the search bar.
class Argument {
  Argument({required this.name, required this.type, this.placeholder, required this.required, this.data = const []});
  final String name;

  /// `text`, `password` or `dropdown`.
  final String type;
  final String? placeholder;
  final bool required;

  /// The choices of a `dropdown`.
  final List<Choice> data;

  factory Argument.fromJson(Json json) => Argument(
    name: json['name'] as String,
    type: json['type'] as String? ?? 'text',
    placeholder: json['placeholder'] as String?,
    required: json['required'] as bool? ?? false,
    data: _list(json['data'], (item) => Choice.fromJson(_map(item))),
  );
  Json toJson() => _present({'name': name, 'type': type, 'placeholder': placeholder, 'required': required, 'data': data.isEmpty ? null : [for (final choice in data) choice.toJson()]});
}

/// A row of the root search.
class Item {
  Item({
    required this.id,
    required this.title,
    this.subtitle,
    this.icon,
    required this.kind,
    this.alias,
    this.favorite = false,
    this.hotkey,
    this.section,
    this.link,
    this.text,
    this.arguments = const [],
  });
  final String id;
  final String title;
  final String? subtitle;

  /// A file path, URL, or Raycast icon name.
  final String? icon;

  /// `Application`, `Command`, `Quicklink` or `Snippet`.
  final String kind;
  final String? alias;
  final bool favorite;

  /// Global hotkey (GTK accelerator) bound to this item.
  final String? hotkey;

  /// Heading for the empty-query view ("Favorites", "Suggestions").
  final String? section;

  /// Quicklink URL template.
  final String? link;

  /// Snippet text.
  final String? text;
  final List<Argument> arguments;

  factory Item.fromJson(Json json) => Item(
    id: json['id'] as String,
    title: json['title'] as String,
    subtitle: json['subtitle'] as String?,
    icon: json['icon'] as String?,
    kind: json['kind'] as String,
    alias: json['alias'] as String?,
    favorite: json['favorite'] as bool? ?? false,
    hotkey: json['hotkey'] as String?,
    section: json['section'] as String?,
    link: json['link'] as String?,
    text: json['text'] as String?,
    arguments: _list(json['arguments'], (item) => Argument.fromJson(_map(item))),
  );
  Json toJson() => _present({
    'id': id,
    'title': title,
    'subtitle': subtitle,
    'icon': icon,
    'kind': kind,
    'alias': alias,
    'favorite': favorite,
    'hotkey': hotkey,
    'section': section,
    'link': link,
    'text': text,
    'arguments': arguments.isEmpty ? null : [for (final argument in arguments) argument.toJson()],
  });

  static List<Item> listFromJson(Object? json) => _list(json, (item) => Item.fromJson(_map(item)));
}

/// The colour scheme the settings ask for.
enum Appearance {
  system,
  light,
  dark;

  static Appearance parse(Object? name) => values.firstWhere((value) => value.name == name, orElse: () => system);
}

/// The settings, with what the settings page shows beside them
/// (`SettingsView` in commands/settings.rs, which holds `Settings`).
class SettingsView {
  SettingsView(this.json);

  /// All of it, as sent: the settings window changes settings by name
  /// (`change_settings`), so nothing here is ever sent back whole.
  final Json json;

  factory SettingsView.fromJson(Json json) => SettingsView(json);
  Json toJson() => {
    'appearance': appearance.name,
    'hideOnBlur': hideOnBlur,
    'compactMode': compactMode,
    'keepCommand': keepCommand,
    'nodePath': nodePath,
    'clipboardHistory': clipboardHistory,
    'clipboardRetentionDays': clipboardRetentionDays,
    'disabledCommands': disabledCommands,
    'favorites': favorites,
    'aliases': aliases,
    'dictationLanguage': dictationLanguage,
    'dictationSource': dictationSource,
    'dictationPolish': dictationPolish,
    'dictationHoldKey': dictationHoldKey,
    'anthropicApiKey': anthropicApiKey,
    'aiProvider': aiProvider,
    'localModelUrl': localModelUrl,
    'localModelName': localModelName,
    'clipboardPaused': clipboardPaused,
    'clipboardIgnoredApps': clipboardIgnoredApps,
    'dictationVocabulary': dictationVocabulary,
    'confirmTerminalPaste': confirmTerminalPaste,
    'snippetExpansion': snippetExpansion,
    'snippetExpansionIgnoredApps': snippetExpansionIgnoredApps,
    'clipboardMeaning': clipboardMeaning,
    'anthropicApiKeySet': anthropicApiKeySet,
    'launchAtLogin': launchAtLogin,
    'nodeVersion': nodeVersion == null ? {'Err': nodeProblem} : {'Ok': nodeVersion},
    'nodeOverridden': nodeOverridden,
    'extensionsDir': extensionsDir,
    'settingsPath': settingsPath,
    'version': version,
    'canUpdate': canUpdate,
  };

  bool _flag(String name) => json[name] as bool? ?? false;
  String _text(String name) => json[name] as String? ?? '';

  Appearance get appearance => Appearance.parse(json['appearance']);

  /// Hide the window when it loses focus.
  bool get hideOnBlur => _flag('hideOnBlur');

  /// Show only the search field until something is typed.
  bool get compactMode => _flag('compactMode');

  /// Reopening the window returns to the running command.
  bool get keepCommand => _flag('keepCommand');
  String get nodePath => _text('nodePath');
  bool get clipboardHistory => _flag('clipboardHistory');
  int get clipboardRetentionDays => (json['clipboardRetentionDays'] as num?)?.toInt() ?? 0;
  List<String> get disabledCommands => _strings(json['disabledCommands']);
  List<String> get favorites => _strings(json['favorites']);
  Map<String, String> get aliases => ((json['aliases'] as Map?) ?? const {}).cast<String, String>();
  String get dictationLanguage => _text('dictationLanguage');
  String get dictationSource => _text('dictationSource');
  String get dictationPolish => _text('dictationPolish');
  String get dictationHoldKey => _text('dictationHoldKey');

  /// Always empty: the key itself never goes to a window.
  String get anthropicApiKey => _text('anthropicApiKey');
  String get aiProvider => _text('aiProvider');
  String get localModelUrl => _text('localModelUrl');
  String get localModelName => _text('localModelName');
  bool get clipboardPaused => _flag('clipboardPaused');
  List<String> get clipboardIgnoredApps => _strings(json['clipboardIgnoredApps']);
  List<String> get dictationVocabulary => _strings(json['dictationVocabulary']);
  bool get confirmTerminalPaste => _flag('confirmTerminalPaste');
  bool get snippetExpansion => _flag('snippetExpansion');
  List<String> get snippetExpansionIgnoredApps => _strings(json['snippetExpansionIgnoredApps']);
  bool get clipboardMeaning => _flag('clipboardMeaning');
  bool get anthropicApiKeySet => _flag('anthropicApiKeySet');
  bool get launchAtLogin => _flag('launchAtLogin');

  /// The version of Node.js found, or null with [nodeProblem] saying why not.
  String? get nodeVersion => (json['nodeVersion'] as Map?)?['Ok'] as String?;
  String? get nodeProblem => (json['nodeVersion'] as Map?)?['Err'] as String?;
  bool get nodeOverridden => _flag('nodeOverridden');

  /// Empty for every window but the settings.
  String get extensionsDir => _text('extensionsDir');
  String get settingsPath => _text('settingsPath');
  String get version => _text('version');
  bool get canUpdate => _flag('canUpdate');
}

class Hotkeys {
  Hotkeys({required this.supported, this.toggle, this.clipboard, this.dictation});
  final bool supported;
  final String? toggle;
  final String? clipboard;
  final String? dictation;

  factory Hotkeys.fromJson(Json json) =>
      Hotkeys(supported: json['supported'] as bool, toggle: json['toggle'] as String?, clipboard: json['clipboard'] as String?, dictation: json['dictation'] as String?);
  Json toJson() => {'supported': supported, 'toggle': toggle, 'clipboard': clipboard, 'dictation': dictation};
}

/// A key that can be held to talk.
class HoldKey {
  HoldKey({required this.value, required this.title});
  final String value;
  final String title;

  factory HoldKey.fromJson(Json json) => HoldKey(value: json['value'] as String, title: json['title'] as String);
  Json toJson() => {'value': value, 'title': title};
}

/// What the running session can do. Each ability is `full`, `limited` or
/// `none`.
class Capabilities {
  Capabilities({
    required this.session,
    required this.windows,
    required this.paste,
    required this.selection,
    required this.clipboardWatch,
    required this.hotkeys,
    required this.holdKey,
    required this.capture,
    required this.overlay,
    required this.snippetExpansion,
    required this.holdKeys,
    required this.extension,
  });

  /// `x11`, `wayland`, `windows` or `unknown`.
  final String session;
  final String windows;
  final String paste;
  final String selection;
  final String clipboardWatch;
  final String hotkeys;
  final String holdKey;
  final String capture;
  final String overlay;
  final String snippetExpansion;
  final List<HoldKey> holdKeys;

  /// Heyo's GNOME Shell extension: `none`, `missing`, `outdated`,
  /// `pending` or `active`.
  final String extension;

  factory Capabilities.fromJson(Json json) => Capabilities(
    session: json['session'] as String,
    windows: json['windows'] as String,
    paste: json['paste'] as String,
    selection: json['selection'] as String,
    clipboardWatch: json['clipboardWatch'] as String,
    hotkeys: json['hotkeys'] as String,
    holdKey: json['holdKey'] as String,
    capture: json['capture'] as String,
    overlay: json['overlay'] as String,
    snippetExpansion: json['snippetExpansion'] as String,
    holdKeys: _list(json['holdKeys'], (item) => HoldKey.fromJson(_map(item))),
    extension: json['extension'] as String,
  );
  Json toJson() => {
    'session': session,
    'windows': windows,
    'paste': paste,
    'selection': selection,
    'clipboardWatch': clipboardWatch,
    'hotkeys': hotkeys,
    'holdKey': holdKey,
    'capture': capture,
    'overlay': overlay,
    'snippetExpansion': snippetExpansion,
    'holdKeys': [for (final key in holdKeys) key.toJson()],
    'extension': extension,
  };
}

/// Whether an engine Heyo downloads when asked is there (`ocr_status`).
class OcrStatus {
  OcrStatus({required this.installed, required this.installing});
  final bool installed;
  final bool installing;

  factory OcrStatus.fromJson(Json json) => OcrStatus(installed: json['installed'] as bool, installing: json['installing'] as bool);
  Json toJson() => {'installed': installed, 'installing': installing};
}

class Microphone {
  Microphone({required this.name, required this.description});
  final String name;
  final String description;

  factory Microphone.fromJson(Json json) => Microphone(name: json['name'] as String, description: json['description'] as String);
  Json toJson() => {'name': name, 'description': description};
}

/// `dictation_status`.
class DictationStatus {
  DictationStatus({required this.installed, required this.installing, required this.microphones});
  final bool installed;
  final bool installing;
  final List<Microphone> microphones;

  factory DictationStatus.fromJson(Json json) => DictationStatus(
    installed: json['installed'] as bool,
    installing: json['installing'] as bool,
    microphones: _list(json['microphones'], (item) => Microphone.fromJson(_map(item))),
  );
  Json toJson() => {'installed': installed, 'installing': installing, 'microphones': [for (final microphone in microphones) microphone.toJson()]};
}

/// A command of an extension, for the settings.
class CommandInfo {
  CommandInfo({required this.id, required this.name, required this.title, this.description, required this.mode, this.icon, this.preferences});
  final String id;
  final String name;
  final String title;
  final String? description;
  final String mode;
  final String? icon;

  /// Preference definitions from the manifest, as they are there.
  final Object? preferences;

  factory CommandInfo.fromJson(Json json) => CommandInfo(
    id: json['id'] as String,
    name: json['name'] as String,
    title: json['title'] as String,
    description: json['description'] as String?,
    mode: json['mode'] as String,
    icon: json['icon'] as String?,
    preferences: json['preferences'],
  );
  Json toJson() => {'id': id, 'name': name, 'title': title, 'description': description, 'mode': mode, 'icon': icon, 'preferences': preferences};
}

/// An installed extension, for the settings (`list_extensions`).
class ExtensionInfo {
  ExtensionInfo({
    required this.id,
    required this.name,
    required this.title,
    this.description,
    this.author,
    this.icon,
    required this.builtin,
    this.development,
    required this.commands,
    this.preferences,
    this.values,
  });

  /// `@heyo`, or the extension's directory.
  final String id;
  final String name;
  final String title;
  final String? description;
  final String? author;
  final String? icon;
  final bool builtin;

  /// The source directory, for an extension being developed (`heyo dev`).
  final String? development;
  final List<CommandInfo> commands;

  /// Extension-level preference definitions from the manifest.
  final Object? preferences;

  /// `{ extension: {…}, commands: { name: {…} } }`, as the runtime stores them.
  final Object? values;

  factory ExtensionInfo.fromJson(Json json) => ExtensionInfo(
    id: json['id'] as String,
    name: json['name'] as String,
    title: json['title'] as String,
    description: json['description'] as String?,
    author: json['author'] as String?,
    icon: json['icon'] as String?,
    builtin: json['builtin'] as bool,
    development: json['development'] as String?,
    commands: _list(json['commands'], (item) => CommandInfo.fromJson(_map(item))),
    preferences: json['preferences'],
    values: json['values'],
  );
  Json toJson() => {
    'id': id,
    'name': name,
    'title': title,
    'description': description,
    'author': author,
    'icon': icon,
    'builtin': builtin,
    'development': development,
    'commands': [for (final command in commands) command.toJson()],
    'preferences': preferences,
    'values': values,
  };
}

/// An application that can be named as one not to record copies from.
class ClipboardSource {
  ClipboardSource({required this.id, required this.name, this.icon, required this.seen});
  final String id;
  final String name;
  final String? icon;

  /// There are copies from it in the history.
  final bool seen;

  factory ClipboardSource.fromJson(Json json) => ClipboardSource(id: json['id'] as String, name: json['name'] as String, icon: json['icon'] as String?, seen: json['seen'] as bool);
  Json toJson() => {'id': id, 'name': name, 'icon': icon, 'seen': seen};
}

/// Where finding clipboard entries by meaning stands.
class ClipboardMeaningStatus {
  ClipboardMeaningStatus({required this.enabled, required this.installed, required this.installing, required this.indexed, required this.waiting, this.problem});
  final bool enabled;
  final bool installed;
  final bool installing;
  final int indexed;
  final int waiting;
  final String? problem;

  factory ClipboardMeaningStatus.fromJson(Json json) => ClipboardMeaningStatus(
    enabled: json['enabled'] as bool,
    installed: json['installed'] as bool,
    installing: json['installing'] as bool,
    indexed: (json['indexed'] as num).toInt(),
    waiting: (json['waiting'] as num).toInt(),
    problem: json['problem'] as String?,
  );
  Json toJson() => {'enabled': enabled, 'installed': installed, 'installing': installing, 'indexed': indexed, 'waiting': waiting, 'problem': problem};
}

/// A permission of an extension, as its page in the settings shows it.
class PermissionRow {
  PermissionRow({required this.permission, required this.allows, required this.state});

  /// Its name: `windows`, `screen.capture`, …
  final String permission;

  /// What it lets the extension do, after "allow it to".
  final String allows;

  /// `allowed`, `asks` or `free`.
  final String state;

  factory PermissionRow.fromJson(Json json) => PermissionRow(permission: json['permission'] as String, allows: json['allows'] as String, state: json['state'] as String);
  Json toJson() => {'permission': permission, 'allows': allows, 'state': state};
}

/// A rectangle in physical pixels.
class PixelRect {
  PixelRect({required this.x, required this.y, required this.width, required this.height});
  final int x;
  final int y;
  final int width;
  final int height;

  factory PixelRect.fromJson(Json json) =>
      PixelRect(x: (json['x'] as num).toInt(), y: (json['y'] as num).toInt(), width: (json['width'] as num).toInt(), height: (json['height'] as num).toInt());
  Json toJson() => {'x': x, 'y': y, 'width': width, 'height': height};
}

/// A capture ready for the screenshot overlay (`heyo://screenshot`).
class ScreenshotCapture {
  ScreenshotCapture({required this.path, required this.mode, required this.x, required this.y, required this.scale, required this.width, required this.height, required this.windows});

  /// The capture, as an uncompressed picture (BMP), to crop from.
  final String path;

  /// `capture` shows the tools; `text` reads the text of what is selected;
  /// `pick` is an extension asking for a selection.
  final String mode;

  /// Where the overlay is on the screen, in physical pixels.
  final int x;
  final int y;
  final double scale;

  /// The size of the capture, in physical pixels.
  final int width;
  final int height;

  /// The windows in the capture, the topmost first, in physical pixels
  /// from the capture's top left corner.
  final List<PixelRect> windows;

  factory ScreenshotCapture.fromJson(Json json) => ScreenshotCapture(
    path: json['path'] as String,
    mode: json['mode'] as String,
    x: (json['x'] as num).toInt(),
    y: (json['y'] as num).toInt(),
    scale: (json['scale'] as num).toDouble(),
    width: (json['width'] as num).toInt(),
    height: (json['height'] as num).toInt(),
    windows: _list(json['windows'], (item) => PixelRect.fromJson(_map(item))),
  );
  Json toJson() => {'path': path, 'mode': mode, 'x': x, 'y': y, 'scale': scale, 'width': width, 'height': height, 'windows': [for (final window in windows) window.toJson()]};
}

/// What the user chose to do with the selected region (`screenshot_finish`).
class ScreenshotFinish {
  ScreenshotFinish({required this.action, this.png = '', this.x = 0, this.y = 0, this.width = 0, this.height = 0, this.originX = 0, this.originY = 0, this.scale = 1});

  /// `copy`, `save`, `pin`, `text`, `cancel` or `pick`.
  final String action;

  /// The cropped, annotated region as base64 PNG.
  final String png;

  /// The selection in logical pixels, from the overlay's corner.
  final double x;
  final double y;
  final double width;
  final double height;

  /// Where the overlay is on the screen in physical pixels, and its scale.
  final int originX;
  final int originY;
  final double scale;

  Json toJson() => {'action': action, 'png': png, 'x': x, 'y': y, 'width': width, 'height': height, 'origin_x': originX, 'origin_y': originY, 'scale': scale};
}

/// What a window may tell the extension runtime (`runtime_send`): input for
/// the command on screen.
abstract final class WebviewMessage {
  static Json dispatch({required int session, required int node, required String prop, List<Object?> args = const []}) =>
      {'type': 'dispatch', 'session': session, 'node': node, 'prop': prop, 'args': args};
  static Json pop({required int session}) => {'type': 'pop', 'session': session};
  static Json close({int? session}) => _present({'type': 'close', 'session': session});
  static Json reply({required int id, Object? result, String? error}) => _present({'type': 'reply', 'id': id, 'result': result, 'error': error});
  static Json toastAction({required int id, required String action}) => {'type': 'toast-action', 'id': id, 'action': action};
  static Json open({required String target}) => {'type': 'open', 'target': target};
}
