// Holds the tokens in Dart to the web view's stylesheet: the scales are
// read out of src/styles/abstracts/_scales.scss and compared, one by one.

import 'dart:io';

import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/theme/tokens.dart';

import '../support/fixtures.dart';

final _scss = File('$repository/src/styles/abstracts/_scales.scss').readAsStringSync();

/// The entries of the Sass map `$name: ( … );`, by key, as written.
Map<String, String> sassMap(String name) {
  final start = _scss.indexOf('\$$name: (');
  expect(start, isNonNegative, reason: 'no \$$name in _scales.scss');
  final body = _scss.substring(start + name.length + 4, _scss.indexOf('\n);', start));
  final entries = <String, String>{};
  for (final line in body.split('\n')) {
    final entry = RegExp(r'^\s*"?([\w-]+)"?:\s*(.+?),?\s*$').firstMatch(line);
    if (entry != null && !line.trim().startsWith('//')) entries[entry.group(1)!] = entry.group(2)!;
  }
  return entries;
}

/// A colour as the stylesheet writes one: `#rgb`, `#rrggbb` or `rgba(…)`.
Color cssColor(String text) {
  final value = text.trim();
  if (value.startsWith('#')) {
    var hex = value.substring(1);
    if (hex.length == 3) hex = hex.split('').map((c) => '$c$c').join();
    return Color(0xFF000000 | int.parse(hex, radix: 16));
  }
  final parts = RegExp(r'rgba\(([^)]*)\)').firstMatch(value)!.group(1)!.split(',').map((part) => part.trim()).toList();
  return Color.fromRGBO(int.parse(parts[0]), int.parse(parts[1]), int.parse(parts[2]), double.parse(parts[3]));
}

/// `(light, dark)`: split at the comma between the two, not those inside `rgba()`.
(Color, Color) pair(String text) {
  final inner = text.substring(1, text.length - 1);
  var depth = 0;
  for (var i = 0; i < inner.length; i++) {
    if (inner[i] == '(') depth++;
    if (inner[i] == ')') depth--;
    if (inner[i] == ',' && depth == 0) return (cssColor(inner.substring(0, i)), cssColor(inner.substring(i + 1)));
  }
  fail('not a pair: $text');
}

double px(String text) => double.parse(text.trim().replaceAll('px', ''));

void main() {
  test('every colour is the stylesheet\'s, in both schemes', () {
    final light = HeyoColors.light.named, dark = HeyoColors.dark.named;
    var compared = 0;
    void compare(String map, String prefix) {
      for (final entry in sassMap(map).entries) {
        final name = '$prefix${entry.key}';
        final (inLight, inDark) = pair(entry.value);
        expect(light[name], inLight, reason: '$name, light');
        expect(dark[name], inDark, reason: '$name, dark');
        compared++;
      }
    }

    compare('colors', '');
    compare('fills', 'fill-');
    compare('shadows', 'shadow-');
    compare('raycast', 'raycast-');
    // Nothing in Dart that the stylesheet does not have, either.
    expect(compared, light.length);
    expect(compared, greaterThan(30));
  });

  test('the type scale is the stylesheet\'s', () {
    final type = sassMap('type');
    expect(type.keys, TextSize.all.map((size) => size.name));
    for (final size in TextSize.all) {
      final parts = type[size.name]!.replaceAll(RegExp(r'[()]'), '').split(',').map((part) => part.trim()).toList();
      expect(size.size, px(parts[0]), reason: size.name);
      expect(size.line, px(parts[1]), reason: size.name);
      expect(size.tracking, double.parse(parts[2].replaceAll('em', '')), reason: size.name);
    }
    expect(sassMap('weights').map((name, value) => MapEntry(name, int.parse(value))), Weights.named);
  });

  test('shapes and sizes are the stylesheet\'s', () {
    expect(sassMap('radii').map((name, value) => MapEntry(name, px(value))), Radii.named);
    expect(sassMap('heights').map((name, value) => MapEntry(name, px(value))), Heights.named);
    expect(sassMap('icons').map((name, value) => MapEntry(name, px(value))), IconSizes.named);
    expect(px(RegExp(r'\$unit:\s*(\w+);').firstMatch(_scss)!.group(1)!), space(1));
    expect(px(RegExp(r'\$window-margin:\s*(\w+);').firstMatch(_scss)!.group(1)!), windowMargin);
  });

  test('motion is the stylesheet\'s', () {
    final durations = sassMap('durations').map((name, value) => MapEntry(name, Duration(milliseconds: int.parse(value.replaceAll('ms', '')))));
    expect(durations, HeyoDurations.named);
    final easings = sassMap('easings');
    expect(easings.keys, HeyoEasings.named.keys);
    for (final entry in easings.entries) {
      final numbers = RegExp(r'cubic-bezier\(([^)]*)\)').firstMatch(entry.value)!.group(1)!.split(',').map((part) => double.parse(part.trim())).toList();
      final curve = HeyoEasings.named[entry.key]!;
      expect([curve.a, curve.b, curve.c, curve.d], numbers, reason: entry.key);
    }
  });

  test('what is made of other colours is made as the stylesheet makes it', () {
    expect(HeyoColors.light.selected, HeyoColors.light.fill3);
    expect(HeyoColors.light.input, HeyoColors.light.fill2);
    expect(HeyoColors.dark.accentSoft.a, closeTo(0.16, 0.001));
    expect(HeyoColors.dark.ring.a, closeTo(0.28, 0.001));
  });
}
