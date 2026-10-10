// The Heyo host, as a window's process knows it.
//
// Heyo (the Rust program) starts one `heyo-ui` process for each window and
// speaks JSON with it, one message a line, over the process's stdin and
// stdout. `src-tauri/src/shell/bridge.rs` lists the messages. What the web
// view does with Tauri's `invoke()` and `listen()`, a view here does with
// [Host.invoke] and [Host.on]: the same commands by the same names with the
// same arguments, and the same events.

import 'dart:async';
import 'dart:convert';
import 'dart:io';

/// A command the host refused or failed: what the web view's `invoke()`
/// rejects with.
class HostError implements Exception {
  HostError(this.message);
  final String message;

  @override
  String toString() => message;
}

/// An event from the host: its name (`heyo://…`) and what it carries.
class HostEvent {
  HostEvent(this.name, this.payload);
  final String name;
  final Object? payload;
}

/// How the process was started: which window it is and what with.
class Launch {
  Launch({this.kind = 'main', String? label, this.window = const {}, this.view = const {}, this.standalone = false, this.bench})
    : label = label ?? kind;

  /// Which view to show: `main`, `settings`, `dictation`, `screenshot`,
  /// `pin` or `island`.
  final String kind;

  /// The window's label, as the host knows it (`main`, `pin-3`).
  final String label;

  /// What the window is like: `title`, `width`, `height` (logical pixels),
  /// `focus`, `above`, `taskbar`.
  final Map<String, Object?> window;

  /// What the view is started with (a pinned capture's `path`).
  final Map<String, Object?> view;

  /// Started by hand, without a host: the view is given made-up answers.
  final bool standalone;

  /// `--bench=…`: measure frames while driving the view, then exit.
  final String? bench;

  /// Reads the arguments Heyo passes: `--window=`, `--label=`, `--init=`.
  factory Launch.parse(List<String> args) {
    String? option(String name) {
      for (final arg in args) {
        if (arg.startsWith('--$name=')) return arg.substring(name.length + 3);
      }
      return null;
    }

    Map<String, Object?> part(Object? value) => value is Map ? value.cast<String, Object?>() : const {};
    final init = option('init');
    final given = init == null ? const <String, Object?>{} : part(jsonDecode(init));
    final kind = option('window') ?? 'main';
    return Launch(
      kind: kind,
      label: option('label') ?? kind,
      window: part(given['window']),
      view: part(given['view']),
      standalone: args.contains('--standalone'),
      bench: option('bench') ?? Platform.environment['HEYO_UI_BENCH'],
    );
  }
}

/// The host. Views are given one and ask it everything; tests and a view
/// started by hand are given a `FakeHost` (fake.dart).
abstract class Host {
  /// Invokes a command of the host's and returns what it answers, as
  /// decoded JSON. Throws [HostError] with the host's words when the
  /// command fails or is not this window's to invoke.
  Future<T> invoke<T>(String command, [Map<String, Object?> args = const {}]);

  /// Every event the host sends this window.
  Stream<HostEvent> get events;

  /// What the host tells the window to do (`show`, `hide`, …), for
  /// `HeyoWindow` (window.dart) alone.
  Stream<Map<String, Object?>> get control;

  /// Tells the host what happened to the window, for `HeyoWindow` alone.
  void report(String what, [Map<String, Object?> detail = const {}]);

  /// The payloads of the events of one name.
  Stream<Object?> on(String event) => events.where((e) => e.name == event).map((e) => e.payload);
}

/// The host at the other end of two streams of lines.
class HostConnection extends Host {
  /// `lines` are the messages from the host, `write` sends it one. With
  /// `held`, events are kept until [open] instead of being passed on.
  HostConnection(Stream<String> lines, this._write, {void Function()? onClosed, bool held = false}) : _open = !held {
    lines.listen(_received, onDone: onClosed, onError: (Object _) => onClosed?.call());
  }

  /// Over this process's stdin and stdout. When the host closes its end
  /// (it has exited, or is done with this window) the process ends.
  ///
  /// Events are held until [open]: the host starts some windows the moment
  /// it has something to tell them (a capture for the screenshot overlay),
  /// and what it says must find the view listening.
  factory HostConnection.stdio() {
    final lines = stdin.transform(utf8.decoder).transform(const LineSplitter());
    return HostConnection(lines, (line) => stdout.writeln(line), onClosed: () => exit(0), held: true);
  }

  bool _open;
  final _early = <HostEvent>[];

  /// The views are up and listening: passes on the events that came before,
  /// in order, and every one after as it comes.
  void open() {
    if (_open) return;
    _open = true;
    _early.forEach(_events.add);
    _early.clear();
  }

  final void Function(String line) _write;

  /// `HEYO_UI_DEBUG`: say on stderr what the host tells this window.
  static final _debug = Platform.environment.containsKey('HEYO_UI_DEBUG');
  final _waiting = <int, Completer<Object?>>{};
  final _events = StreamController<HostEvent>.broadcast();
  final _control = StreamController<Map<String, Object?>>.broadcast();
  var _next = 0;

  void _received(String line) {
    final Object? decoded;
    try {
      decoded = jsonDecode(line);
    } on FormatException {
      stderr.writeln('heyo-ui: not a message from the host: $line');
      return;
    }
    if (decoded is! Map) return;
    final message = decoded.cast<String, Object?>();
    if (_debug && !message.containsKey('id') && !message.containsKey('ping')) {
      stderr.writeln('heyo-ui: host says ${line.length > 160 ? '${line.substring(0, 160)}…' : line}');
    }
    if (message.containsKey('event')) {
      final event = HostEvent(message['event'] as String, message['payload']);
      _open ? _events.add(event) : _early.add(event);
    } else if (message.containsKey('window')) {
      _control.add(message);
    } else if (message.containsKey('ping')) {
      // Answered from here, where the views run: an answer means they do.
      _write(jsonEncode({'pong': message['ping']}));
    } else if (message['id'] is int) {
      final waiter = _waiting.remove(message['id']);
      if (message.containsKey('error')) {
        waiter?.completeError(HostError('${message['error']}'));
      } else {
        waiter?.complete(message['result']);
      }
    }
  }

  @override
  Future<T> invoke<T>(String command, [Map<String, Object?> args = const {}]) async {
    final id = ++_next;
    final answer = Completer<Object?>();
    _waiting[id] = answer;
    _write(jsonEncode({'id': id, 'invoke': command, 'args': args}));
    return await answer.future as T;
  }

  @override
  Stream<HostEvent> get events => _events.stream;

  @override
  Stream<Map<String, Object?>> get control => _control.stream;

  @override
  void report(String what, [Map<String, Object?> detail = const {}]) => _write(jsonEncode({...detail, 'window': what}));
}
