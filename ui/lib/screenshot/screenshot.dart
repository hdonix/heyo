// The overlay a capture is selected on (`ScreenshotOverlay` in
// src/Screenshot.tsx). A placeholder: it says what it was sent and lets go
// of the screen again at any key or click (P pins a piece of nothing where
// the pointer is not, to show that a pinned capture's window opens). It
// holds the keyboard while it is up, so whatever replaces it must always
// offer that way out.

import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/commands.dart';
import '../host/events.g.dart';
import '../host/models.dart';
import '../host/scope.dart';
import '../theme/theme.dart';
import '../widgets/keys.dart';

class ScreenshotOverlay extends StatefulWidget {
  const ScreenshotOverlay({super.key});

  @override
  State<ScreenshotOverlay> createState() => _ScreenshotOverlayState();
}

class _ScreenshotOverlayState extends State<ScreenshotOverlay> {
  final _focus = FocusNode(debugLabel: 'screenshot');
  StreamSubscription<Object?>? _captures;
  ScreenshotCapture? _capture;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _captures ??= HostScope.of(context).on(HeyoEvent.screenshot).listen((payload) {
      if (payload is Map) setState(() => _capture = ScreenshotCapture.fromJson(payload.cast()));
      _focus.requestFocus();
    });
  }

  @override
  void dispose() {
    _captures?.cancel();
    _focus.dispose();
    super.dispose();
  }

  void _cancel() => HostScope.of(context).screenshotFinish(ScreenshotFinish(action: 'cancel')).catchError((Object _) => '');

  /// One grey pixel, as a PNG.
  static const _pixel = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mM8c+bMfwAHLgMJfsQFNQAAAABJRU5ErkJggg==';

  void _pin(ScreenshotCapture capture) => HostScope.of(context)
      .screenshotFinish(ScreenshotFinish(action: 'pin', png: _pixel, x: 120, y: 120, width: 320, height: 200, originX: capture.x, originY: capture.y, scale: capture.scale))
      .catchError((Object _) => '');

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final capture = _capture;
    return Focus(
      focusNode: _focus,
      autofocus: true,
      onKeyEvent: (node, event) {
        if (isPress(event)) {
          event.logicalKey == LogicalKeyboardKey.keyP && capture != null ? _pin(capture) : _cancel();
        }
        return KeyEventResult.handled;
      },
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: _cancel,
        child: Center(
          child: Container(
            padding: EdgeInsets.symmetric(horizontal: space(4), vertical: space(3)),
            decoration: BoxDecoration(color: HeyoColors.hud, borderRadius: BorderRadius.circular(Radii.lg)),
            child: Text(
              capture == null ? 'Screenshot' : 'Screenshot (${capture.mode}): ${capture.width} × ${capture.height}. Not drawn by Flutter yet; any key closes (P pins).',
              style: theme.text(TextSize.md, color: HeyoColors.white),
            ),
          ),
        ),
      ),
    );
  }
}
