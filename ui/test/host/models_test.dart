import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/host/models.dart';

import '../support/fixtures.dart';

void main() {
  test('each model reads what the host sends and writes it back the same', () {
    expectRoundTrip('Item', (json) => Item.fromJson(json).toJson());
    expectRoundTrip('SettingsView', (json) => SettingsView.fromJson(json).toJson());
    expectRoundTrip('Hotkeys', (json) => Hotkeys.fromJson(json).toJson());
    expectRoundTrip('Capabilities', (json) => Capabilities.fromJson(json).toJson());
    expectRoundTrip('OcrStatus', (json) => OcrStatus.fromJson(json).toJson());
    expectRoundTrip('DictationStatus', (json) => DictationStatus.fromJson(json).toJson());
    expectRoundTrip('ExtensionInfo', (json) => ExtensionInfo.fromJson(json).toJson());
    expectRoundTrip('ClipboardSource', (json) => ClipboardSource.fromJson(json).toJson());
    expectRoundTrip('ClipboardMeaningStatus', (json) => ClipboardMeaningStatus.fromJson(json).toJson());
    expectRoundTrip('PermissionRow', (json) => PermissionRow.fromJson(json).toJson());
    expectRoundTrip('ScreenshotCapture', (json) => ScreenshotCapture.fromJson(json).toJson());
  });

  test('what a window sends is what the host reads', () {
    // The host's test reads these very examples into its own types.
    final finish = ScreenshotFinish(action: 'pin', png: 'iVBORw0KGgo=', x: 10.5, y: 20, width: 300, height: 200, originX: 1920, originY: 0, scale: 2);
    expect(finish.toJson(), fixture('ScreenshotFinish'));
    expect(fixture('WebviewMessage'), [
      WebviewMessage.dispatch(session: 3, node: 12, prop: 'onAction', args: [1, 'two']),
      WebviewMessage.pop(session: 3),
      WebviewMessage.close(session: 3),
      WebviewMessage.reply(id: 7, result: true),
      WebviewMessage.reply(id: 8, error: 'no'),
      WebviewMessage.toastAction(id: 2, action: 'primary'),
      WebviewMessage.open(target: 'https://example.com'),
    ]);
  });

  test('an item says what it is', () {
    final items = Item.listFromJson(fixture('Item'));
    expect(items.first.title, 'Search 文件');
    expect(items.first.arguments.single.data.single.title, 'First');
    expect(items.last.subtitle, isNull);
    expect(items.last.arguments, isEmpty);
  });

  test('the settings say which appearance is asked for', () {
    final settings = SettingsView.fromJson((fixture('SettingsView') as Map).cast());
    expect(settings.appearance, Appearance.system);
    expect(settings.nodeVersion, 'v24.0.0');
    expect(settings.aliases, {'app:firefox': 'ff'});
    expect(Appearance.parse('dark'), Appearance.dark);
    expect(Appearance.parse('sepia'), Appearance.system);
  });

  /// An example nobody reads is a type of the host's that Dart has no model
  /// for: whoever uses it writes one, with a test like the first one here
  /// (in any file under test/).
  test('every example the host writes is read by a test', () {
    final tests = Directory('test').listSync(recursive: true).whereType<File>().where((file) => file.path.endsWith('_test.dart')).map((file) => file.readAsStringSync()).join();
    final examples = Directory('$repository/shared/fixtures').listSync().whereType<File>().map((file) => file.uri.pathSegments.last.replaceAll('.json', '')).toList();
    expect(examples.length, greaterThan(10));
    for (final name in examples) {
      expect(tests.contains("'$name'"), isTrue, reason: 'shared/fixtures/$name.json is read by no test: give the type a model and a round trip');
    }
  });
}
