// The pill that shows a dictation's progress (src/Dictation.tsx in the web
// view). A placeholder: the pill, saying what the host last told it.

import 'dart:async';

import 'package:flutter/widgets.dart';

import '../host/events.g.dart';
import '../host/scope.dart';
import '../theme/theme.dart';

class DictationWindow extends StatefulWidget {
  const DictationWindow({super.key});

  @override
  State<DictationWindow> createState() => _DictationWindowState();
}

class _DictationWindowState extends State<DictationWindow> {
  StreamSubscription<Object?>? _events;
  var _phase = 'Dictation';

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _events ??= HostScope.of(context).on(HeyoEvent.dictation).listen((event) {
      if (event is Map && event['type'] is String) setState(() => _phase = event['type'] as String);
    });
  }

  @override
  void dispose() {
    _events?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    return Padding(
      padding: const EdgeInsets.all(4),
      child: DecoratedBox(
        decoration: BoxDecoration(color: HeyoColors.hud, borderRadius: BorderRadius.circular(Radii.full)),
        child: Center(child: Text(_phase, style: theme.text(TextSize.sm, color: HeyoColors.white))),
      ),
    );
  }
}
