// The icon at the start of a row, of one size and weight whatever it is.
//
// In the root search, as in Raycast: an application's own icon as it comes,
// a command's symbol in white on a tile of its own colour, and where there
// is no icon the initial of the title. Inside a command's own list, symbols
// are plain: there they are labels, not identities. (src/ItemIcon.tsx.)

import 'package:flutter/widgets.dart';

import '../theme/theme.dart';
import 'image.dart';

const _tileColors = ['blue', 'green', 'magenta', 'orange', 'purple', 'red', 'yellow'];

/// The same colour for the same name, every time: the name of one of
/// Raycast's colours.
String tileColorName(String name) {
  var hash = 0;
  for (final rune in name.runes) {
    hash = (hash * 31 + rune) & 0xFFFFFFFF;
  }
  return _tileColors[hash % _tileColors.length];
}

enum ItemIconVariant { tile, plain }

class ItemIcon extends StatelessWidget {
  const ItemIcon({super.key, required this.icon, required this.title, this.variant = ItemIconVariant.tile, this.colorKey, this.selected = false});

  /// A Raycast `ImageLike`, or nothing.
  final Object? icon;
  final String title;
  final ItemIconVariant variant;

  /// What the tile's colour is chosen by, when not the title.
  final String? colorKey;

  /// Whether the row it starts is the selected one (a plain symbol reads
  /// stronger there).
  final bool selected;

  static const size = 22.0;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    final tile = colors.raycast[tileColorName(colorKey ?? title)]!;
    final icon = this.icon;
    if (icon == null || icon == '') {
      final trimmed = title.trim();
      final initial = trimmed.isEmpty ? '?' : String.fromCharCode(trimmed.runes.first).toUpperCase();
      return Container(
        width: size,
        height: size,
        alignment: Alignment.center,
        decoration: BoxDecoration(color: tile.withValues(alpha: 0.18), borderRadius: BorderRadius.circular(Radii.sm)),
        child: Text(initial, maxLines: 1, style: theme.text(TextSize.xs, weight: Weights.semibold, color: tile)),
      );
    }
    if (!isSymbol(icon)) return HeyoImage(icon, size: size);
    if (variant == ItemIconVariant.plain) {
      return SizedBox(
        width: size,
        height: size,
        child: Center(child: HeyoImage(icon, size: IconSizes.lg, color: selected ? colors.text : colors.textMuted)),
      );
    }
    // A command: its symbol in white on its own colour, lit from above.
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      foregroundDecoration: BoxDecoration(
        borderRadius: BorderRadius.circular(Radii.sm),
        border: Border.all(color: const Color.fromRGBO(0, 0, 0, 0.12), width: 0.5),
      ),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(Radii.sm),
        gradient: LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [Color.lerp(tile, HeyoColors.white, 0.14)!, tile]),
        boxShadow: [BoxShadow(color: colors.shadowNear, offset: const Offset(0, 1), blurRadius: 1)],
      ),
      child: HeyoImage(icon, size: 15, color: HeyoColors.white),
    );
  }
}
