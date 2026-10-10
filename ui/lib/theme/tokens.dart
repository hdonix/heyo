// The design tokens: the scales everything is coloured, sized, spaced and
// timed by. They are the web view's (src/styles/abstracts/_scales.scss and
// base/_tokens.scss) written out in Dart; test/theme/tokens_test.dart reads
// that stylesheet and fails when the two differ. Views take colours from
// `HeyoTheme.of(context).colors` and sizes from here, never values of
// their own.

import 'package:flutter/animation.dart';
import 'package:flutter/painting.dart';

/// The colours of one scheme.
class HeyoColors {
  const HeyoColors({
    required this.tint,
    required this.text,
    required this.textMuted,
    required this.textFaint,
    required this.bg,
    required this.bgSolid,
    required this.surface,
    required this.surfaceRaised,
    required this.card,
    required this.highlight,
    required this.border,
    required this.borderStrong,
    required this.divider,
    required this.accent,
    required this.onAccent,
    required this.danger,
    required this.success,
    required this.warning,
    required this.scrim,
    required this.fill1,
    required this.fill2,
    required this.fill3,
    required this.fill4,
    required this.shadowHairline,
    required this.shadowNear,
    required this.shadowFar,
    required this.raycast,
  });

  /// What fills are tinted with: ink on paper, light on dark.
  final Color tint;
  final Color text;
  final Color textMuted;

  /// Quieter still: placeholders, the kind of an item.
  final Color textFaint;

  /// Surfaces, from the back forward. The palette is slightly translucent.
  final Color bg;
  final Color bgSolid;
  final Color surface;
  final Color surfaceRaised;

  /// A card on the solid background: paper on light, a lift on dark.
  final Color card;

  /// The edge light catches along the top of a raised thing.
  final Color highlight;
  final Color border;
  final Color borderStrong;

  /// Between the rows of a list: barely there.
  final Color divider;
  final Color accent;
  final Color onAccent;
  final Color danger;
  final Color success;
  final Color warning;
  final Color scrim;

  /// Fills: hover, pressed, selected, controls. The tint at rising strengths.
  final Color fill1;
  final Color fill2;
  final Color fill3;
  final Color fill4;

  /// What shadows are made of, from the faintest up.
  final Color shadowHairline;
  final Color shadowNear;
  final Color shadowFar;

  /// The colours extensions name (Raycast's `Color`): `blue`, `green`, …
  final Map<String, Color> raycast;

  /// The selected row of a list.
  Color get selected => fill3;

  /// The inside of a field.
  Color get input => fill2;

  /// `color-mix(in srgb, accent 16%, transparent)`.
  Color get accentSoft => accent.withValues(alpha: accent.a * 0.16);

  /// The focus ring: the accent at 28 %.
  Color get ring => accent.withValues(alpha: accent.a * 0.28);

  static const white = Color(0xFFFFFFFF);

  /// What floats over the screen itself, in either scheme.
  static const hud = Color.fromRGBO(20, 20, 22, 0.9);

  static const light = HeyoColors(
    tint: Color(0xFF000000),
    text: Color(0xFF1A1918),
    textMuted: Color(0xFF76726F),
    textFaint: Color(0xFFA5A09D),
    bg: Color.fromRGBO(245, 242, 241, 0.985),
    bgSolid: Color(0xFFFFFFFF),
    surface: Color(0xFFFFFFFF),
    surfaceRaised: Color(0xFFFFFFFF),
    card: Color(0xFFF7F6F5),
    highlight: Color.fromRGBO(255, 255, 255, 0.9),
    border: Color.fromRGBO(0, 0, 0, 0.08),
    borderStrong: Color.fromRGBO(0, 0, 0, 0.16),
    divider: Color.fromRGBO(0, 0, 0, 0.06),
    accent: Color(0xFFD9471F),
    onAccent: Color(0xFFFFFFFF),
    danger: Color(0xFFD92D20),
    success: Color(0xFF188A52),
    warning: Color(0xFFA86A00),
    scrim: Color.fromRGBO(0, 0, 0, 0.2),
    fill1: Color.fromRGBO(30, 20, 15, 0.035),
    fill2: Color.fromRGBO(30, 20, 15, 0.055),
    fill3: Color.fromRGBO(30, 20, 15, 0.09),
    fill4: Color.fromRGBO(30, 20, 15, 0.14),
    shadowHairline: Color.fromRGBO(0, 0, 0, 0.12),
    shadowNear: Color.fromRGBO(0, 0, 0, 0.1),
    shadowFar: Color.fromRGBO(0, 0, 0, 0.26),
    raycast: {
      'blue': Color(0xFF0A7FD6),
      'green': Color(0xFF188A52),
      'magenta': Color(0xFFD1309A),
      'orange': Color(0xFFD2601A),
      'purple': Color(0xFF8A48D6),
      'red': Color(0xFFD93636),
      'yellow': Color(0xFFA86A00),
      'brown': Color(0xFF8A5A2B),
    },
  );

