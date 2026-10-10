// A window of Heyo's on the desktop: the panel, and the shadow it casts
// into the transparent margin its window keeps around it.
//
// The shadow is a picture laid around the panel (scripts/window-shadow.py
// draws it, for both interfaces), stretched between its corners. The panel
// has the palette's translucent background, a hairline edge, and the edge
// light catches along its top. (`.floating-window #root` and `.panel` in
// src/styles/components/_palette.scss.)

import 'package:flutter/widgets.dart';

import '../theme/theme.dart';

/// Which of the two corner radii the window has.
enum PanelRadius {
  /// The palette.
  xxl('2xl', Radii.xxl),

  /// The settings window.
  xl('xl', Radii.xl);

  const PanelRadius(this.token, this.value);
  final String token;
  final double value;
}

class WindowPanel extends StatelessWidget {
  const WindowPanel({super.key, this.radius = PanelRadius.xxl, this.color, this.onTapOutside, required this.child});
  final PanelRadius radius;

  /// The panel's background: the palette's, unless given.
  final Color? color;

  /// A click in the margin is a click outside the window.
  final VoidCallback? onTapOutside;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    // The pictures are drawn at twice the size; a corner is the margin and
    // the radius, and between the corners there is a little to stretch.
    final corner = windowMargin + radius.value;
    final scheme = theme.isDark ? 'dark' : 'light';
    final shape = BorderRadius.circular(radius.value);
    return GestureDetector(
      behavior: HitTestBehavior.translucent,
      onTapDown: onTapOutside == null ? null : (_) => onTapOutside!(),
      child: DecoratedBox(
        decoration: BoxDecoration(
          image: DecorationImage(
            image: ExactAssetImage('assets/shadows/window-${radius.token}-$scheme.png', scale: 2),
            centerSlice: Rect.fromLTRB(corner, corner, corner + 4, corner + 4),
            fit: BoxFit.fill,
          ),
        ),
        child: Padding(
          // The hairline is outside the panel, as a box-shadow's spread is.
          padding: const EdgeInsets.all(windowMargin - 1),
          child: GestureDetector(
            // A click on the panel is not one outside it.
            behavior: HitTestBehavior.opaque,
            onTapDown: (_) {},
            child: DecoratedBox(
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(radius.value + 1),
                border: Border.all(color: colors.shadowHairline),
              ),
              child: Padding(
                padding: const EdgeInsets.all(1),
                child: ClipRRect(
                  borderRadius: shape,
                  child: CustomPaint(
                    foregroundPainter: _TopLight(colors.highlight, radius.value),
                    child: ColoredBox(color: color ?? colors.bg, child: child),
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// `inset 0 1px 0`: one pixel of light along the top edge, round the corners.
class _TopLight extends CustomPainter {
  _TopLight(this.color, this.radius);
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final box = RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(radius));
    final edge = Path.combine(PathOperation.difference, Path()..addRRect(box), Path()..addRRect(box.shift(const Offset(0, 1))));
    canvas.drawPath(edge, Paint()..color = color);
  }

  @override
  bool shouldRepaint(_TopLight old) => old.color != color || old.radius != radius;
}
