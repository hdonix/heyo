// Which row of a list is selected, and how that changes: by the arrow
// keys, or by the pointer moving over a row.
//
// The root search, an extension's list or grid, and the action panel are
// all lists of this kind. What they share lives here: how a step lands,
// when the selection is scrolled into view, and when the pointer counts as
// choosing a row. (src/selection.ts and src/pointer.ts, for Flutter.)

import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../theme/theme.dart';

/// Where a step of `delta` rows from `index` lands among `count` rows.
/// Wrapping, the ends meet (a short list walked with the arrow keys);
/// otherwise the step stops at the ends (a long list, a page at a time).
int step(int index, int delta, int count, {required bool wrap}) {
  if (count <= 0) return 0;
  if (!wrap) return (index + delta).clamp(0, count - 1);
  return (((index + delta) % count) + count) % count;
}

/// A stretch along the scrolling axis: a row, or the view.
typedef Span = ({double top, double bottom});

/// How far a list must scroll for a row to be inside its view, less the
/// padding kept clear at each end: negative is up, zero is "it is in view".
/// (All in the same coordinates.)
double shortfall(Span row, Span view, Span padding) {
  if (row.top < view.top + padding.top) return row.top - (view.top + padding.top);
  if (row.bottom > view.bottom - padding.bottom) return row.bottom - (view.bottom - padding.bottom);
  return 0;
}

/// Scrolls just far enough to show `row` (in the coordinates of the list's
/// contents), at once, and returns how far it went. `padding` is kept
/// clear: the room for a section's heading above, the actions that float
/// over the end of the list below.
double reveal(ScrollPosition position, Span row, {Span padding = (top: 0, bottom: 0)}) {
  if (!position.hasContentDimensions) return 0;
  final view = (top: position.pixels, bottom: position.pixels + position.viewportDimension);
  final by = shortfall(row, view, padding);
  if (by == 0) return 0;
  final to = (position.pixels + by).clamp(position.minScrollExtent, position.maxScrollExtent);
  final went = to - position.pixels;
  if (went != 0) position.jumpTo(to);
  return went;
}

/// Whether the pointer or the keyboard is driving.
///
/// Lists select the row under the pointer. But a list that scrolls to keep
/// the selection in view moves rows under a pointer that is still, and
/// acting on that makes the selection chase the scrolling. So rows select
/// only when the pointer really moved, and a list scrolls its selection
/// into view only when the keyboard moved it.
class PointerWatch {
  Offset? _at;
  bool _pointer = false;

  /// Notes a pointer event; true if the pointer is somewhere it was not.
  bool moved(PointerEvent event) {
    if (event.position == _at) return false;
    _at = event.position;
    _pointer = true;
    return true;
  }

  /// Turning the wheel is the pointer's doing too, though nothing moves.
  void scrolled() => _pointer = true;

  void keyPressed() => _pointer = false;

  /// Whether what last changed things was the pointer (rather than a key).
  bool get pointerDriven => _pointer;
}

/// Watches the pointer and the keyboard for everything below it; rows ask
/// it whether the pointer chose them (`PointerScope.of(context)`).
class PointerScope extends StatefulWidget {
  const PointerScope({super.key, required this.child});
  final Widget child;

  static PointerWatch of(BuildContext context) => context.findAncestorStateOfType<_PointerScopeState>()?.watch ?? PointerWatch();

  @override
  State<PointerScope> createState() => _PointerScopeState();
}

class _PointerScopeState extends State<PointerScope> {
  final watch = PointerWatch();

  bool _key(KeyEvent event) {
    if (event is KeyDownEvent) watch.keyPressed();
    return false;
  }

  @override
  void initState() {
    super.initState();
    HardwareKeyboard.instance.addHandler(_key);
  }

  @override
  void dispose() {
    HardwareKeyboard.instance.removeHandler(_key);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Listener(
    behavior: HitTestBehavior.translucent,
    onPointerSignal: (event) {
      if (event is PointerScrollEvent) watch.scrolled();
    },
    child: widget.child,
  );
}

/// Lets the pointer select what is below it: only by really moving.
class UnderPointer extends StatelessWidget {
  const UnderPointer({super.key, required this.onSelect, required this.child});
  final VoidCallback onSelect;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final watch = PointerScope.of(context);
    return MouseRegion(
      opaque: false,
      onHover: (event) {
        if (watch.moved(event)) onSelect();
      },
      child: child,
    );
  }
}

/// What marks the selected row of a list: one piece that slides from row to
/// row rather than a background each row turns on. Put it first in a
/// `Stack` that scrolls with the rows; `top` and `height` are the selected
/// row's, among the list's contents. It slides for the keyboard; under the
/// pointer, and when the list has just changed (`slide` false), it is
/// simply there.
class SelectionMark extends StatelessWidget {
  const SelectionMark({super.key, required this.top, required this.height, this.visible = true, this.slide = true});
  final double top;
  final double height;
  final bool visible;
  final bool slide;

  @override
  Widget build(BuildContext context) {
    final colors = HeyoTheme.of(context).colors;
    return AnimatedPositioned(
      duration: slide ? HeyoDurations.fast : Duration.zero,
      curve: HeyoEasings.out,
      top: top,
      height: height,
      left: space(2),
      right: space(2),
      child: IgnorePointer(
        child: AnimatedOpacity(
          duration: HeyoDurations.instant,
          opacity: visible ? 1 : 0,
          child: DecoratedBox(decoration: BoxDecoration(color: colors.selected, borderRadius: BorderRadius.circular(Radii.lg))),
        ),
      ),
    );
  }
}
