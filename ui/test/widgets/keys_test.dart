import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/widgets/keys.dart';

void main() {
  testWidgets('a key going down is a press; a key held is not', (tester) async {
    KeyPresses.instance.attach();
    final seen = <(Type, bool)>[];
    await tester.pumpWidget(
      Focus(
        autofocus: true,
        onKeyEvent: (node, event) {
          seen.add((event.runtimeType, isPress(event)));
          return KeyEventResult.handled;
        },
        child: const SizedBox(),
      ),
    );
    await tester.pump();

    await tester.sendKeyDownEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyRepeatEvent(LogicalKeyboardKey.enter);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.enter);
    expect(seen, [(KeyDownEvent, true), (KeyRepeatEvent, false), (KeyUpEvent, false)]);
  });

  testWidgets('the first press after the window comes back counts, though Flutter calls it a repeat', (tester) async {
    KeyPresses.instance.attach();
    final presses = <bool>[];
    await tester.pumpWidget(
      Focus(
        autofocus: true,
        onKeyEvent: (node, event) {
          if (event is! KeyUpEvent) presses.add(isPress(event));
          return KeyEventResult.handled;
        },
        child: const SizedBox(),
      ),
    );
    await tester.pump();

    // Escape goes down and hides the window, which never sees it go up.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.escape);
    KeyPresses.instance.windowShown();
    // Shown again, Escape is pressed: to Flutter the key was down all along.
    await tester.sendKeyRepeatEvent(LogicalKeyboardKey.escape);
    // And held.
    await tester.sendKeyRepeatEvent(LogicalKeyboardKey.escape);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.escape);
    expect(presses, [true, true, false]);
  });
}
