import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/host/events.g.dart';
import 'package:heyo_ui/host/fake.dart';
import 'package:heyo_ui/host/models.dart';
import 'package:heyo_ui/theme/fonts.dart' as fonts;
import 'package:heyo_ui/theme/theme.dart';

import '../support/fixtures.dart';

void main() {
  test('the scheme is the one chosen, or the system\'s', () {
    expect(schemeFor(Appearance.light, Brightness.dark), Brightness.light);
    expect(schemeFor(Appearance.dark, Brightness.light), Brightness.dark);
    expect(schemeFor(Appearance.system, Brightness.dark), Brightness.dark);
    expect(schemeFor(Appearance.system, Brightness.light), Brightness.light);
  });

  test('text of the scale has its size, its line and its spacing', () {
    const theme = HeyoThemeData(brightness: Brightness.light, fallback: ['Noto Sans CJK SC']);
    final row = theme.text(TextSize.row, weight: Weights.medium);
    expect(row.fontFamily, 'Inter');
    expect(row.fontFamilyFallback, ['Noto Sans CJK SC']);
    expect(row.fontSize, 15);
    expect(row.height! * row.fontSize!, closeTo(20, 1e-9));
    expect(row.letterSpacing, closeTo(-0.135, 1e-9));
    // Inter is one file with every weight: the weight is asked of it.
    expect(row.fontVariations, const [FontVariation('wght', 500)]);
    expect(row.color, HeyoColors.light.text);
    final strut = theme.strut(TextSize.row);
    expect((strut.fontSize, strut.forceStrutHeight), (15, true));
    expect(const HeyoThemeData(brightness: Brightness.dark, fallback: []).colors, same(HeyoColors.dark));
  });

  testWidgets('the views follow the appearance in the settings as it changes', (tester) async {
    final host = FakeHost({'get_settings': (_) => {'appearance': 'dark'}});
    late HeyoThemeData seen;
    await tester.pumpWidget(
      ThemeFollower(
        host: host,
        child: Builder(
          builder: (context) {
            seen = HeyoTheme.of(context);
            return const SizedBox();
          },
        ),
      ),
    );
    await tester.pump();
    expect(seen.isDark, isTrue);
    host.emit(HeyoEvent.settingsChanged, {'appearance': 'light'});
    await tester.pump();
    expect(seen.isDark, isFalse);
    // Back to the system's, which the test's window says is light.
    host.emit(HeyoEvent.settingsChanged, {'appearance': 'system'});
    tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
    await tester.pump();
    expect(seen.isDark, isTrue);
    tester.platformDispatcher.clearPlatformBrightnessTestValue();
  });

  testWidgets('what floats over the screen is dark whatever the settings say', (tester) async {
    final host = FakeHost({'get_settings': (_) => {'appearance': 'light'}});
    late HeyoThemeData seen;
    await tester.pumpWidget(
      ThemeFollower(
        host: host,
        always: Brightness.dark,
        child: Builder(
          builder: (context) {
            seen = HeyoTheme.of(context);
            return const SizedBox();
          },
        ),
      ),
    );
    await tester.pump();
    expect(seen.isDark, isTrue);
    // It does not even ask: the dictation pill may not read the settings.
    expect(host.invoked, isEmpty);
  });

  test('the user\'s own language is asked for first', () {
    expect(fonts.languages('zh_CN.UTF-8'), ['zh-cn', 'zh-tw', 'ja', 'ko']);
    expect(fonts.languages('zh_TW'), ['zh-tw', 'zh-cn', 'ja', 'ko']);
    expect(fonts.languages('ja_JP.UTF-8'), ['ja', 'zh-cn', 'zh-tw', 'ko']);
    expect(fonts.languages('ko-KR'), ['ko', 'zh-cn', 'zh-tw', 'ja']);
    expect(fonts.languages('en_US.UTF-8'), ['zh-cn', 'zh-tw', 'ja', 'ko']);
    expect(fonts.languages('C'), ['zh-cn', 'zh-tw', 'ja', 'ko']);
  });

  test('what the system answers is a list of faces, each once', () {
    expect(fonts.families(['Noto Sans CJK SC,Noto Sans CJK SC Regular\nDejaVu Sans\n', 'Noto Color Emoji\nDejaVu Sans\n\n']), ['Noto Sans CJK SC', 'DejaVu Sans', 'Noto Color Emoji']);
    expect(fonts.families(['']), isEmpty);
    expect(fonts.fallback, isNotEmpty);
  });

  test('the bundled face is the one the web view bundles, every weight in one file', () {
    final font = File('assets/fonts/Inter.ttf');
    expect(font.lengthSync(), greaterThan(500000));
    expect(File('pubspec.yaml').readAsStringSync(), contains('assets/fonts/Inter.ttf'));
  });

  test('the shadows are the pictures the web view lays around its windows', () {
    for (final name in ['window-2xl-light', 'window-2xl-dark', 'window-xl-light', 'window-xl-dark']) {
      expect(File('assets/shadows/$name.png').readAsBytesSync(), File('$repository/src/assets/shadows/$name.png').readAsBytesSync(), reason: '$name: run scripts/window-shadow.py');
    }
  });
}
