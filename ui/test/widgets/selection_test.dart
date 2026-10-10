// src/selection.test.ts, for the Dart port.

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/widgets/selection.dart';

void main() {
  test('a wrapping step joins the ends', () {
    expect(step(0, 1, 3, wrap: true), 1);
    expect(step(2, 1, 3, wrap: true), 0);
    expect(step(0, -1, 3, wrap: true), 2);
    expect(step(1, -8, 3, wrap: true), 2);
  });

  test('a step that does not wrap stops at the ends', () {
    expect(step(1, 8, 5, wrap: false), 4);
    expect(step(1, -8, 5, wrap: false), 0);
    expect(step(2, 1, 5, wrap: false), 3);
  });

  test('an empty list has nothing to step to', () {
    expect(step(0, 1, 0, wrap: true), 0);
    expect(step(3, -1, 0, wrap: false), 0);
  });

  test('the pointer selects a row only by really moving', () {
    final watch = PointerWatch();
    expect(watch.moved(const PointerHoverEvent(position: Offset(10, 10))), isTrue);
    // The list scrolled under a pointer that is still: an event for the row
    // now under it, at the same place.
    expect(watch.moved(const PointerHoverEvent(position: Offset(10, 10))), isFalse);
    expect(watch.moved(const PointerHoverEvent(position: Offset(10, 14))), isTrue);
  });

  test('the last of pointer and keyboard to act is the one driving', () {
    final watch = PointerWatch();
    watch.moved(const PointerHoverEvent(position: Offset(20, 20)));
    expect(watch.pointerDriven, isTrue);
    watch.keyPressed();
    expect(watch.pointerDriven, isFalse);
    // A still pointer does not take over from the keyboard.
    watch.moved(const PointerHoverEvent(position: Offset(20, 20)));
    expect(watch.pointerDriven, isFalse);
    watch.moved(const PointerHoverEvent(position: Offset(21, 20)));
    expect(watch.pointerDriven, isTrue);
    watch.keyPressed();
    // Turning the wheel is the pointer's doing too.
    watch.scrolled();
    expect(watch.pointerDriven, isTrue);
  });

  test('a list scrolls only as far as it takes to show the row', () {
    const view = (top: 100.0, bottom: 500.0);
    const padding = (top: 32.0, bottom: 56.0);
    // In view, clear of the padding: nothing to do.
    expect(shortfall((top: 200, bottom: 238), view, padding), 0);
    // Under the actions that float over the end of the list.
    expect(shortfall((top: 430, bottom: 468), view, padding), 24);
    // Above, under a section heading's space.
    expect(shortfall((top: 110, bottom: 148), view, padding), -22);
    // Far below: just far enough, not to the middle.
    expect(shortfall((top: 900, bottom: 938), view, padding), 494);
  });

  testWidgets('revealing a row moves the list at once, and no further than its ends', (tester) async {
    final scroll = ScrollController();
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Center(
          child: SizedBox(
            height: 400,
            child: ListView.builder(controller: scroll, itemExtent: 38, itemCount: 100, itemBuilder: (_, index) => Text('row $index')),
          ),
        ),
      ),
    );
    const padding = (top: 32.0, bottom: 56.0);
    // Row 3 is in view.
    expect(reveal(scroll.position, (top: 114, bottom: 152), padding: padding), 0);
    // Row 12 ends at 494: 150 past the view less its padding.
    expect(reveal(scroll.position, (top: 456, bottom: 494), padding: padding), 150);
    expect(scroll.offset, 150);
    // Back to the first: as far as the list goes, which is less than asked.
    expect(reveal(scroll.position, (top: 0, bottom: 38), padding: padding), -150);
    expect(scroll.offset, 0);
  });

  testWidgets('a row under the pointer is selected when the pointer moves, and keys take over', (tester) async {
    final chosen = <int>[];
    late PointerWatch watch;
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: PointerScope(
          child: Builder(
            builder: (context) {
              watch = PointerScope.of(context);
              return Column(
                children: [
                  for (final index in [0, 1]) UnderPointer(onSelect: () => chosen.add(index), child: const SizedBox(width: 200, height: 38)),
                ],
              );
            },
          ),
        ),
      ),
    );
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(find.byType(UnderPointer).last));
    await tester.pump();
    expect(chosen, [1]);
    expect(watch.pointerDriven, isTrue);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.arrowDown);
    expect(watch.pointerDriven, isFalse);
    await mouse.moveTo(tester.getCenter(find.byType(UnderPointer).first));
    await tester.pump();
    expect(chosen, [1, 0]);
  });
}
