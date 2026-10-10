// Gives the views below it the host.

import 'package:flutter/widgets.dart';

import 'host.dart';

class HostScope extends InheritedWidget {
  const HostScope({super.key, required this.host, required super.child});
  final Host host;

  static Host of(BuildContext context) => context.dependOnInheritedWidgetOfExactType<HostScope>()!.host;

  @override
  bool updateShouldNotify(HostScope old) => old.host != host;
}
