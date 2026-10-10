// Telling a key going down from a key held down.
//
// A view that acts once on a key (Enter opens, Escape closes) acts on
// `KeyDownEvent` and leaves `KeyRepeatEvent` alone. But Heyo's windows come
// and go under the user's hands: Escape hides the palette while the key is
// still down, and the window never sees it go up. When the window is next
// shown, Flutter believes Escape is still held, and reports the next press
// of it as a repeat. A view that asked `event is KeyDownEvent` would ignore
// the first Escape, or the first Enter, after every show.
//
// So: ask [isPress] instead. It remembers which keys the window has seen go
// down since it last came up.

import 'package:flutter/services.dart';

class KeyPresses {
  KeyPresses._();
  static final instance = KeyPresses._();

  final _held = <PhysicalKeyboardKey>{};
  KeyEvent? _latest;
  var _latestIsFirst = false;

  /// Starts watching the keyboard. (Without it, a press is a `KeyDownEvent`
  /// and nothing else.) Calling it again does no harm.
  void attach() {
    HardwareKeyboard.instance
      ..removeHandler(_note)
      ..addHandler(_note);
  }

  bool _note(KeyEvent event) {
    _latest = event;
    if (event is KeyUpEvent) {
      _held.remove(event.physicalKey);
      _latestIsFirst = false;
    } else {
      _latestIsFirst = _held.add(event.physicalKey);
    }
    return false;
  }

  /// The window came up: whatever keys it saw go down before it went away
  /// have long been let go.
  void windowShown() => _held.clear();

  bool isPress(KeyEvent event) => event is KeyDownEvent || (event is KeyRepeatEvent && identical(event, _latest) && _latestIsFirst);
}

/// Whether a key event is the key going down, rather than repeating while
/// held or going up.
bool isPress(KeyEvent event) => KeyPresses.instance.isPress(event);