  static const dark = HeyoColors(
    tint: Color(0xFFFFFFFF),
    text: Color(0xFFF4F3F2),
    textMuted: Color(0xFFA39F9C),
    textFaint: Color(0xFF76726F),
    bg: Color.fromRGBO(30, 28, 28, 0.975),
    bgSolid: Color(0xFF1C1B1B),
    surface: Color(0xFF2B2929),
    surfaceRaised: Color(0xFF322F2F),
    card: Color(0xFF262424),
    highlight: Color.fromRGBO(255, 255, 255, 0.07),
    border: Color.fromRGBO(255, 255, 255, 0.09),
    borderStrong: Color.fromRGBO(255, 255, 255, 0.18),
    divider: Color.fromRGBO(255, 255, 255, 0.06),
    accent: Color(0xFFFF6B3D),
    onAccent: Color(0xFF1C1210),
    danger: Color(0xFFFF453A),
    success: Color(0xFF59D499),
    warning: Color(0xFFFFC531),
    scrim: Color.fromRGBO(0, 0, 0, 0.4),
    fill1: Color.fromRGBO(255, 250, 245, 0.04),
    fill2: Color.fromRGBO(255, 250, 245, 0.065),
    fill3: Color.fromRGBO(255, 250, 245, 0.1),
    fill4: Color.fromRGBO(255, 250, 245, 0.16),
    shadowHairline: Color.fromRGBO(0, 0, 0, 0.6),
    shadowNear: Color.fromRGBO(0, 0, 0, 0.3),
    shadowFar: Color.fromRGBO(0, 0, 0, 0.55),
    raycast: {
      'blue': Color(0xFF56C2FF),
      'green': Color(0xFF59D499),
      'magenta': Color(0xFFFF63C3),
      'orange': Color(0xFFFF8C42),
      'purple': Color(0xFFC88CFF),
      'red': Color(0xFFFF6363),
      'yellow': Color(0xFFFFC531),
      'brown': Color(0xFFC8935A),
    },
  );

  /// The colours by the names the stylesheet gives them, for the test that
  /// compares the two.
  Map<String, Color> get named => {
    'tint': tint,
    'text': text,
    'text-muted': textMuted,
    'text-faint': textFaint,
    'bg': bg,
    'bg-solid': bgSolid,
    'surface': surface,
    'surface-raised': surfaceRaised,
    'card': card,
    'highlight': highlight,
    'border': border,
    'border-strong': borderStrong,
    'divider': divider,
    'accent': accent,
    'on-accent': onAccent,
    'danger': danger,
    'success': success,
    'warning': warning,
    'scrim': scrim,
    'fill-1': fill1,
    'fill-2': fill2,
    'fill-3': fill3,
    'fill-4': fill4,
    'shadow-hairline': shadowHairline,
    'shadow-near': shadowNear,
    'shadow-far': shadowFar,
    for (final entry in raycast.entries) 'raycast-${entry.key}': entry.value,
  };
}

/// A 4 px grid. `space(3)` is 12; halves are for the inside of small
/// controls only.
double space(double steps) => steps * 4;

/// A size of text: the size, the line height it is set on (which keeps text
/// on the grid) and how its letters are spaced, in ems.
class TextSize {
  const TextSize(this.name, this.size, this.line, this.tracking);
  final String name;
  final double size;
  final double line;
  final double tracking;

  static const xxs = TextSize('2xs', 11, 16, 0.02);
  static const xs = TextSize('xs', 12, 16, 0.01);
  static const sm = TextSize('sm', 13, 18, 0);
  static const md = TextSize('md', 14, 20, -0.006);

  /// Rows of the palette: the size things are read at.
  static const row = TextSize('row', 15, 20, -0.009);
  static const lg = TextSize('lg', 15, 22, -0.01);
  static const xl = TextSize('xl', 18, 24, -0.014);
  static const xxl = TextSize('2xl', 22, 28, -0.018);

  static const all = [xxs, xs, sm, md, row, lg, xl, xxl];
}

abstract final class Weights {
  static const regular = FontWeight.w400;
  static const medium = FontWeight.w500;
  static const semibold = FontWeight.w600;

  static const named = {'regular': 400, 'medium': 500, 'semibold': 600};
}

/// Nested corners follow "outer = inner + the gap between them".
abstract final class Radii {
  static const xs = 4.0;
  static const sm = 6.0;
  static const md = 8.0;
  static const lg = 10.0;
  static const xl = 16.0;

  /// The palette itself.
  static const xxl = 22.0;
  static const full = 999.0;

  static const named = {'xs': xs, 'sm': sm, 'md': md, 'lg': lg, 'xl': xl, '2xl': xxl, 'full': full};
}

/// How tall things one clicks are.
abstract final class Heights {
  static const sm = 24.0;
  static const md = 28.0;
  static const lg = 32.0;
  static const row = 40.0;

  static const named = {'sm': sm, 'md': md, 'lg': lg, 'row': row};
}

abstract final class IconSizes {
  static const sm = 14.0;
  static const md = 16.0;
  static const lg = 18.0;

  /// An application's icon, or the tile a command's icon sits in.
  static const tile = 24.0;

  static const named = {'sm': sm, 'md': md, 'lg': lg, 'tile': tile};
}

abstract final class HeyoDurations {
  static const instant = Duration(milliseconds: 80);
  static const fast = Duration(milliseconds: 120);
  static const base = Duration(milliseconds: 160);
  static const slow = Duration(milliseconds: 240);

  static const named = {'instant': instant, 'fast': fast, 'base': base, 'slow': slow};
}

abstract final class HeyoEasings {
  /// Things arriving and moving: quick off the mark, settling gently.
  static const out = Cubic(0.2, 0, 0, 1);

  /// Things leaving.
  static const in_ = Cubic(0.4, 0, 1, 1);

  /// A little past and back, for what pops up.
  static const spring = Cubic(0.34, 1.3, 0.64, 1);

  static const named = {'out': out, 'in': in_, 'spring': spring};
}

/// The transparent margin a window of Heyo's keeps around its panel for the
/// shadow to fall in.
const windowMargin = 32.0;
