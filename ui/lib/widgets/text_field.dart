// A field one types in, and the one rule every key handler near a field
// must keep: while an input method is composing (pinyin, kana, …), the keys
// are its own. Enter commits the candidate, Escape cancels it, the arrows
// move among candidates. Flutter shows a key to the framework before the
// input method sees it, so a handler that acts on it takes it away.
//
// So: handle keys through [HeyoTextField.onKey] (or [keysUnlessComposing]),
// which is not asked while text is being composed. `composing(e)` in
// src/ime.ts is the same rule for the web view.

import 'package:flutter/material.dart';

import '../theme/theme.dart';

/// Whether an input method is composing in the field of that controller.
bool isComposing(TextEditingController controller) => controller.value.composing.isValid;

/// A key handler that leaves every key to the input method while it
/// composes in `controller`'s field, and asks `onKey` otherwise.
FocusOnKeyEventCallback keysUnlessComposing(TextEditingController controller, KeyEventResult Function(KeyEvent event) onKey) =>
    (node, event) => isComposing(controller) ? KeyEventResult.ignored : onKey(event);

class HeyoTextField extends StatelessWidget {
  const HeyoTextField({
    super.key,
    required this.controller,
    this.focusNode,
    this.placeholder,
    this.size = TextSize.md,
    this.autofocus = false,
    this.obscure = false,
    this.onChanged,
    this.onKey,
  });
  final TextEditingController controller;
  final FocusNode? focusNode;
  final String? placeholder;
  final TextSize size;
  final bool autofocus;

  /// A password: what is typed is not shown.
  final bool obscure;

  /// The text changed. While an input method composes, this is called with
  /// the text as it stands, the composition included.
  final ValueChanged<String>? onChanged;

  /// A key was pressed (or repeated) in the field, and no input method is
  /// composing. Return `handled` to keep the key from the field.
  final KeyEventResult Function(KeyEvent event)? onKey;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    final style = theme.text(size);
    final field = TextField(
      controller: controller,
      focusNode: focusNode,
      autofocus: autofocus,
      obscureText: obscure,
      onChanged: onChanged,
      maxLines: 1,
      style: style,
      strutStyle: theme.strut(size),
      cursorColor: colors.accent,
      cursorWidth: 1.5,
      // Drawn by Heyo, not by Material: no underline, no padding of its own.
      decoration: InputDecoration.collapsed(hintText: placeholder, hintStyle: style.copyWith(color: colors.textFaint)),
      enableInteractiveSelection: true,
      // One line: Enter is the view's to act on, not the field's.
      textInputAction: TextInputAction.none,
      // (Material would give up the keyboard focus on Enter.)
      onEditingComplete: () {},
    );
    return TextSelectionTheme(
      data: TextSelectionThemeData(cursorColor: colors.accent, selectionColor: colors.accentSoft),
      child: onKey == null ? field : Focus(canRequestFocus: false, skipTraversal: true, onKeyEvent: keysUnlessComposing(controller, onKey!), child: field),
    );
  }
}
