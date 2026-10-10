// What pops up over a view: a toast, a dialog that asks, and the shell of
// the action panel. (src/styles/components/_footer.scss and _overlays.scss.)

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../theme/theme.dart';
import 'keys.dart';
import 'list.dart';
import 'text.dart';

/// Menus and dialogs: an edge, a close shadow and a distant one.
List<BoxShadow> overlayShadow(HeyoColors colors) => [
  BoxShadow(color: colors.borderStrong, spreadRadius: 1),
  BoxShadow(color: colors.shadowNear, offset: const Offset(0, 4), blurRadius: 12),
  BoxShadow(color: colors.shadowFar, offset: const Offset(0, 16), blurRadius: 40),
];

/// What floats over the end of a list: the actions, a toast.
List<BoxShadow> pillShadow(HeyoColors colors) => [
  BoxShadow(color: colors.border, spreadRadius: 1),
  BoxShadow(color: colors.shadowNear, offset: const Offset(0, 2), blurRadius: 8),
  BoxShadow(color: colors.shadowNear, offset: const Offset(0, 6), blurRadius: 20),
];

/// Something one clicks, with the states of the web view's `interactive`
/// mixin: resting, hovered, pressed.
class Pressable extends StatefulWidget {
  const Pressable({super.key, required this.onPressed, required this.builder, this.rest, this.hover, this.active});
  final VoidCallback? onPressed;

  /// Builds the thing on the fill of its state.
  final Widget Function(BuildContext context, Color fill) builder;
  final Color? rest;
  final Color? hover;
  final Color? active;

  @override
  State<Pressable> createState() => _PressableState();
}

class _PressableState extends State<Pressable> {
  var _hovered = false;
  var _pressed = false;

  @override
  Widget build(BuildContext context) {
    final colors = HeyoTheme.of(context).colors;
    final enabled = widget.onPressed != null;
    final fill = !enabled
        ? (widget.rest ?? const Color(0x00000000))
        : _pressed
        ? (widget.active ?? colors.fill3)
        : _hovered
        ? (widget.hover ?? colors.fill2)
        : (widget.rest ?? const Color(0x00000000));
    return MouseRegion(
      cursor: enabled ? SystemMouseCursors.click : MouseCursor.defer,
      onEnter: (_) => setState(() => _hovered = true),
      onExit: (_) => setState(() => _hovered = false),
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTapDown: enabled ? (_) => setState(() => _pressed = true) : null,
        onTapCancel: () => setState(() => _pressed = false),
        onTapUp: (_) => setState(() => _pressed = false),
        onTap: widget.onPressed,
        child: Opacity(opacity: enabled ? 1 : 0.45, child: widget.builder(context, fill)),
      ),
    );
  }
}

// --- Toast --------------------------------------------------------------------

enum ToastStyle { success, failure, animated }

/// What the view has to say, in a pill at the start of the footer.
class Toast extends StatelessWidget {
  const Toast({super.key, required this.title, this.message, this.style = ToastStyle.success});
  final String title;
  final String? message;
  final ToastStyle style;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    return Container(
      height: 38,
      padding: EdgeInsets.only(left: space(3), right: space(4)),
      decoration: BoxDecoration(color: colors.surfaceRaised, borderRadius: BorderRadius.circular(Radii.full), boxShadow: pillShadow(colors)),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (style == ToastStyle.animated) _Spinner(track: colors.fill4, color: colors.accent) else _dot(style == ToastStyle.failure ? colors.danger : colors.success),
          SizedBox(width: space(2)),
          // One line, cut off where the pill ends.
          Flexible(
            child: Text.rich(
              TextSpan(
                text: title,
                style: theme.text(TextSize.sm, weight: Weights.medium),
                children: [
                  if (message != null && message!.isNotEmpty) TextSpan(text: '  ${message!}', style: theme.text(TextSize.sm, color: colors.textMuted)),
                ],
              ),
              maxLines: 1,
              softWrap: false,
              overflow: TextOverflow.ellipsis,
              strutStyle: theme.strut(TextSize.sm),
            ),
          ),
        ],
      ),
    );
  }

  Widget _dot(Color color) => Container(width: 8, height: 8, decoration: BoxDecoration(color: color, shape: BoxShape.circle));
}

class _Spinner extends StatefulWidget {
  const _Spinner({required this.track, required this.color});
  final Color track;
  final Color color;

