// Which colour scheme the interface uses, and the text styles of the scale.
//
// The scheme is the system's, or the one chosen in the settings
// (`appearance`), followed as it changes. What floats over the screen (the
// screenshot tools, the dictation pill, a pinned capture) is dark whatever
// the scheme. This is `src/theme.ts` and the `color-scheme` rules of
// `base/_tokens.scss`, for Flutter.

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/commands.dart';
import '../host/events.g.dart';
import '../host/host.dart';
import '../host/models.dart';
import 'fonts.dart' as fonts;
import 'tokens.dart';

export 'tokens.dart';

/// What a view draws with: the colours of the scheme in use.
class HeyoThemeData {
  const HeyoThemeData({required this.brightness, required this.fallback});
  final Brightness brightness;

  /// The faces after Inter (fonts.dart).
  final List<String> fallback;

  bool get isDark => brightness == Brightness.dark;
  HeyoColors get colors => isDark ? HeyoColors.dark : HeyoColors.light;

  /// Text at a size of the scale: its size, line height and letter
  /// spacing. The colour is the text's unless given.
  TextStyle text(TextSize size, {FontWeight weight = Weights.regular, Color? color, bool tabular = false, bool mono = false}) => TextStyle(
    fontFamily: mono ? fonts.mono.first : fonts.sans,
    fontFamilyFallback: mono ? fonts.mono.sublist(1) : fallback,
    fontSize: size.size,
    height: size.line / size.size,
    letterSpacing: size.tracking * size.size,
    fontWeight: weight,
    // Inter is one file with every weight in it; this picks the weight.
    fontVariations: [FontVariation.weight(weight.value.toDouble())],
    fontFeatures: tabular ? const [FontFeature.tabularFigures()] : null,
    color: color ?? colors.text,
    decoration: TextDecoration.none,
    leadingDistribution: TextLeadingDistribution.even,
  );

  /// The line text of a size is set on, whatever fonts its characters come
  /// from: without it a line with Chinese in it is taller than one without.
  StrutStyle strut(TextSize size) =>
      StrutStyle(fontFamily: fonts.sans, fontSize: size.size, height: size.line / size.size, forceStrutHeight: true, leadingDistribution: TextLeadingDistribution.even);

  @override
  bool operator ==(Object other) => other is HeyoThemeData && other.brightness == brightness && other.fallback == fallback;

  @override
  int get hashCode => Object.hash(brightness, fallback);
}

/// Gives the views below it the theme.
class HeyoTheme extends InheritedWidget {
  const HeyoTheme({super.key, required this.data, required super.child});
  final HeyoThemeData data;

  static HeyoThemeData of(BuildContext context) =>
      context.dependOnInheritedWidgetOfExactType<HeyoTheme>()?.data ?? HeyoThemeData(brightness: Brightness.light, fallback: fonts.fallback);

  @override
  bool updateShouldNotify(HeyoTheme old) => old.data != data;
}

/// Which scheme an appearance means, given the system's.
Brightness schemeFor(Appearance appearance, Brightness system) => switch (appearance) {
  Appearance.light => Brightness.light,
  Appearance.dark => Brightness.dark,
  Appearance.system => system,
};

/// Keeps the views below it in the appearance the settings ask for, now and
/// as they change. With `always`, in that scheme whatever the settings say.
class ThemeFollower extends StatefulWidget {
  const ThemeFollower({super.key, required this.host, this.always, required this.child});
  final Host host;
  final Brightness? always;
  final Widget child;

  @override
  State<ThemeFollower> createState() => _ThemeFollowerState();
}

class _ThemeFollowerState extends State<ThemeFollower> with WidgetsBindingObserver {
  var _appearance = Appearance.system;
  var _fallback = fonts.fallback;
  StreamSubscription<Object?>? _changes;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    fonts.askSystem().then((changed) {
      if (changed && mounted) setState(() => _fallback = fonts.fallback);
    });
    if (widget.always != null) return;
    // A window that may not read the settings follows the system.
    widget.host.getSettings().then((settings) => _apply(settings.appearance)).catchError((Object _) {});
    _changes = widget.host.on(HeyoEvent.settingsChanged).listen((payload) {
      if (payload is Map) _apply(SettingsView.fromJson(payload.cast()).appearance);
    });
  }

  void _apply(Appearance appearance) {
    if (mounted && appearance != _appearance) setState(() => _appearance = appearance);
  }

  @override
  void didChangePlatformBrightness() => setState(() {});

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _changes?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final system = WidgetsBinding.instance.platformDispatcher.platformBrightness;
    final data = HeyoThemeData(brightness: widget.always ?? schemeFor(_appearance, system), fallback: _fallback);
    return HeyoTheme(
      data: data,
      child: DefaultTextStyle(style: data.text(TextSize.md), child: widget.child),
    );
  }
}
