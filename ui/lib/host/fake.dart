// A host that is not there: for widget tests, and for a view started by
// hand (`heyo-ui --standalone --window=…`) while it is being worked on.

import 'dart:async';

import 'host.dart';

/// Answers a command from its arguments.
typedef FakeCommand = FutureOr<Object?> Function(Map<String, Object?> args);

class FakeHost extends Host {
  FakeHost([Map<String, FakeCommand>? commands]) : commands = commands ?? {};

  /// What each command answers. One that is not here fails as the host
  /// fails a command the window may not invoke.
  final Map<String, FakeCommand> commands;

  /// Every command invoked, in order, with its arguments.
  final invoked = <(String, Map<String, Object?>)>[];

  /// Everything the window reported, in order.
  final reported = <(String, Map<String, Object?>)>[];

  final _events = StreamController<HostEvent>.broadcast(sync: true);
  final _control = StreamController<Map<String, Object?>>.broadcast(sync: true);

  /// Sends the views an event, as the host would.
  void emit(String event, [Object? payload]) => _events.add(HostEvent(event, payload));

  /// Tells the window to do something, as the host would.
  void tell(String what, [Map<String, Object?> detail = const {}]) => _control.add({...detail, 'window': what});

  @override
  Future<T> invoke<T>(String command, [Map<String, Object?> args = const {}]) async {
    invoked.add((command, args));
    final answer = commands[command];
    if (answer == null) throw HostError('$command is not available to this window');
    return await answer(args) as T;
  }

  @override
  Stream<HostEvent> get events => _events.stream;

  @override
  Stream<Map<String, Object?>> get control => _control.stream;

  @override
  void report(String what, [Map<String, Object?> detail = const {}]) => reported.add((what, detail));
}
