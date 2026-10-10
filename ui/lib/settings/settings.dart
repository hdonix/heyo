// The settings window (src/Settings.tsx in the web view). A placeholder:
// the window, its panel and where it is dragged by; the pages are to come.

import 'package:flutter/widgets.dart';

import '../host/window.dart';
import '../theme/theme.dart';
import '../widgets/panel.dart';
import '../widgets/placeholder.dart';

class SettingsWindow extends StatelessWidget {
  const SettingsWindow({super.key});

  @override
  Widget build(BuildContext context) {
    final colors = HeyoTheme.of(context).colors;
    return WindowPanel(
      radius: PanelRadius.xl,
      color: colors.bgSolid,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // Drawn by Heyo itself, title bar and all: this is where the
          // window is dragged by.
          GestureDetector(
            behavior: HitTestBehavior.opaque,
            onPanStart: (_) => WindowScope.of(context).startMove(),
            child: const SizedBox(height: 44),
          ),
          const Expanded(child: ViewPlaceholder('Settings')),
        ],
      ),
    );
  }
}