  @override
  State<_Spinner> createState() => _SpinnerState();
}

class _SpinnerState extends State<_Spinner> with SingleTickerProviderStateMixin {
  late final _turn = AnimationController(vsync: this, duration: const Duration(milliseconds: 800))..repeat();

  @override
  void dispose() {
    _turn.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => RotationTransition(
    turns: _turn,
    child: CustomPaint(size: const Size.square(12), painter: _Ring(widget.track, widget.color)),
  );
}

class _Ring extends CustomPainter {
  _Ring(this.track, this.color);
  final Color track;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final stroke = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2;
    final box = (Offset.zero & size).deflate(1);
    canvas.drawOval(box, stroke..color = track);
    // A quarter of it, at the top, in the accent.
    canvas.drawArc(box, -3.14159 * 0.75, 3.14159 / 2, false, stroke..color = color);
  }

  @override
  bool shouldRepaint(_Ring old) => old.track != track || old.color != color;
}

// --- A dialog that asks ---------------------------------------------------------

class DialogChoice {
  const DialogChoice(this.title, {this.primary = false, this.destructive = false});
  final String title;

  /// The one filled button, which Enter chooses.
  final bool primary;
  final bool destructive;
}

/// A question over the view, which dims: a title, what it is about, and the
/// answers. Enter chooses the primary (or destructive) answer, Escape
/// dismisses (`onChoose(null)`). Lay it over the panel's contents in a
/// `Stack`.
class ConfirmDialog extends StatefulWidget {
  const ConfirmDialog({super.key, required this.title, this.message, required this.choices, required this.onChoose, this.radius = Radii.xxl, this.child});
  final String title;
  final String? message;
  final List<DialogChoice> choices;

  /// The index of the answer chosen, or null when dismissed.
  final ValueChanged<int?> onChoose;

  /// The corner of the panel it covers.
  final double radius;

  /// Something more between the message and the answers (a field).
  final Widget? child;

  @override
  State<ConfirmDialog> createState() => _ConfirmDialogState();
}

class _ConfirmDialogState extends State<ConfirmDialog> {
  final _focus = FocusNode(debugLabel: 'dialog');

  @override
  void initState() {
    super.initState();
    _focus.requestFocus();
  }

  @override
  void dispose() {
    _focus.dispose();
    super.dispose();
  }

