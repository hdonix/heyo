import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/app.dart';
import 'package:heyo_ui/host/events.g.dart';
import 'package:heyo_ui/host/fake.dart';
import 'package:heyo_ui/host/host.dart';
import 'package:heyo_ui/host/window.dart';
import 'package:heyo_ui/palette/palette.dart';
import 'package:heyo_ui/widgets/list.dart';

Map<String, Object?> _item(String id, String title, {String kind = 'Application', String? section, String? icon}) =>
    {'id': id, 'title': title, 'subtitle': null, 'icon': icon, 'kind': kind, 'favorite': false, 'section': ?section};

final _all = [
  _item('app:firefox', 'Firefox', section: 'Favorites'),
  _item('cmd:@heyo:emoji', 'Search Emoji', kind: 'Command', section: 'Suggestions', icon: 'emoji-16'),
  _item('app:files', 'Files 文件', section: 'Suggestions'),
  _item('quicklink:1', 'Search the Web', kind: 'Quicklink', section: 'Suggestions'),
];

FakeHost _host({Object? Function(Map<String, Object?> args)? launch}) => FakeHost({
  'search': (args) {
    final query = (args['query'] as String).toLowerCase();
    if (query.isEmpty) return _all;
    return [
      for (final item in _all)
        if ((item['title'] as String).toLowerCase().contains(query)) {...item}..remove('section'),
    ];
  },
  'get_settings': (_) => {'appearance': 'light'},
  'launch': launch ?? (_) => null,
  'record_usage': (_) => null,
  'hide': (_) => null,
  'open_settings': (_) => null,
  'runtime_send': (_) => null,
});

Future<FakeHost> _pump(WidgetTester tester, {FakeHost? host}) async {
  host ??= _host();
  tester.view.physicalSize = const Size(814, 534);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(HeyoApp(host: host, window: HeyoWindow(host), launch: Launch()));
  await tester.pumpAndSettle();
  return host;
}

String? _selected(WidgetTester tester) => tester.widgetList<ListRow>(find.byType(ListRow)).where((row) => row.selected).map((row) => row.title).singleOrNull;

