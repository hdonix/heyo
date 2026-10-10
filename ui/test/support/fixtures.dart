// The examples of the host's types that `cargo test` writes into
// `shared/fixtures/` (src-tauri/src/shell/fixtures.rs), for holding Dart's
// models to them. `flutter test` runs in `ui/`, so the repository is `..`.

import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

const repository = '..';

/// The example of that name, as decoded JSON.
Object? fixture(String name) => jsonDecode(File('$repository/shared/fixtures/$name.json').readAsStringSync());

/// JSON without the entries that hold nothing: the host leaves some of
/// those out and writes others as null, and the two mean the same.
Object? withoutNulls(Object? json) => switch (json) {
  Map() => {
    for (final entry in json.entries)
      if (entry.value != null) entry.key: withoutNulls(entry.value),
  },
  List() => [for (final item in json) withoutNulls(item)],
  _ => json,
};

/// Reads the example into a model and writes it back: the same JSON must
/// come out. A field the model does not know is lost on the way, and one
/// it has that the host does not appears from nowhere: either fails.
void expectRoundTrip(String name, Object? Function(Map<String, Object?> json) through) {
  final example = fixture(name);
  final examples = example is List ? example : [example];
  expect(examples, isNotEmpty);
  for (final one in examples) {
    expect(withoutNulls(through((one as Map).cast())), withoutNulls(one), reason: 'shared/fixtures/$name.json and its model in Dart differ');
  }
}
