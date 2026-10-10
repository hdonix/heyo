// A view started by hand, without Heyo (`heyo-ui --standalone --window=…`):
// for working on a view. The host is a `FakeHost` that answers with made-up
// rows and settings, and the window shows itself.

import 'dart:io';

import '../host/fake.dart';
import '../host/host.dart';

const _words = ['Search', 'Open', 'Toggle', 'Create', 'Switch', 'Capture', 'Copy', 'Paste', 'Show', 'Manage'];
const _things = ['Files', 'Windows', 'Clipboard History', 'Snippets', 'Quicklinks', 'Dark Mode', 'Emoji', 'Screenshot', '书签', 'Extensions'];
const _icons = ['magnifying-glass-16', 'app-window-16', 'clipboard-16', 'text-cursor-16', 'link-16', 'moon-16', 'emoji-16', 'camera-16', 'bookmark-16', null];

/// Two thousand rows: about what the largest lists in Heyo hold.
List<Map<String, Object?>> madeUpItems(String query) {
  final rows = <Map<String, Object?>>[];
  for (var i = 0; i < 2000; i++) {
    final title = '${_words[i % 10]} ${_things[(i ~/ 10) % 10]} $i';
    if (query.isEmpty || title.toLowerCase().contains(query.toLowerCase())) {
      rows.add({
        'id': 'made:$i',
        'title': title,
        'subtitle': i % 3 == 0 ? 'Heyo' : 'A longer description of what number $i does',
        'icon': _icons[(i ~/ 10) % 10],
        'kind': i % 4 == 0 ? 'Application' : 'Command',
        'favorite': false,
        if (query.isEmpty && i < 12) 'section': i < 4 ? 'Favorites' : 'Suggestions',
      });
    }
  }
  return rows;
}

Map<String, Object?> madeUpSettings() => {'appearance': Platform.environment['HEYO_UI_APPEARANCE'] ?? 'system', 'hideOnBlur': false, 'compactMode': false, 'keepCommand': false, 'version': '0.0.0'};

FakeHost standaloneHost() => FakeHost({
  'search': (args) => madeUpItems(args['query'] as String? ?? ''),
  'get_settings': (_) => madeUpSettings(),
  'launch': (args) => stderr.writeln('heyo-ui: would launch ${args['id']}'),
  'record_usage': (_) => null,
  'hide': (_) => exit(0),
  'screenshot_finish': (_) => exit(0),
});

/// What the host would have said the window is like.
Map<String, Object?> standaloneWindow(String kind) => switch (kind) {
  'settings' => {'title': 'Heyo Settings', 'width': 924.0, 'height': 704.0, 'focus': true, 'taskbar': true},
  'dictation' => {'title': 'Heyo Dictation', 'width': 420.0, 'height': 52.0, 'focus': false, 'above': true, 'taskbar': false},
  'screenshot' => {'title': 'Heyo Screenshot', 'width': 800.0, 'height': 600.0, 'above': true, 'taskbar': false},
  'pin' => {'title': 'Heyo Pinned Screenshot', 'width': 320.0, 'height': 200.0, 'above': true, 'taskbar': false},
  'island' => {'title': 'Heyo Island', 'width': 420.0, 'height': 160.0, 'focus': false, 'above': true, 'taskbar': false},
  _ => {'title': 'Heyo', 'width': 814.0, 'height': 534.0, 'focus': true, 'above': true, 'taskbar': false},
};

/// Puts the window on screen, as the host would. `HEYO_UI_COVER=x,y,w,h`
/// has it cover that rectangle instead, as the screenshot overlay does
/// (it takes the keyboard: the placeholder lets go at any key).
void standaloneShow(Host host) {
  if (host is! FakeHost) return;
  final cover = Platform.environment['HEYO_UI_COVER']?.split(',').map(double.tryParse).toList();
  if (cover != null && cover.length == 4 && !cover.contains(null)) {
    host.tell('overlay', {'x': cover[0], 'y': cover[1], 'width': cover[2], 'height': cover[3], 'backdrop': false});
    return;
  }
  host
    ..tell('center')
    ..tell('show')
    ..tell('focus')
    ..emit('heyo://shown');
}