void main() {
  testWidgets('the root search shows what the index holds, under its headings', (tester) async {
    final host = await _pump(tester);
    expect(host.invoked.where((call) => call.$1 == 'search').single.$2, {'query': ''});
    expect(find.byType(PaletteWindow), findsOneWidget);
    expect(find.byType(SectionTitle), findsNWidgets(2));
    expect(find.text('Favorites'), findsOneWidget);
    expect(find.byType(ListRow), findsNWidgets(4));
    expect(find.text('Files 文件'), findsOneWidget);
    expect(_selected(tester), 'Firefox');
    expect(find.text('Open Application'), findsOneWidget);
  });

  testWidgets('typing searches, and only the latest answer counts', (tester) async {
    final host = await _pump(tester);
    await tester.enterText(find.byType(TextField), 'fi');
    await tester.pumpAndSettle();
    expect(host.invoked.last.$1, 'search');
    expect(host.invoked.last.$2, {'query': 'fi'});
    expect(tester.widgetList<ListRow>(find.byType(ListRow)).map((row) => row.title), ['Firefox', 'Files 文件']);
    expect(find.byType(SectionTitle), findsNothing);
    await tester.enterText(find.byType(TextField), 'zzz');
    await tester.pumpAndSettle();
    expect(find.text('No results'), findsOneWidget);
  });

  testWidgets('the arrows walk the items, round and round', (tester) async {
    await _pump(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(_selected(tester), 'Search Emoji');
    expect(find.text('Open Command'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await tester.pump();
    expect(_selected(tester), 'Search the Web');
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(_selected(tester), 'Firefox');
  });

  testWidgets('Enter opens the selected item and counts the use', (tester) async {
    final host = await _pump(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    final calls = host.invoked.map((call) => call.$1).toList();
    expect(calls, containsAllInOrder(['record_usage', 'launch']));
    expect(host.invoked.last.$1, 'launch');
    expect(host.invoked.last.$2, {'id': 'cmd:@heyo:emoji', 'fallbackText': null, 'arguments': null});
  });

  testWidgets('a click opens the row clicked', (tester) async {
    final host = await _pump(tester);
    await tester.tap(find.text('Files 文件'));
    await tester.pump();
    expect(host.invoked.last.$2['id'], 'app:files');
  });

  testWidgets('what could not be opened is said, in the host\'s words', (tester) async {
    await _pump(tester, host: _host(launch: (_) => throw HostError('firefox is not installed')));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(find.textContaining('Could not open Firefox', findRichText: true), findsOneWidget);
    expect(find.textContaining('firefox is not installed', findRichText: true), findsOneWidget);
    // It goes away by itself.
    await tester.pump(const Duration(seconds: 5));
    expect(find.textContaining('Could not open Firefox', findRichText: true), findsNothing);
  });

  testWidgets('Escape clears what was typed, then hides the palette', (tester) async {
    final host = await _pump(tester);
    await tester.enterText(find.byType(TextField), 'fi');
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(tester.widget<TextField>(find.byType(TextField)).controller!.text, '');
    expect(host.invoked.where((call) => call.$1 == 'hide'), isEmpty);
    expect(find.byType(ListRow), findsNWidgets(4));
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(host.invoked.last.$1, 'hide');
  });

  testWidgets('a click in the margin around the panel hides the palette', (tester) async {
    final host = await _pump(tester);
    await tester.tapAt(const Offset(12, 12));
    await tester.pump();
    expect(host.invoked.last.$1, 'hide');
  });

  testWidgets('while an input method composes, Enter and Escape are its own', (tester) async {
    final host = await _pump(tester);
    tester.testTextInput.updateEditingValue(const TextEditingValue(text: 'wen jian', selection: TextSelection.collapsed(offset: 8), composing: TextRange(start: 0, end: 8)));
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    expect(host.invoked.map((call) => call.$1), isNot(contains('launch')));
    expect(host.invoked.map((call) => call.$1), isNot(contains('hide')));
    expect(tester.widget<TextField>(find.byType(TextField)).controller!.text, 'wen jian');
  });

  testWidgets('coming up again starts from the root search, and follows the index', (tester) async {
    final host = await _pump(tester);
    await tester.enterText(find.byType(TextField), 'fi');
    await tester.pumpAndSettle();
    host.emit(HeyoEvent.shown);
    await tester.pumpAndSettle();
    expect(tester.widget<TextField>(find.byType(TextField)).controller!.text, '');
    expect(find.byType(ListRow), findsNWidgets(4));
    // The index changed under it: asked again, the selection kept.
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pump();
    final searches = host.invoked.where((call) => call.$1 == 'search').length;
    host.emit(HeyoEvent.indexReady);
    await tester.pumpAndSettle();
    expect(host.invoked.where((call) => call.$1 == 'search').length, searches + 1);
    expect(_selected(tester), 'Search Emoji');
  });

  testWidgets('a command that shows something takes the palette over until Escape', (tester) async {
    final host = await _pump(tester);
    host.emit(HeyoEvent.runtime, {'type': 'render', 'session': 4, 'view': {'type': 'List'}});
    await tester.pump();
    expect(find.byType(ListRow), findsNothing);
    expect(find.text('Command'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(host.invoked.last.$1, 'runtime_send');
    expect(host.invoked.last.$2, {'message': {'type': 'close', 'session': 4}});
    expect(find.byType(ListRow), findsNWidgets(4));
  });

  testWidgets('each kind of window gets its own view', (tester) async {
    for (final (kind, text) in [('settings', 'Settings'), ('dictation', 'Dictation'), ('screenshot', 'Screenshot')]) {
      final host = _host();
      await tester.pumpWidget(HeyoApp(key: ValueKey(kind), host: host, window: HeyoWindow(host), launch: Launch(kind: kind)));
      await tester.pump();
      expect(find.text(text), findsOneWidget, reason: kind);
      expect(find.byType(PaletteWindow), findsNothing);
    }
    expect(floats('dictation') && floats('screenshot') && floats('pin'), isTrue);
    expect(floats('main') || floats('settings'), isFalse);
  });
}
