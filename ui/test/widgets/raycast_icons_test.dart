import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/widgets/image.dart';
import 'package:heyo_ui/widgets/raycast_icons.g.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../tool/gen_icons.dart' as generator;
import '../support/fixtures.dart';

void main() {
  final aliasSource = File('$repository/src/raycast/iconAliases.ts').readAsStringSync();

  test('the tables are the ones the generator writes today', () {
    final expected = generator.generate(aliasSource: aliasSource, packageSource: generator.packageSource(Directory.current.path));
    expect(File('lib/widgets/raycast_icons.g.dart').readAsStringSync(), expected, reason: 'run: dart run tool/gen_icons.dart');
  });

  test('every alias of the web view is here', () {
    final inWebView = generator.readAliases(aliasSource);
    expect(inWebView.length, greaterThan(200));
    expect(raycastAliases.keys, inWebView.keys);
  });

  /// The web view's aliases name icons of lucide-react; the same icon must
  /// exist in the package Flutter draws them from.
  test('every name in the mapping resolves to an icon', () {
    for (final entry in raycastAliases.entries) {
      expect(lucideIcons[entry.value], isNotNull, reason: '${entry.key} → ${entry.value} is not an icon of lucide_icons_flutter');
      expect(raycastIcon('${entry.key}-16'), isNotNull, reason: entry.key);
    }
  });

  test('a name is looked up as the web view looks it up', () {
    // An alias.
    expect(raycastIcon('add-person-16')!.icon, LucideIcons.userPlus);
    // The Lucide icon of the same name.
    expect(raycastIcon('camera-16')!.icon, LucideIcons.camera);
    expect(raycastIcon('arrow-right-16')!.icon, LucideIcons.arrowRight);
    expect(raycastIcon('app-window-grid-3x3-16')!.icon, LucideIcons.grid3x3);
    // Filled: the same icon, said to be filled.
    final star = raycastIcon('star-filled-16')!;
    expect((star.icon, star.filled), (LucideIcons.star, true));
    expect(raycastIcon('star-16')!.filled, isFalse);
    expect(raycastIcon('no-such-icon-16'), isNull);
  });

  /// What extensions can name: every icon of Raycast's API. Those the web
  /// view cannot draw either are not this interface's to draw.
  test('Raycast\'s own icons resolve wherever the web view resolves them', () {
    final types = File('$repository/node_modules/@raycast/api/types/index.d.ts');
    if (!types.existsSync()) return markTestSkipped('node_modules is not installed');
    final names = RegExp(r'= "([a-z0-9-]+-16)"').allMatches(types.readAsStringSync()).map((match) => match.group(1)!).toSet();
    expect(names.length, greaterThan(400));
    final missing = names.where((name) => raycastIcon(name) == null && !RegExp(r'^number-\d+-16$').hasMatch(name)).toList()..sort();
    // Raycast has icons Lucide has none for; they are the same ones in both
    // interfaces. The number only goes down.
    expect(missing.length, lessThanOrEqualTo(_knownMissing), reason: 'Raycast icons without a Lucide icon: $missing');
  });
}

/// Raycast icons that neither interface can draw, when this was written.
const _knownMissing = 0;
