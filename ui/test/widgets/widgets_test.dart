import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/theme/theme.dart';
import 'package:heyo_ui/widgets/list.dart';
import 'package:heyo_ui/widgets/overlays.dart';
import 'package:heyo_ui/widgets/panel.dart';
import 'package:heyo_ui/widgets/text.dart';
import 'package:heyo_ui/widgets/text_field.dart';

const _light = HeyoThemeData(brightness: Brightness.light, fallback: []);

Widget _app(Widget child, {HeyoThemeData theme = _light}) => MaterialApp(
  home: HeyoTheme(
    data: theme,
    child: Material(type: MaterialType.transparency, child: Center(child: child)),
  ),
);

void main() {
  testWidgets('a row is as tall as the web view\'s and marks itself when selected', (tester) async {
    await tester.pumpWidget(_app(const SizedBox(width: 700, child: ListRow(title: 'Firefox', subtitle: 'Web Browser', trailing: KindLabel('Application'), selected: true))));
    expect(tester.getSize(find.byType(ListRow)).height, 38);
    final box = tester.widget<Container>(find.descendant(of: find.byType(ListRow), matching: find.byType(Container)).first);
    expect((box.decoration! as BoxDecoration).color, HeyoColors.light.selected);
    expect(find.text('Firefox'), findsOneWidget);
    expect(find.text('Web Browser'), findsOneWidget);
    expect(find.text('Application'), findsOneWidget);

    // With a sliding mark, the row leaves the marking to it.
    await tester.pumpWidget(_app(const SizedBox(width: 700, child: ListRow(title: 'Firefox', selected: true, marked: false))));
    final unmarked = tester.widget<Container>(find.descendant(of: find.byType(ListRow), matching: find.byType(Container)).first);
    expect((unmarked.decoration! as BoxDecoration).color, isNull);
  });

  testWidgets('a long title leaves room for what follows it', (tester) async {
    await tester.pumpWidget(_app(SizedBox(width: 500, child: ListRow(title: 'A title ' * 30, subtitle: 'subtitle', trailing: const KindLabel('Command')))));
    expect(tester.takeException(), isNull);
    expect(tester.getSize(find.text('A title ' * 30)).width, lessThanOrEqualTo(500 * 0.6));
  });

  testWidgets('text keeps its line whatever fonts its characters come from', (tester) async {
    await tester.pumpWidget(_app(const Column(mainAxisSize: MainAxisSize.min, children: [HeyoText('Latin', size: TextSize.row), HeyoText('中文 日本語 한국어', size: TextSize.row)])));
    final lines = tester.widgetList<Text>(find.byType(Text)).toList();
    expect(lines.every((line) => line.strutStyle!.forceStrutHeight == true), isTrue);
    expect(tester.getSize(find.text('Latin')).height, tester.getSize(find.text('中文 日本語 한국어')).height);
    expect(tester.getSize(find.text('Latin')).height, 20);
  });

  testWidgets('a heading, a key and a shortcut have the sizes of the web view\'s', (tester) async {
    await tester.pumpWidget(_app(const SizedBox(width: 400, child: SectionTitle('Favorites'))));
    expect(tester.getSize(find.byType(SectionTitle)).height, SectionTitle.height);
    await tester.pumpWidget(_app(const Row(mainAxisSize: MainAxisSize.min, children: [Keycap('↵'), Keycap('Ctrl')])));
    expect(tester.getSize(find.byType(Keycap).first).height, 22);
    expect(tester.getSize(find.byType(Keycap).first).width, closeTo(22, 0.5));
    expect(tester.getSize(find.byType(Keycap).last).width, greaterThan(22));
    await tester.pumpWidget(_app(const Shortcut(['Ctrl', 'K'])));
    expect(find.byType(Keycap), findsNWidgets(2));
  });

  testWidgets('a window\'s panel sits inside the margin its shadow falls in', (tester) async {
    var outside = 0;
    await tester.pumpWidget(_app(SizedBox(width: 814, height: 534, child: WindowPanel(onTapOutside: () => outside++, child: const SizedBox.expand(key: Key('inside'))))));
    final inside = tester.getRect(find.byKey(const Key('inside')));
    final window = tester.getRect(find.byType(WindowPanel));
    expect(inside, window.deflate(windowMargin));
    // A click in the margin is a click outside; one on the panel is not.
    await tester.tapAt(window.topLeft + const Offset(10, 10));
    expect(outside, 1);
    await tester.tapAt(window.center);
    expect(outside, 1);
    final shadow = tester.widget<DecoratedBox>(find.descendant(of: find.byType(WindowPanel), matching: find.byType(DecoratedBox)).first).decoration as BoxDecoration;
    expect((shadow.image!.image as ExactAssetImage).assetName, 'assets/shadows/window-2xl-light.png');
    // The corners of the picture are the margin and the radius.
    expect(shadow.image!.centerSlice, const Rect.fromLTRB(54, 54, 58, 58));
  });

  testWidgets('the settings window has the smaller corner and the dark shadow in the dark', (tester) async {
    const dark = HeyoThemeData(brightness: Brightness.dark, fallback: []);
    await tester.pumpWidget(_app(const SizedBox(width: 400, height: 300, child: WindowPanel(radius: PanelRadius.xl, child: SizedBox.expand())), theme: dark));
    final shadow = tester.widget<DecoratedBox>(find.descendant(of: find.byType(WindowPanel), matching: find.byType(DecoratedBox)).first).decoration as BoxDecoration;
    expect((shadow.image!.image as ExactAssetImage).assetName, 'assets/shadows/window-xl-dark.png');
    expect(shadow.image!.centerSlice, const Rect.fromLTRB(48, 48, 52, 52));
  });

  testWidgets('a toast says how it went', (tester) async {
    await tester.pumpWidget(_app(const Toast(title: 'Copied', message: 'to the clipboard')));
    expect(tester.getSize(find.byType(Toast)).height, 38);
    expect(find.textContaining('Copied  to the clipboard', findRichText: true), findsOneWidget);
    await tester.pumpWidget(_app(const Toast(title: 'Working', style: ToastStyle.animated)));
    // The spinner turns: there is always another frame.
    expect(tester.hasRunningAnimations, isTrue);
  });

  testWidgets('a dialog is answered by its buttons, by Enter and by Escape', (tester) async {
    final answers = <int?>[];
    Widget dialog() => _app(
      SizedBox(
        width: 750,
        height: 470,
        child: ConfirmDialog(
          title: 'Paste 3 lines into the terminal?',
          message: 'Each line runs as a command.',
          choices: const [DialogChoice('Cancel'), DialogChoice('Paste', primary: true)],
          onChoose: answers.add,
        ),
      ),
    );
    await tester.pumpWidget(dialog());
    await tester.pump();
    await tester.tap(find.text('Cancel'));
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    expect(answers, [0, 1, null]);
    // Two answers side by side, each as tall as a control.
    expect(tester.getTopLeft(find.text('Cancel')).dy, tester.getTopLeft(find.text('Paste')).dy);
  });

  testWidgets('three answers stand one under the other', (tester) async {
    await tester.pumpWidget(
      _app(
        SizedBox(
          width: 750,
          height: 470,
          child: ConfirmDialog(title: 'Allow?', choices: const [DialogChoice('Always', primary: true), DialogChoice('Once'), DialogChoice('Never', destructive: true)], onChoose: (_) {}),
        ),
      ),
    );
    expect(tester.getTopLeft(find.text('Once')).dy, greaterThan(tester.getTopLeft(find.text('Always')).dy));
    expect(tester.getTopLeft(find.text('Never')).dy, greaterThan(tester.getTopLeft(find.text('Once')).dy));
  });

  testWidgets('the action panel opens from the end of the view', (tester) async {
    var dismissed = 0;
    await tester.pumpWidget(
      _app(
        SizedBox(
          width: 750,
          height: 470,
          child: ActionPanel(
            title: 'Firefox',
            onDismiss: () => dismissed++,
            rows: const [ActionRow(title: 'Open Application', shortcut: ['↵'], selected: true), ActionDivider(), ActionRow(title: 'Remove', destructive: true)],
          ),
        ),
      ),
    );
    final view = tester.getRect(find.byType(ActionPanel));
    final panel = tester.getRect(find.text('Firefox')).expandToInclude(tester.getRect(find.text('Remove')));
    expect(panel.right, lessThan(view.right));
    expect(panel.left, greaterThan(view.right - 340 - 8));
    expect(tester.getSize(find.byType(ActionRow).first).height, 32);
    await tester.tapAt(view.topLeft + const Offset(20, 20));
    expect(dismissed, 1);
    await tester.tap(find.text('Remove'));
    expect(dismissed, 1);
  });

  group('a field', () {
    late TextEditingController controller;
    late List<LogicalKeyboardKey> keys;

    Future<void> pump(WidgetTester tester) async {
      controller = TextEditingController();
      keys = [];
      await tester.pumpWidget(
        _app(
          SizedBox(
            width: 400,
            child: HeyoTextField(
              controller: controller,
              autofocus: true,
              placeholder: 'Search…',
              size: TextSize.xl,
              onKey: (event) {
                if (event is! KeyDownEvent) return KeyEventResult.ignored;
                keys.add(event.logicalKey);
                return event.logicalKey == LogicalKeyboardKey.enter ? KeyEventResult.handled : KeyEventResult.ignored;
              },
            ),
          ),
        ),
      );
      await tester.pump();
    }

    testWidgets('shows its placeholder until something is typed', (tester) async {
      await pump(tester);
      expect(find.text('Search…'), findsOneWidget);
      await tester.enterText(find.byType(TextField), 'se');
      expect(controller.text, 'se');
    });

    testWidgets('hands its keys to the view', (tester) async {
      await pump(tester);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      expect(keys, [LogicalKeyboardKey.arrowDown, LogicalKeyboardKey.enter]);
      // Enter was the view's: the field still has the keyboard.
      expect(tester.testTextInput.hasAnyClients, isTrue);
    });

    testWidgets('keeps every key from the view while an input method composes', (tester) async {
      await pump(tester);
      // Pinyin being typed: "ce shi" underlined, candidates showing.
      tester.testTextInput.updateEditingValue(const TextEditingValue(text: 'ce shi', selection: TextSelection.collapsed(offset: 6), composing: TextRange(start: 0, end: 6)));
      await tester.pump();
      expect(isComposing(controller), isTrue);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      expect(keys, isEmpty);
      // The candidate is committed: the keys are the view's again.
      tester.testTextInput.updateEditingValue(const TextEditingValue(text: '测试', selection: TextSelection.collapsed(offset: 2)));
      await tester.pump();
      expect(isComposing(controller), isFalse);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      expect(keys, [LogicalKeyboardKey.enter]);
      expect(controller.text, '测试');
    });
  });
}
