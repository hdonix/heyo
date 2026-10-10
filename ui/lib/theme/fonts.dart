// The faces text is set in.
//
// Inter is bundled, as the web view bundles it. Everything Inter lacks
// (Chinese, Japanese, Korean, emoji, symbols) must be named: Flutter does
// not ask the system which font has a character, as a web view does, and
// draws an empty box instead. So the system is asked once, here, which
// fonts it would use, and their names follow Inter in every text style.

import 'dart:io';

const sans = 'Inter';

/// Faces that have Chinese, Japanese and Korean, and emoji, where they are
/// commonly installed. Used until the system has answered, and where it
/// cannot be asked.
const _usual = [
  'Noto Sans CJK SC',
  'Noto Sans SC',
  'Source Han Sans SC',
  'PingFang SC',
  'Microsoft YaHei UI',
  'Microsoft YaHei',
  'WenQuanYi Micro Hei',
  'WenQuanYi Zen Hei',
  'Droid Sans Fallback',
  'Noto Color Emoji',
  'Segoe UI Emoji',
  'Apple Color Emoji',
  'Noto Sans Symbols2',
  'Noto Sans Symbols',
  'Segoe UI Symbol',
  'DejaVu Sans',
];

const _usualMono = ['JetBrains Mono', 'Cascadia Mono', 'DejaVu Sans Mono', 'Liberation Mono', 'Noto Sans Mono', 'Menlo', 'Consolas', 'Noto Sans Mono CJK SC'];

List<String> _fallback = _usual;

/// The faces to try, in order, for what Inter lacks.
List<String> get fallback => _fallback;

/// The faces for text in a fixed width; the first one installed is used.
List<String> get mono => [..._usualMono, ..._fallback];

/// The languages to ask for, the user's own first: Chinese, Japanese and
/// Korean draw some of the same characters differently, and the first font
/// that has a character wins.
List<String> languages(String locale) {
  final own = locale.toLowerCase().replaceAll('_', '-').split('.').first;
  final all = ['zh-cn', 'zh-tw', 'ja', 'ko'];
  final exact = all.where(own.startsWith);
  final related = all.where((language) => own.length >= 2 && language.startsWith(own.substring(0, 2)));
  // Mainland forms unless the locale says otherwise.
  return {...exact, ...related, ...all}.toList();
}

/// What `fc-match --sort` printed, one family a line, as a list without
/// repeats.
List<String> families(Iterable<String> outputs) {
  final seen = <String>{};
  for (final output in outputs) {
    for (final line in output.split('\n')) {
      // A font may go by several names: "Noto Sans CJK SC,Noto Sans CJK SC Regular".
      final family = line.split(',').first.trim();
      if (family.isNotEmpty) seen.add(family);
    }
  }
  return seen.toList();
}

/// Asks the system (fontconfig) which fonts it would use for each language
/// and for emoji, and makes those the fallback. Returns whether anything
/// changed. Where it cannot be asked, the usual names stay.
Future<bool> askSystem({String? locale}) async {
  // (`flutter test` has no system worth asking, and waits for no process.)
  if (!Platform.isLinux || Platform.environment.containsKey('FLUTTER_TEST')) return false;
  try {
    final asked = [
      for (final language in languages(locale ?? Platform.localeName)) Process.run('fc-match', ['--sort', '--format=%{family}\n', 'sans-serif:lang=$language']),
      Process.run('fc-match', ['--sort', '--format=%{family}\n', 'emoji']),
    ];
    final answers = await Future.wait(asked);
    // Each answer lists every font installed, best first; the first few of
    // each are the ones that matter, and a short list is searched faster.
    final best = [
      for (final answer in answers)
        if (answer.exitCode == 0) (answer.stdout as String).split('\n').take(6).join('\n'),
    ];
    final found = families(best).where((family) => family != sans).toList();
    if (found.isEmpty) return false;
    _fallback = {...found, ..._usual}.toList();
    return true;
  } on ProcessException {
    return false;
  }
}
