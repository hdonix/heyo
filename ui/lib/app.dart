// The application: one window of Heyo's, and the view that belongs in it.
//
// Heyo starts a process for each window and says which with `--window`
// (`Launch.kind`). Each kind of window has its own directory under `lib/`,
// with one widget at its root that [viewFor] returns:
//
//   main        palette/     the root search, and the commands of extensions
//                            (extension/ draws what a command shows)
//   settings    settings/
//   dictation   dictation/   the pill that shows a dictation's progress
//   screenshot  screenshot/  the overlay a capture is selected on
//   pin         screenshot/  a capture kept on screen
//   island      island/      live activities
//
// A view gets the host and the window from the scopes above it
// (`HostScope.of`, `WindowScope.of`) and the theme from `HeyoTheme.of`.

import 'package:flutter/material.dart';

import 'dictation/dictation.dart';
import 'host/host.dart';
import 'host/scope.dart';
import 'host/window.dart';
import 'island/island.dart';
import 'palette/palette.dart';
import 'screenshot/pin.dart';
import 'screenshot/screenshot.dart';
import 'settings/settings.dart';
import 'theme/theme.dart';
import 'widgets/selection.dart';

/// The view of a kind of window.
Widget viewFor(Launch launch) => switch (launch.kind) {
  'settings' => const SettingsWindow(),
  'dictation' => const DictationWindow(),
  'screenshot' => const ScreenshotOverlay(),
  'pin' => PinWindow(path: launch.view['path'] as String? ?? ''),
  'island' => IslandWindow(bench: launch.bench == 'island'),
  _ => PaletteWindow(bench: launch.bench, benchExits: launch.standalone),
};

/// What floats over the screen itself is dark whatever the scheme; the rest
/// follows the settings.
bool floats(String kind) => const {'dictation', 'screenshot', 'pin', 'island'}.contains(kind);

class HeyoApp extends StatelessWidget {
  const HeyoApp({super.key, required this.host, required this.window, required this.launch, this.view});
  final Host host;
  final HeyoWindow window;
  final Launch launch;

  /// Shown instead of the view the launch asks for (tests).
  final Widget? view;

  @override
  Widget build(BuildContext context) {
    return HostScope(
      host: host,
      child: WindowScope(
        window: window,
        child: MaterialApp(
          debugShowCheckedModeBanner: false,
          title: launch.window['title'] as String? ?? 'Heyo',
          // The window is transparent wherever a view paints nothing.
          color: const Color(0x00000000),
          theme: ThemeData(useMaterial3: true, splashFactory: NoSplash.splashFactory, scaffoldBackgroundColor: const Color(0x00000000)),
          builder: (context, child) => ThemeFollower(
            host: host,
            always: floats(launch.kind) ? Brightness.dark : null,
            child: PointerScope(child: child!),
          ),
          home: Material(type: MaterialType.transparency, child: view ?? viewFor(launch)),
        ),
      ),
    );
  }
}
