import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/host/commands.dart';
import 'package:heyo_ui/host/commands.g.dart';
import 'package:heyo_ui/host/events.g.dart';
import 'package:heyo_ui/host/fake.dart';
import 'package:heyo_ui/host/host.dart';

/// A host at the other end of two streams, driven by hand.
class _Wire {
  final toUi = StreamController<String>();
  final fromUi = <Map<String, Object?>>[];
  late final HostConnection host;

  _Wire() {
    host = HostConnection(toUi.stream, (line) => fromUi.add((jsonDecode(line) as Map).cast()));
  }

  Future<void> say(Map<String, Object?> message) async {
    toUi.add(jsonEncode(message));
    await Future<void>.delayed(Duration.zero);
  }
}

void main() {
  test('a command goes out by name with its arguments, and its answer comes back', () async {
    final wire = _Wire();
    final answer = wire.host.invoke<List<Object?>>('search', {'query': 'se'});
    expect(wire.fromUi.single, {'id': 1, 'invoke': 'search', 'args': {'query': 'se'}});
    await wire.say({'id': 1, 'result': ['a', 'b']});
    expect(await answer, ['a', 'b']);
  });

  test('answers find their questions in whatever order they come', () async {
    final wire = _Wire();
    final first = wire.host.invoke<String>('read_clipboard');
    final second = wire.host.invoke<Object?>('hide');
    await wire.say({'id': 2, 'result': null});
    await wire.say({'id': 1, 'result': 'copied'});
    expect(await second, isNull);
    expect(await first, 'copied');
  });

  test('a command the host refuses fails with the host\'s words', () async {
    final wire = _Wire();
    final refused = expectLater(
      wire.host.invoke<Object?>('change_settings', {'changes': {}}),
      throwsA(isA<HostError>().having((e) => e.message, 'message', 'change_settings is not available to this window')),
    );
    await wire.say({'id': 1, 'error': 'change_settings is not available to this window'});
    await refused;
  });

  test('events reach those listening for them, by name', () async {
    final wire = _Wire();
    final shown = <Object?>[];
    final opened = <Object?>[];
    wire.host.on(HeyoEvent.shown).listen(shown.add);
    wire.host.on(HeyoEvent.openItem).listen(opened.add);
    await wire.say({'event': 'heyo://shown', 'payload': null});
    await wire.say({'event': 'heyo://open-item', 'payload': 'quicklink:1'});
    expect(shown, [null]);
    expect(opened, ['quicklink:1']);
  });

  test('what the window is told and what it reports go their own way', () async {
    final wire = _Wire();
    final told = <Map<String, Object?>>[];
    wire.host.control.listen(told.add);
    await wire.say({'window': 'frame', 'x': 10, 'logical': true});
    expect(told.single['window'], 'frame');
    wire.host.report('shown', {'ms': 12});
    expect(wire.fromUi.single, {'window': 'shown', 'ms': 12});
  });

  test('a ping is answered: the host ends a window that does not', () async {
    final wire = _Wire();
    await wire.say({'ping': 4});
    expect(wire.fromUi.single, {'pong': 4});
  });

  test('what the host says before the views listen waits for them', () async {
    final lines = StreamController<String>();
    final host = HostConnection(lines.stream, (_) {}, held: true);
    lines.add(jsonEncode({'event': 'heyo://screenshot', 'payload': {'mode': 'capture'}}));
    lines.add(jsonEncode({'event': 'heyo://shown', 'payload': null}));
    await Future<void>.delayed(Duration.zero);
    // The view is built and subscribes; then the window says it is ready.
    final heard = <String>[];
    host.events.listen((event) => heard.add(event.name));
    await Future<void>.delayed(Duration.zero);
    expect(heard, isEmpty);
    host.open();
    await Future<void>.delayed(Duration.zero);
    expect(heard, ['heyo://screenshot', 'heyo://shown']);
    lines.add(jsonEncode({'event': 'heyo://index-ready', 'payload': null}));
    await Future<void>.delayed(Duration.zero);
    expect(heard.last, 'heyo://index-ready');
  });

  test('a line that is not a message is passed over', () async {
    final wire = _Wire();
    final events = <HostEvent>[];
    wire.host.events.listen(events.add);
    wire.toUi.add('warming up');
    await wire.say({'event': 'heyo://shown', 'payload': null});
    expect(events, hasLength(1));
  });

  test('the process ends with the host', () async {
    var closed = false;
    final lines = StreamController<String>();
    HostConnection(lines.stream, (_) {}, onClosed: () => closed = true);
    await lines.close();
    expect(closed, isTrue);
  });

  test('how it was started says which window it is', () {
    final launch = Launch.parse(['--window=pin', '--label=pin-3', '--init={"window":{"title":"Heyo Pinned Screenshot","width":300.0},"view":{"path":"/tmp/pin-3.png"}}']);
    expect((launch.kind, launch.label), ('pin', 'pin-3'));
    expect(launch.window['width'], 300.0);
    expect(launch.view['path'], '/tmp/pin-3.png');
    expect(launch.standalone, isFalse);
    final bare = Launch.parse([]);
    expect((bare.kind, bare.label), ('main', 'main'));
    expect(bare.window, isEmpty);
    expect(bare.view, isEmpty);
    expect(Launch.parse(['--standalone', '--window=settings', '--bench=keys']).bench, 'keys');
  });

  test('every command of the host has a method here, and no other', () {
    final source = File('lib/host/commands.dart').readAsStringSync();
    final called = RegExp(r"(?:invoke(?:<[^(]*>)?|_object)\(\s*'([a-z_]+)'").allMatches(source).map((match) => match.group(1)!).toSet();
    expect(called.difference(hostCommands.keys.toSet()), isEmpty, reason: 'methods for commands the host does not have');
    expect(hostCommands.keys.toSet().difference(called), isEmpty, reason: 'commands of the host without a method in lib/host/commands.dart');
  });

  test('the typed methods send what the host expects and read what it answers', () async {
    final host = FakeHost({
      'search': (args) => [
        {'id': 'app:firefox', 'title': 'Firefox', 'subtitle': null, 'icon': null, 'kind': 'Application', 'favorite': false},
      ],
      'launch': (_) => null,
      'get_item': (_) => null,
      'read_clipboard': (_) => 'copied',
      'item_hotkeys': (_) => {'app:firefox': '<Super>f'},
    });
    expect((await host.search('fire')).single.title, 'Firefox');
    await host.launch('app:firefox', fallbackText: 'x');
    expect(host.invoked.last.$1, 'launch');
    expect(host.invoked.last.$2, {'id': 'app:firefox', 'fallbackText': 'x', 'arguments': null});
    expect(await host.getItem('nothing'), isNull);
    expect(await host.readClipboard(), 'copied');
    expect(await host.itemHotkeys(), {'app:firefox': '<Super>f'});
    await expectLater(host.hide(), throwsA(isA<HostError>()));
  });
}
