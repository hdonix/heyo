// Live activities: a pill at the top of the screen that grows into a card
// under the pointer (docs/live-activities.md). A placeholder that shows the
// motion with a made-up timer, kept from the experiment that tried Flutter:
// nothing in Heyo starts this window yet.

import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../dev/bench.dart';
import '../host/window.dart';
import '../theme/theme.dart';

class IslandWindow extends StatefulWidget {
  const IslandWindow({super.key, this.bench = false});

  /// Open and close by itself a few times, and say how the frames went.
  final bool bench;

  @override
  State<IslandWindow> createState() => _IslandWindowState();
}

class _IslandWindowState extends State<IslandWindow> {
  static const _closed = Size(150, 34);
  static const _opened = Size(380, 132);
  static const _top = 6.0;
  static const _grow = Duration(milliseconds: 260);

  var _open = false;
  var _seconds = 25 * 60;
  Timer? _intent;
  Timer? _shrunk;
  late final Timer _clock;

  @override
  void initState() {
    super.initState();
    _clock = Timer.periodic(const Duration(seconds: 1), (_) => setState(() => _seconds = _seconds > 0 ? _seconds - 1 : 0));
    if (widget.bench) WidgetsBinding.instance.addPostFrameCallback((_) => _measure());
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    // Now, and whenever the window's size changes under the pill.
    MediaQuery.sizeOf(context);
    WidgetsBinding.instance.addPostFrameCallback((_) => _shape(_open ? _opened : _closed));
  }

  /// The window is larger than the pill, for the card to grow into: only
  /// the pill itself takes the pointer, and a click beside it goes to
  /// whatever is underneath.
  void _shape(Size pill) {
    if (!mounted) return;
    final width = MediaQuery.sizeOf(context).width;
    WindowScope.of(context).setInputRegion([Rect.fromLTWH((width - pill.width) / 2, _top, pill.width, pill.height)]);
  }

  void _setOpen(bool open) {
    setState(() => _open = open);
    _shrunk?.cancel();
    // Growing, the whole card takes the pointer at once; shrinking, until
    // it has shrunk.
    open ? _shape(_opened) : _shrunk = Timer(_grow, () => _shape(_closed));
  }

  @override
  void dispose() {
    _clock.cancel();
    _intent?.cancel();
    _shrunk?.cancel();
    super.dispose();
  }

  Future<void> _measure() async {
    await Future<void>.delayed(const Duration(milliseconds: 800));
    final frames = Frames()..start();
    for (var i = 0; i < 8; i++) {
      _setOpen(!_open);
      await Future<void>.delayed(const Duration(milliseconds: 500));
    }
    await frames.report('island opening and closing 8 times');
  }

  /// Opens after the pointer has rested a moment; closes a little later
  /// than it leaves.
  void _hover(bool inside) {
    _intent?.cancel();
    _intent = Timer(Duration(milliseconds: inside ? 300 : 600), () => _setOpen(inside));
  }

  String get _time => '${_seconds ~/ 60}:${(_seconds % 60).toString().padLeft(2, '0')}';

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final orange = HeyoColors.dark.raycast['orange']!;
    const dim = Color(0x99FFFFFF);
    return Align(
      alignment: Alignment.topCenter,
      child: MouseRegion(
        onEnter: (_) => _hover(true),
        onExit: (_) => _hover(false),
        child: AnimatedContainer(
          duration: _grow,
          curve: HeyoEasings.out,
          width: (_open ? _opened : _closed).width,
          height: (_open ? _opened : _closed).height,
          margin: const EdgeInsets.only(top: _top),
          padding: EdgeInsets.symmetric(horizontal: _open ? 18 : 12, vertical: _open ? 14 : 0),
          decoration: BoxDecoration(color: const Color(0xFF111111), borderRadius: BorderRadius.circular(_open ? 26 : 17)),
          clipBehavior: Clip.antiAlias,
          child: AnimatedSwitcher(
            duration: HeyoDurations.base,
            child: _open
                ? Column(
                    key: const ValueKey('open'),
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Icon(LucideIcons.timer, size: IconSizes.md, color: orange),
                          SizedBox(width: space(2)),
                          Text('Focus', style: theme.text(TextSize.xs, color: dim)),
                          const Spacer(),
                          Text(_time, style: theme.text(TextSize.xxl, weight: Weights.semibold, color: HeyoColors.white, tabular: true)),
                        ],
                      ),
                      const Spacer(),
                      Row(
                        children: [
                          for (final label in const ['Pause', '+1 min', 'End'])
                            Padding(
                              padding: EdgeInsets.only(right: space(2)),
                              child: Container(
                                padding: EdgeInsets.symmetric(horizontal: space(3), vertical: 5),
                                decoration: BoxDecoration(color: const Color(0x26FFFFFF), borderRadius: BorderRadius.circular(12)),
                                child: Text(label, style: theme.text(TextSize.xs, color: HeyoColors.white)),
                              ),
                            ),
                        ],
                      ),
                    ],
                  )
                : Row(
                    key: const ValueKey('closed'),
                    children: [
                      Icon(LucideIcons.timer, size: 15, color: orange),
                      const Spacer(),
                      Text(_time, style: theme.text(TextSize.sm, weight: Weights.semibold, color: HeyoColors.white, tabular: true)),
                    ],
                  ),
          ),
        ),
      ),
    );
  }
}
