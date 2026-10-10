// A capture kept on screen (`PinWindow` in src/Screenshot.tsx): the picture,
// dragged to move, closed by Escape, a double click or a right click.

import 'dart:io';

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../host/window.dart';
import '../widgets/keys.dart';

class PinWindow extends StatelessWidget {
  const PinWindow({super.key, required this.path});

  /// The picture's file.
  final String path;

  @override
  Widget build(BuildContext context) {
    // Closing is the host's to do: it deletes the picture with the window.
    void close() => WindowScope.of(context).requestClose();
    return Focus(
      autofocus: true,
      onKeyEvent: (node, event) {
        if (!isPress(event) || event.logicalKey != LogicalKeyboardKey.escape) return KeyEventResult.ignored;
        close();
        return KeyEventResult.handled;
      },
      child: Listener(
        onPointerDown: (event) {
          if (event.buttons & kSecondaryMouseButton != 0) close();
        },
        child: GestureDetector(
          behavior: HitTestBehavior.opaque,
          onDoubleTap: close,
          // Dragging starts only once the pointer moves, so a double click
          // still closes.
          onPanStart: (_) => WindowScope.of(context).startMove(),
          child: path.isEmpty ? const SizedBox.expand() : Image.file(File(path), fit: BoxFit.fill, gaplessPlayback: true),
        ),
      ),
    );
  }
}