  KeyEventResult _key(FocusNode node, KeyEvent event) {
    if (!isPress(event)) return KeyEventResult.ignored;
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      widget.onChoose(null);
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter || event.logicalKey == LogicalKeyboardKey.numpadEnter) {
      final chosen = widget.choices.indexWhere((choice) => choice.primary || choice.destructive);
      if (chosen >= 0) widget.onChoose(chosen);
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    final stacked = widget.choices.length > 2;
    final buttons = [
      for (final (index, choice) in widget.choices.indexed) _button(theme, choice, () => widget.onChoose(index)),
    ];
    return Focus(
      focusNode: _focus,
      onKeyEvent: _key,
      child: GestureDetector(
        behavior: HitTestBehavior.opaque,
        onTap: () {},
        child: DecoratedBox(
          decoration: BoxDecoration(color: colors.scrim, borderRadius: BorderRadius.circular(widget.radius)),
          child: Center(
            child: Container(
              width: 320,
              padding: EdgeInsets.all(space(5)),
              decoration: BoxDecoration(color: colors.surfaceRaised, borderRadius: BorderRadius.circular(Radii.xl), boxShadow: overlayShadow(colors)),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  HeyoText(widget.title, size: TextSize.lg, weight: Weights.semibold, maxLines: null, align: TextAlign.center),
                  if (widget.message != null && widget.message!.isNotEmpty)
                    Padding(
                      padding: EdgeInsets.only(top: space(2)),
                      child: HeyoText(widget.message!, size: TextSize.sm, color: colors.textMuted, maxLines: null, align: TextAlign.center),
                    ),
                  ?widget.child,
                  SizedBox(height: space(5)),
                  if (stacked)
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        for (final (index, button) in buttons.indexed) ...[if (index > 0) SizedBox(height: space(2)), button],
                      ],
                    )
                  else
                    Row(
                      children: [
                        for (final (index, button) in buttons.indexed) ...[if (index > 0) SizedBox(width: space(2)), Expanded(child: button)],
                      ],
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _button(HeyoThemeData theme, DialogChoice choice, VoidCallback onPressed) {
    final colors = theme.colors;
    final (ink, rest, hover, active) = choice.destructive
        ? (HeyoColors.white, colors.danger, Color.lerp(colors.danger, const Color(0xFF000000), 0.10)!, Color.lerp(colors.danger, const Color(0xFF000000), 0.18)!)
        : choice.primary
        ? (colors.onAccent, colors.accent, Color.lerp(colors.accent, colors.tint, 0.10)!, Color.lerp(colors.accent, colors.tint, 0.18)!)
        : (colors.text, colors.fill2, colors.fill3, colors.fill4);
    return Pressable(
      onPressed: onPressed,
      rest: rest,
      hover: hover,
      active: active,
      builder: (context, fill) => Container(
        height: Heights.lg,
        alignment: Alignment.center,
        decoration: BoxDecoration(color: fill, borderRadius: BorderRadius.circular(Radii.md)),
        child: HeyoText(choice.title, size: TextSize.sm, weight: Weights.medium, color: ink),
      ),
    );
  }
}

// --- The action panel -----------------------------------------------------------

/// The shell of the action panel: a floating box at the end of the view,
/// above the pill of actions it opens from, holding a title, the rows, and
/// a field to search them at the bottom. What the rows are and what the
/// keys do is the view's.
class ActionPanel extends StatelessWidget {
  const ActionPanel({super.key, this.title, required this.rows, this.search, this.onDismiss});
  final String? title;

  /// The rows (`ActionRow`), with `ActionDivider` between sections.
  final List<Widget> rows;

  /// The field under the rows.
  final Widget? search;

  /// A click outside the panel.
  final VoidCallback? onDismiss;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    return GestureDetector(
      behavior: HitTestBehavior.translucent,
      onTap: onDismiss,
      child: Padding(
        padding: EdgeInsets.only(left: space(2), right: space(2), bottom: 54),
        child: Align(
          alignment: Alignment.bottomRight,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: () {},
            child: Container(
              width: 340,
              constraints: const BoxConstraints(maxHeight: 340),
              decoration: BoxDecoration(color: colors.surfaceRaised, borderRadius: BorderRadius.circular(Radii.lg), boxShadow: overlayShadow(colors)),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  if (title != null)
                    Padding(
                      padding: EdgeInsets.fromLTRB(space(3), space(3), space(3), space(1)),
                      child: HeyoText(title!, size: TextSize.xxs, weight: Weights.semibold, color: colors.textMuted),
                    ),
                  Flexible(
                    child: ListView(padding: EdgeInsets.all(space(1)), shrinkWrap: true, children: rows),
                  ),
                  if (search != null)
                    Container(
                      height: 40,
                      padding: EdgeInsets.symmetric(horizontal: space(3)),
                      alignment: Alignment.centerLeft,
                      decoration: BoxDecoration(border: Border(top: BorderSide(color: colors.divider))),
                      child: search,
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// A row of the action panel.
class ActionRow extends StatelessWidget {
  const ActionRow({super.key, this.leading, required this.title, this.shortcut, this.selected = false, this.destructive = false, this.onTap});
  final Widget? leading;
  final String title;
  final List<String>? shortcut;
  final bool selected;
  final bool destructive;
  final VoidCallback? onTap;

  static const height = 32.0;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    final ink = destructive ? colors.danger : colors.text;
    final fill = !selected ? null : (destructive ? colors.danger.withValues(alpha: 0.14) : colors.selected);
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: Container(
        height: height,
        padding: EdgeInsets.symmetric(horizontal: space(2)),
        decoration: BoxDecoration(color: fill, borderRadius: BorderRadius.circular(Radii.md)),
        child: Row(
          children: [
            if (leading != null) ...[IconTheme(data: IconThemeData(color: ink), child: leading!), SizedBox(width: space(2))],
            Expanded(child: HeyoText(title, color: ink)),
            if (shortcut != null) ...[SizedBox(width: space(2)), Shortcut(shortcut!)],
          ],
        ),
      ),
    );
  }
}

/// Between two sections of the action panel.
class ActionDivider extends StatelessWidget {
  const ActionDivider({super.key});

  @override
  Widget build(BuildContext context) => Container(
    margin: EdgeInsets.symmetric(vertical: space(1)),
    height: 1,
    color: HeyoTheme.of(context).colors.divider,
  );
}
