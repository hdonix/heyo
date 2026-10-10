// heyo-ui: one of Heyo's windows.
//
//   heyo-ui --window=main|settings|dictation|screenshot|pin|island
//           [--label=…] [--init=JSON]      as Heyo starts it
//   heyo-ui --standalone --window=…        by hand, without Heyo: the view
//                                          is given made-up answers
//   … --bench=keys|scroll                  measure frames, print, exit
//
// See docs/flutter-ui.md.

import 'dart:async';
import 'dart:io';

import 'package:flutter/widgets.dart';

import 'app.dart';
import 'dev/standalone.dart';
import 'host/host.dart';
import 'host/window.dart';
import 'widgets/keys.dart';

void main(List<String> args) {
  final launch = Launch.parse(args);
  // The host reads this process's stdout as messages: nothing else may be
  // written there. What Dart prints goes to stderr, which joins Heyo's log.
  runZoned(() => _run(launch), zoneSpecification: ZoneSpecification(print: (self, parent, zone, line) => stderr.writeln(line)));
}

Future<void> _run(Launch launch) async {
  WidgetsFlutterBinding.ensureInitialized();
  KeyPresses.instance.attach();
  final Host host = launch.standalone ? standaloneHost() : HostConnection.stdio();
  // What the host tells the window to do waits until Flutter has drawn
  // once (see HeyoWindow): the host may ask at once, and usually does.
  final drawn = Completer<void>();
  final window = HeyoWindow(host, ready: drawn.future);
  await window.configureFrom(launch.standalone ? standaloneWindow(launch.kind) : launch.window);
  runApp(HeyoApp(host: host, window: window, launch: launch));
  await WidgetsBinding.instance.waitUntilFirstFrameRasterized;
  // The views have been built, and listen: what the host said meanwhile.
  if (host is HostConnection) host.open();
  drawn.complete();
  host.report('ready');
  if (launch.standalone) standaloneShow(host);
}
