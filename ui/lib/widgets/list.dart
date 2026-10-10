// What lists are made of: a row, a section's heading, a key of the
// keyboard. (`.item`, `.section-title` and `kbd` in src/styles/components.)

import 'package:flutter/widgets.dart';

import '../theme/theme.dart';
import 'text.dart';

/// A row of a list: an icon, a title, what there is to say about it, and
/// at the end what kind of thing it is (or its accessories).
class ListRow extends StatelessWidget {
  const ListRow({super.key, this.leading, required this.title, this.subtitle, this.trailing, this.selected = false, this.marked = true, this.onTap});
  final Widget? leading;
  final String title;
  final String? subtitle;
  final Widget? trailing;
  final bool selected;

  /// Whether the row marks itself when selected. A list with a sliding
  /// `SelectionMark` marks the row with that instead.
  final bool marked;
  final VoidCallback? onTap;

  static const height = 38.0;

  @override
  Widget build(BuildContext context) {
    final colors = HeyoTheme.of(context).colors;
    final subtitle = this.subtitle;
    return GestureDetector(
      behavior: HitTestBehavior.opaque,
      onTap: onTap,
      child: Container(
        height: height,
        padding: EdgeInsets.symmetric(horizontal: space(2)),
        decoration: BoxDecoration(color: selected && marked ? colors.selected : null, borderRadius: BorderRadius.circular(Radii.lg)),
        child: LayoutBuilder(
          builder: (context, box) => Row(
            children: [
              if (leading != null) ...[leading!, SizedBox(width: space(3))],
              ConstrainedBox(
                constraints: BoxConstraints(maxWidth: box.maxWidth * 0.6),
                child: HeyoText(title, size: TextSize.row, weight: Weights.medium),
              ),
              SizedBox(width: space(3)),
              Expanded(
                child: subtitle == null || subtitle.isEmpty
                    ? const SizedBox()
                    // Fades out at the end rather than being cut off.
                    : HeyoText(subtitle, size: TextSize.row, color: colors.textMuted, fade: true),
              ),
              if (trailing != null) ...[SizedBox(width: space(3)), trailing!],
            ],
          ),
        ),
      ),
    );
  }
}

/// What a thing is, at the end of its row.
class KindLabel extends StatelessWidget {
  const KindLabel(this.kind, {super.key});
  final String kind;

  @override
  Widget build(BuildContext context) {
    return HeyoText(kind, size: TextSize.row, color: HeyoTheme.of(context).colors.textMuted);
  }
}

/// The heading of a section of a list.
class SectionTitle extends StatelessWidget {
  const SectionTitle(this.title, {super.key, this.subtitle});
  final String title;
  final String? subtitle;

  /// Its height: the padding above and below a line of `xs` text.
  static const height = 8.0 + 16.0 + 6.0;

  @override
  Widget build(BuildContext context) {
    final muted = HeyoTheme.of(context).colors.textMuted;
    return Padding(
      padding: EdgeInsets.fromLTRB(space(2), space(2), space(2), space(1.5)),
      child: Row(
        children: [
          HeyoText(title, size: TextSize.xs, color: muted),
          if (subtitle != null) ...[SizedBox(width: space(2)), Flexible(child: HeyoText(subtitle!, size: TextSize.xs, color: muted))],
        ],
      ),
    );
  }
}

/// A key of the keyboard: an outline, no more. A key, not a button.
class Keycap extends StatelessWidget {
  const Keycap(this.label, {super.key});
  final String label;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    return Container(
      constraints: const BoxConstraints(minWidth: 22),
      height: 22,
      padding: EdgeInsets.symmetric(horizontal: space(1)),
      decoration: BoxDecoration(borderRadius: BorderRadius.circular(Radii.sm), border: Border.all(color: colors.borderStrong)),
      child: Center(
        widthFactor: 1,
        child: Text(label, maxLines: 1, style: theme.text(TextSize.xs, weight: Weights.medium, color: colors.textMuted, tabular: true).copyWith(height: 1)),
      ),
    );
  }
}

/// The keys of a shortcut, side by side.
class Shortcut extends StatelessWidget {
  const Shortcut(this.keys, {super.key});
  final List<String> keys;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      for (final (index, key) in keys.indexed) ...[if (index > 0) const SizedBox(width: 2), Keycap(key)],
    ],
  );
}
