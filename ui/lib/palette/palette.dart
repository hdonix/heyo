// The palette: the root search (src/App.tsx and src/RootSearch.tsx in the
// web view).
//
// What is here is the whole path, end to end, and little more: it comes up
// when the host shows it, searches the real index, draws the items with
// their real icons, opens applications and commands, and hides on Escape,
// on a click outside and when it loses the keyboard. Quicklinks, snippets,
// the calculator, arguments, the action panel, compact mode and what a
// command shows once it runs are still to come.

import 'dart:async';
import 'dart:io' show exit;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../dev/bench.dart';
import '../extension/extension.dart';
import '../host/commands.dart';
import '../host/events.g.dart';
import '../host/host.dart';
import '../host/models.dart';
import '../host/scope.dart';
import '../theme/theme.dart';
import '../widgets/item_icon.dart';
import '../widgets/keys.dart';
import '../widgets/list.dart';
import '../widgets/overlays.dart';
import '../widgets/panel.dart';
import '../widgets/selection.dart';
import '../widgets/text.dart';
import '../widgets/text_field.dart';

const _placeholder = 'Search apps, commands, or calculate…';
const _settingsId = 'heyo:settings';

/// Room at the end of the list for the actions, which float over it.
const _listEnd = 56.0;

/// Where the list's end fades, as parts of the list's height from its
/// bottom: gone at 8 px, whole at 64 px (of a list 410 px tall).
const _fadeTo = 8 / 410;
const _fadeFrom = 64 / 410;

/// A line of the list: the heading of a section, or an item.
class _Line {
  _Line.heading(this.heading) : item = null, index = -1;
  _Line.item(Item this.item, this.index) : heading = null;
  final String? heading;
  final Item? item;

  /// The item's place among the items.
  final int index;

  double get extent => heading != null ? SectionTitle.height : ListRow.height;
}

class PaletteWindow extends StatefulWidget {
  const PaletteWindow({super.key, this.bench, this.benchExits = false});

  /// `keys` or `scroll`: once shown, drive the list and say how the frames
  /// went (dev/bench.dart).
  final String? bench;

  /// End the process once that is said (a palette started by hand).
  final bool benchExits;

  @override
  State<PaletteWindow> createState() => _PaletteWindowState();
}

class _PaletteWindowState extends State<PaletteWindow> {
  final _query = TextEditingController();
  final _focus = FocusNode(debugLabel: 'search');
  final _scroll = ScrollController();
  final _subscriptions = <StreamSubscription<Object?>>[];
  Host? _host;

  var _items = <Item>[];
  var _lines = <_Line>[];

  /// Where each line starts among the list's contents.
  var _tops = <double>[];
  var _selected = 0;

  /// Counts searches: answers can come back out of order, and only the
  /// latest question's counts.
  var _asked = 0;

  /// A command has taken the palette over (the session the runtime gave it).
  int? _session;
  ({String title, String? message})? _problem;
  Timer? _problemTimer;
  var _benched = false;

