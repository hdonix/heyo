// This process's window: what the host tells it to do, what the view may do
// with it, and what the host is told about it.
//
// Everything that is the toolkit's business happens in the platform's
// runner, behind the method channel `heyo/window` (Linux:
// linux/runner/heyo_window.cc). A runner for another platform offers the
// same methods:
//
//   configure {title?, width?, height?, focus?, above?, taskbar?, type?, resizable?}
//   show, hide, focus, center, close, startMove
//   setFrame {x?, y?, width?, height?, logical}
//   geometry -> {window, monitor, workarea: {x, y, width, height}, scale, visible, focused}
//   setInputRegion {rects: [[x, y, width, height], …]?}
//   cover {x, y, width, height}
//
// and calls back `focusChanged(bool)` and `closeRequested()`.
//
// The host decides when the window shows and hides (it must know whether
// the palette is up to toggle it), so a view never hides its own window:
// it invokes `hide`, as the web view does.

import 'dart:async';

import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../widgets/keys.dart';
import 'host.dart';

/// Where the window, its monitor and that monitor's usable area are, in
/// logical pixels, and how many physical pixels each of those is.
class WindowGeometry {
  WindowGeometry({required this.window, required this.monitor, required this.workarea, required this.scale});
  final Rect window;
  final Rect monitor;
  final Rect workarea;
  final double scale;
}

/// The screenshot overlay was put over a monitor.
class Covering {
  Covering({required this.area, required this.backdrop});

  /// The rectangle covered, in logical pixels.
  final Rect area;

  /// Whether the capture is already shown underneath, by a window of the
  /// host's. If not, the view paints it.
  final bool backdrop;
}

class HeyoWindow {
  /// `ready` completes when Flutter has drawn its first frame; until then
  /// nothing the host says is done to the window. (A window put on screen
  /// before that stays empty for good: the engine never learns its size.)
  HeyoWindow(this.host, {MethodChannel? channel, Future<void>? ready}) : _channel = channel ?? const MethodChannel('heyo/window') {
    _channel.setMethodCallHandler(_fromRunner);
    _doing = ready ?? Future<void>.value();
    // One thing at a time, in the order the host said them.
    _control = host.control.listen((message) => _doing = _doing.then((_) => _fromHost(message)));
  }

  final Host host;
  final MethodChannel _channel;
  late final StreamSubscription<Map<String, Object?>> _control;
  late Future<void> _doing;

  /// Whether the window has the keyboard.
  final focused = ValueNotifier<bool>(false);

  /// Whether the host has the window on screen.
  final visible = ValueNotifier<bool>(false);

  /// Set each time the host puts the window over a monitor (`cover`).
  final covering = ValueNotifier<Covering?>(null);

  void dispose() {
    _control.cancel();
    _channel.setMethodCallHandler(null);
  }

  Future<T?> _call<T>(String method, [Map<String, Object?>? args]) async {
    try {
      return await _channel.invokeMethod<T>(method, args);
    } on MissingPluginException {
      // No runner: a widget test.
      return null;
    }
  }

  /// Says what kind of window this is, before it is first shown. Only what
  /// is given changes.
  Future<void> configure({String? title, double? width, double? height, bool? focus, bool? above, bool? taskbar, String? type, bool? resizable}) => _call('configure', {
    'title': ?title,
    'width': ?width,
    'height': ?height,
    'focus': ?focus,
    'above': ?above,
    'taskbar': ?taskbar,
    'type': ?type,
    'resizable': ?resizable,
  });

  /// As the host described the window when it started the process.
  Future<void> configureFrom(Map<String, Object?> spec) => _call('configure', spec);

  /// Moves or sizes the window; what is left out stays. Logical pixels.
  Future<void> setFrame({double? x, double? y, double? width, double? height}) =>
      _call('setFrame', {'x': ?x, 'y': ?y, 'width': ?width, 'height': ?height, 'logical': true});

  Future<void> center() => _call('center');

  Future<WindowGeometry?> geometry() async {
    final map = await _call<Map<Object?, Object?>>('geometry');
    if (map == null) return null;
    Rect rect(Object? value) {
      final r = (value as Map?)?.cast<String, Object?>() ?? const {};
      double side(String name) => (r[name] as num?)?.toDouble() ?? 0;
      return Rect.fromLTWH(side('x'), side('y'), side('width'), side('height'));
    }

    return WindowGeometry(window: rect(map['window']), monitor: rect(map['monitor']), workarea: rect(map['workarea']), scale: (map['scale'] as num?)?.toDouble() ?? 1);
  }

  /// The parts of the window that take the pointer (logical pixels); a
  /// click anywhere else goes to what is under the window. `null`: all of
  /// it does.
  Future<void> setInputRegion(List<Rect>? rects) => _call('setInputRegion', {
    'rects': rects == null ? null : [for (final r in rects) [r.left, r.top, r.width, r.height]],
  });

  /// The pointer went down on something that drags the window.
  Future<void> startMove() => _call('startMove');

  /// Asks the host to close the window, as Alt+F4 does: a window that
  /// lives as long as Heyo only hides, any other ends.
  void requestClose() => host.report('close');

  Future<void> _fromRunner(MethodCall call) async {
    switch (call.method) {
      case 'focusChanged':
        final now = call.arguments == true;
        if (now != focused.value) {
          focused.value = now;
          host.report(now ? 'focused' : 'blurred');
        }
      case 'closeRequested':
        host.report('close');
    }
  }

  Future<void> _fromHost(Map<String, Object?> message) async {
    double? number(String name) => (message[name] as num?)?.toDouble();
    switch (message['window']) {
      case 'show':
        final asked = Stopwatch()..start();
        KeyPresses.instance.windowShown();
        visible.value = true;
        await _call('show');
        // Hidden, nothing asked for a frame; shown, one is due at once.
        SchedulerBinding.instance.scheduleFrame();
        await SchedulerBinding.instance.endOfFrame;
        host.report('shown', {'ms': asked.elapsedMilliseconds});
      case 'hide':
        visible.value = false;
        covering.value = null;
        await _call('hide');
      case 'focus':
        await _call('focus');
      case 'center':
        await _call('center');
      case 'frame':
        await _call('setFrame', {'x': number('x'), 'y': number('y'), 'width': number('width'), 'height': number('height'), 'logical': message['logical'] != false});
      case 'title':
        await _call('configure', {'title': message['title']});
      case 'overlay':
        final area = Rect.fromLTWH(number('x') ?? 0, number('y') ?? 0, number('width') ?? 0, number('height') ?? 0);
        KeyPresses.instance.windowShown();
        visible.value = true;
        await _call('cover', {'x': area.left, 'y': area.top, 'width': area.width, 'height': area.height});
        covering.value = Covering(area: area, backdrop: message['backdrop'] == true);
        SchedulerBinding.instance.scheduleFrame();
      case 'close':
        await _call('close');
    }
  }
}

/// Gives the views below it the window.
class WindowScope extends InheritedWidget {
  const WindowScope({super.key, required this.window, required super.child});
  final HeyoWindow window;

  static HeyoWindow of(BuildContext context) => context.dependOnInheritedWidgetOfExactType<WindowScope>()!.window;

  @override
  bool updateShouldNotify(WindowScope old) => old.window != window;
}
