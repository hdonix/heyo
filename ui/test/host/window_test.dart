import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/host/fake.dart';
import 'package:heyo_ui/host/window.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('heyo/window');
  late List<MethodCall> calls;
  late FakeHost host;
  late HeyoWindow window;

  /// The window, made inside the test: what it listens to must run in the
  /// test's own time.
  void attach(WidgetTester tester) {
    calls = [];
    host = FakeHost();
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call);
      if (call.method == 'geometry') {
        return {
          'window': {'x': 553, 'y': 273, 'width': 814, 'height': 534},
          'monitor': {'x': 0, 'y': 0, 'width': 1920, 'height': 1080},
          'workarea': {'x': 0, 'y': 27, 'width': 1920, 'height': 1053},
          'scale': 2.0,
        };
      }
      return null;
    });
    window = HeyoWindow(host, channel: channel);
    addTearDown(() {
      window.dispose();
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, null);
    });
  }

  /// As the runner calls Dart.
  Future<void> fromRunner(String method, [Object? arguments]) => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger.handlePlatformMessage(
    channel.name,
    const StandardMethodCodec().encodeMethodCall(MethodCall(method, arguments)),
    (_) {},
  );

  testWidgets('the host shows the window, and is told when it is drawn', (tester) async {
    attach(tester);
    host
      ..tell('center')
      ..tell('show')
      ..tell('focus');
    await tester.pumpAndSettle();
    expect(calls.map((call) => call.method), ['center', 'show', 'focus']);
    expect(window.visible.value, isTrue);
    expect(host.reported.single.$1, 'shown');
    expect(host.reported.single.$2['ms'], isA<int>());
    host.tell('hide');
    await tester.pump();
    expect(window.visible.value, isFalse);
    expect(calls.last.method, 'hide');
  });

  testWidgets('nothing is done to the window before Flutter has drawn once', (tester) async {
    final host = FakeHost();
    final calls = <String>[];
    tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, (call) async {
      calls.add(call.method);
      return null;
    });
    final drawn = Completer<void>();
    final window = HeyoWindow(host, channel: channel, ready: drawn.future);
    addTearDown(() {
      window.dispose();
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(channel, null);
    });
    // The host starts the overlay's process and covers the screen at once.
    host
      ..tell('overlay', {'x': 0, 'y': 0, 'width': 1920, 'height': 1080, 'backdrop': true})
      ..tell('hide');
    await tester.pump();
    expect(calls, isEmpty);
    drawn.complete();
    await tester.pumpAndSettle();
    // Then all of it, in the order it was said.
    expect(calls, ['cover', 'hide']);
  });

  testWidgets('the host places the window in its own pixels or in logical ones', (tester) async {
    attach(tester);
    host.tell('frame', {'x': 100, 'y': 50, 'width': 840, 'height': 104, 'logical': false});
    await tester.pump();
    expect(calls.single.method, 'setFrame');
    expect(calls.single.arguments, {'x': 100.0, 'y': 50.0, 'width': 840.0, 'height': 104.0, 'logical': false});
    host.tell('title', {'title': 'Heyo Settings'});
    await tester.pump();
    expect(calls.last.arguments, {'title': 'Heyo Settings'});
  });

  testWidgets('the host hears when the window gains and loses the keyboard', (tester) async {
    attach(tester);
    await fromRunner('focusChanged', true);
    expect(window.focused.value, isTrue);
    await fromRunner('focusChanged', true);
    await fromRunner('focusChanged', false);
    expect(window.focused.value, isFalse);
    // Once each: the host hides the palette on the second.
    expect(host.reported.map((report) => report.$1), ['focused', 'blurred']);
  });

  testWidgets('closing is asked of the host', (tester) async {
    attach(tester);
    await fromRunner('closeRequested');
    window.requestClose();
    expect(host.reported.map((report) => report.$1), ['close', 'close']);
    expect(calls, isEmpty);
  });

  testWidgets('covering a monitor says whether the capture is already under it', (tester) async {
    attach(tester);
    host.tell('overlay', {'x': 1920, 'y': 0, 'width': 1920, 'height': 1080, 'backdrop': true});
    await tester.pump();
    await tester.pump();
    expect(calls.single.method, 'cover');
    expect(window.covering.value!.area, const Rect.fromLTWH(1920, 0, 1920, 1080));
    expect(window.covering.value!.backdrop, isTrue);
    host.tell('hide');
    await tester.pump();
    expect(window.covering.value, isNull);
  });

  testWidgets('a view asks where the window is and shapes where it takes the pointer', (tester) async {
    attach(tester);
    final geometry = await window.geometry();
    expect(geometry!.window, const Rect.fromLTWH(553, 273, 814, 534));
    expect(geometry.workarea.top, 27);
    expect(geometry.scale, 2.0);
    await window.setInputRegion([const Rect.fromLTWH(135, 0, 150, 40)]);
    expect(calls.last.arguments, {
      'rects': [
        [135.0, 0.0, 150.0, 40.0],
      ],
    });
    await window.setInputRegion(null);
    expect(calls.last.arguments, {'rects': null});
    await window.configure(focus: false, above: true);
    expect(calls.last.arguments, {'focus': false, 'above': true});
  });
}