  Host get host => _host!;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_host != null) return;
    _host = HostScope.of(context);
    _subscriptions
      ..add(host.on(HeyoEvent.shown).listen((_) => _shown()))
      ..add(host.on(HeyoEvent.indexReady).listen((_) => _search(_query.text, keepSelection: true)))
      ..add(host.on(HeyoEvent.runtime).listen(_runtime));
    _search('');
  }

  @override
  void dispose() {
    for (final subscription in _subscriptions) {
      subscription.cancel();
    }
    _problemTimer?.cancel();
    _query.dispose();
    _focus.dispose();
    _scroll.dispose();
    super.dispose();
  }

  /// The palette came up: the root search, empty, with the keyboard.
  void _shown() {
    _focus.requestFocus();
    if (_session == null && _query.text.isNotEmpty) {
      _query.clear();
      _search('');
    }
    if (widget.bench != null && !_benched) {
      _benched = true;
      _measure(widget.bench!);
    }
  }

  /// What the runtime says: here, only whether a command is showing.
  void _runtime(Object? message) {
    if (message is! Map) return;
    switch (message['type']) {
      case 'render':
        final showing = message['view'] != null ? message['session'] as int? : null;
        if (showing != _session) setState(() => _session = showing);
      case 'closed' || 'exited':
        if (_session != null) setState(() => _session = null);
        _focus.requestFocus();
    }
  }

  Future<void> _search(String query, {bool keepSelection = false}) async {
    final asked = ++_asked;
    final List<Item> items;
    try {
      items = await host.search(query);
    } on HostError catch (e) {
      return _fail('Could not search', e.message);
    }
    if (!mounted || asked != _asked) return;
    final chosen = keepSelection && _selected < _items.length ? _items[_selected].id : null;
    final lines = <_Line>[];
    final tops = <double>[];
    var top = 0.0;
    String? section;
    for (final (index, item) in items.indexed) {
      if (item.section != section) {
        section = item.section;
        if (section != null) {
          lines.add(_Line.heading(section));
          tops.add(top);
          top += SectionTitle.height;
        }
      }
      lines.add(_Line.item(item, index));
      tops.add(top);
      top += ListRow.height;
    }
    final kept = chosen == null ? -1 : items.indexWhere((item) => item.id == chosen);
    setState(() {
      _items = items;
      _lines = lines;
      _tops = tops;
      _selected = kept >= 0 ? kept : 0;
    });
    // Before the list has been laid out there is nothing to scroll back.
    if (kept < 0 && _scroll.hasClients && _scroll.position.hasContentDimensions) _scroll.jumpTo(0);
  }

  /// Selects an item; the keyboard's choice is also scrolled into view.
  void _select(int index, {required bool byKeyboard}) {
    if (index == _selected || index < 0 || index >= _items.length) return;
    setState(() => _selected = index);
    if (byKeyboard) _reveal();
  }

  void _reveal() {
    final line = _lines.indexWhere((line) => line.index == _selected);
    if (line < 0 || !_scroll.hasClients) return;
    final top = _tops[line];
    // The first row of the list brings its heading along.
    final from = line == 1 && _lines.first.heading != null ? 0.0 : top;
    reveal(_scroll.position, (top: from, bottom: top + ListRow.height), padding: (top: line <= 1 ? 0 : space(8), bottom: _listEnd));
  }

  void _move(int delta, {bool wrap = true}) => _select(step(_selected, delta, _items.length, wrap: wrap), byKeyboard: true);

  Future<void> _open(Item item) async {
    unawaited(host.recordUsage(item.id).catchError((Object _) {}));
    try {
      if (item.id == _settingsId) return await host.openSettings();
      if (item.kind == 'Quicklink' || item.kind == 'Snippet') {
        return _fail('Could not open ${item.title}', '${item.kind}s are not in the Flutter palette yet');
      }
      await host.launch(item.id);
    } on HostError catch (e) {
      _fail('Could not open ${item.title}', e.message);
    }
  }

  void _fail(String title, String? message) {
    if (!mounted) return;
    setState(() => _problem = (title: title, message: message));
    _problemTimer?.cancel();
    _problemTimer = Timer(const Duration(seconds: 4), () {
      if (mounted) setState(() => _problem = null);
    });
  }

  void _hide() => unawaited(host.hide().catchError((Object _) {}));

  KeyEventResult _key(KeyEvent event) {
    if (event is KeyUpEvent) return KeyEventResult.ignored;
    final key = event.logicalKey;
    if (_session != null) {
      // A command is showing, which this palette cannot draw yet: Escape
      // closes it, and nothing else is the palette's to act on.
      if (key == LogicalKeyboardKey.escape) {
        unawaited(host.runtimeSend(WebviewMessage.close(session: _session)).catchError((Object _) {}));
        setState(() => _session = null);
        return KeyEventResult.handled;
      }
      return KeyEventResult.ignored;
    }
    if (key == LogicalKeyboardKey.arrowDown) {
      _move(1);
    } else if (key == LogicalKeyboardKey.arrowUp) {
      _move(-1);
    } else if (key == LogicalKeyboardKey.pageDown) {
      _move(8, wrap: false);
    } else if (key == LogicalKeyboardKey.pageUp) {
      _move(-8, wrap: false);
    } else if (key == LogicalKeyboardKey.enter || key == LogicalKeyboardKey.numpadEnter) {
      // Once for each press: a held Enter does not open the item again.
      if (isPress(event) && _selected < _items.length) _open(_items[_selected]);
    } else if (key == LogicalKeyboardKey.escape) {
      if (_query.text.isNotEmpty) {
        _query.clear();
        _search('');
      } else {
        _hide();
      }
    } else {
      return KeyEventResult.ignored;
    }
    return KeyEventResult.handled;
  }

  Future<void> _measure(String what) async {
    await Future<void>.delayed(const Duration(milliseconds: 800));
    if (!_scroll.hasClients) return;
    final frames = Frames()..start();
    if (what == 'keys') {
      // An arrow key held down repeats about thirty times a second.
      for (var i = 0; i < 90; i++) {
        _move(1);
        await Future<void>.delayed(const Duration(milliseconds: 33));
      }
    } else {
      final end = _scroll.position.maxScrollExtent.clamp(0.0, 15000.0);
      await _scroll.animateTo(end, duration: const Duration(milliseconds: 1500), curve: Curves.linear);
      await _scroll.animateTo(0, duration: const Duration(milliseconds: 1500), curve: Curves.linear);
    }
    await frames.report('palette $what, ${_items.length} items');
    if (widget.benchExits) exit(0);
  }

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final colors = theme.colors;
    final selected = _selected < _items.length ? _items[_selected] : null;
    return WindowPanel(
      onTapOutside: _hide,
      child: Stack(
        children: [
          Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SizedBox(
                height: 60,
                child: Padding(
                  padding: EdgeInsets.symmetric(horizontal: space(5)),
                  child: Center(
                    child: HeyoTextField(
                      controller: _query,
                      focusNode: _focus,
                      autofocus: true,
                      size: TextSize.xl,
                      placeholder: _placeholder,
                      onChanged: _search,
                      onKey: _key,
                    ),
                  ),
                ),
              ),
              Expanded(child: _session != null ? const ExtensionView() : _list(theme)),
            ],
          ),
          // Not a bar: what the footer holds floats over the end of the view.
          Positioned(
            left: space(2),
            right: space(2),
            bottom: space(2),
            height: 38,
            child: Row(
              children: [
                Expanded(
                  child: _problem == null
                      ? const SizedBox()
                      : Align(alignment: Alignment.centerLeft, child: Toast(title: _problem!.title, message: _problem!.message, style: ToastStyle.failure)),
                ),
                SizedBox(width: space(2)),
                if (selected != null && _session == null)
                  Container(
                    height: 38,
                    padding: EdgeInsets.symmetric(horizontal: space(1)),
                    decoration: BoxDecoration(color: colors.surfaceRaised, borderRadius: BorderRadius.circular(Radii.full), boxShadow: pillShadow(colors)),
                    child: Center(
                      child: Pressable(
                        onPressed: () => _open(selected),
                        builder: (context, fill) => Container(
                          height: 30,
                          padding: EdgeInsets.only(left: space(3), right: space(1)),
                          decoration: BoxDecoration(color: fill, borderRadius: BorderRadius.circular(Radii.full)),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              HeyoText(_verb(selected), size: TextSize.sm, weight: Weights.medium),
                              SizedBox(width: space(2)),
                              const Keycap('↵'),
                            ],
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }

  static String _verb(Item item) => switch (item.kind) {
    'Application' => 'Open Application',
    'Snippet' => 'Paste Snippet',
    'Quicklink' => 'Open Quicklink',
    _ => 'Open Command',
  };

  Widget _list(HeyoThemeData theme) {
    if (_items.isEmpty) {
      return Align(
        alignment: Alignment.topCenter,
        child: Padding(
          padding: EdgeInsets.all(space(6)),
          child: HeyoText('No results', size: TextSize.row, color: theme.colors.textMuted),
        ),
      );
    }
    // The rows fade out under the actions rather than end at a bar. (The
    // web view masks the list; here the panel's colour is laid over its
    // end, which looks the same and does not make every frame of a scroll
    // be drawn twice.)
    final bg = theme.colors.bg.withValues(alpha: 1);
    return DecoratedBox(
      position: DecorationPosition.foreground,
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: Alignment.bottomCenter,
          end: Alignment.topCenter,
          colors: [bg, bg, bg.withValues(alpha: 0)],
          stops: const [0, _fadeTo, _fadeFrom],
        ),
      ),
      child: ListView.builder(
        controller: _scroll,
        padding: EdgeInsets.only(left: space(2), right: space(2), bottom: _listEnd),
        itemCount: _lines.length,
        itemExtentBuilder: (index, _) => index < _lines.length ? _lines[index].extent : null,
        itemBuilder: (context, index) {
          final line = _lines[index];
          final item = line.item;
          if (item == null) return SectionTitle(line.heading!);
          return UnderPointer(
            onSelect: () => _select(line.index, byKeyboard: false),
            child: ListRow(
              leading: ItemIcon(icon: item.icon, title: item.title, colorKey: item.subtitle?.isNotEmpty == true ? '${item.subtitle}/${item.title}' : item.title),
              title: item.title,
              subtitle: item.subtitle,
              trailing: KindLabel(item.kind),
              selected: line.index == _selected,
              onTap: () {
                _select(line.index, byKeyboard: false);
                _open(item);
              },
            ),
          );
        },
      ),
    );
  }
}
