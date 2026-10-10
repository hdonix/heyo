import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:heyo_ui/theme/theme.dart';
import 'package:heyo_ui/widgets/image.dart';
import 'package:heyo_ui/widgets/item_icon.dart';

const _light = HeyoThemeData(brightness: Brightness.light, fallback: []);
const _dark = HeyoThemeData(brightness: Brightness.dark, fallback: []);

Widget _in(HeyoThemeData theme, Widget child) => Directionality(
  textDirection: TextDirection.ltr,
  child: HeyoTheme(data: theme, child: Center(child: child)),
);

void main() {
  test('an emoji is told from the name of an image by what it is made of', () {
    // (What src/raycast/glyph.ts says of the same strings.)
    for (final emoji in ['😀', '👨‍👩‍👧‍👦', '🇯🇵', '1️⃣', '#️⃣', '🏴󠁧󠁢󠁷󠁬󠁳󠁿', '👍🏽']) {
      expect(isEmoji(emoji), isTrue, reason: emoji);
      expect(isGlyph(emoji), isTrue, reason: emoji);
    }
    for (final plain in ['1', 'ab', 'abcde', '文', 'icon.png']) {
      expect(isEmoji(plain), isFalse, reason: plain);
    }
    // A symbol of a few characters is drawn as it is, too.
    for (final glyph in ['→', '✓', '⌘', 'ab', 'abcd', '文']) {
      expect(isGlyph(glyph), isTrue, reason: glyph);
    }
    for (final name in ['abcde', 'a.b', 'icon.png', '/usr/share/icons/a.png', 'https://example.com/a.png', 'data:image/png;base64,AAAA', '']) {
      expect(isGlyph(name), isFalse, reason: name);
    }
    expect(isFilePath('/usr/share/icons/a.png'), isTrue);
    expect(isFilePath(r'C:\Users\ada\a.png'), isTrue);
    expect(isFilePath('assets/a.png'), isFalse);
  });

  test('an image is taken apart as Raycast puts one together', () {
    expect(resolveImage('star-16', dark: false).src, 'star-16');
    expect(resolveImage({'fileIcon': '/home/ada/notes.txt'}, dark: false).src, 'document-16');
    final themed = {'source': {'light': 'sun.png', 'dark': 'moon.png'}, 'tintColor': 'raycast-blue', 'mask': 'circle', 'fallback': 'star-16'};
    expect(resolveImage(themed, dark: false).src, 'sun.png');
    final inDark = resolveImage(themed, dark: true);
    expect((inDark.src, inDark.tint, inDark.mask, inDark.fallback), ('moon.png', 'raycast-blue', 'circle', 'star-16'));
    // Only one of the two drawn: it serves both schemes.
    expect(resolveImage({'source': {'dark': 'moon.png'}}, dark: false).src, 'moon.png');
    expect(resolveImage({'value': 'star-16', 'tooltip': 'Starred'}, dark: false).src, 'star-16');
    expect(resolveImage(null, dark: false).src, isNull);
    expect(resolveImage(3, dark: false).src, isNull);
  });

  test('a symbol is a named icon, a glyph, or a picture that is tinted', () {
    expect(isSymbol('star-16'), isTrue);
    expect(isSymbol('😀'), isTrue);
    expect(isSymbol({'source': 'icon.png', 'tintColor': '#ff0000'}), isTrue);
    expect(isSymbol('icon.png'), isFalse);
    expect(isSymbol('/usr/share/icons/firefox.png'), isFalse);
    expect(isSymbol({'source': 'icon.png'}), isFalse);
    expect(isSymbol(null), isFalse);
    expect(isSymbol(''), isFalse);
  });

  test('a colour is one of Raycast\'s by name, a CSS colour, or one for each scheme', () {
    expect(raycastColor('raycast-blue', _light), HeyoColors.light.raycast['blue']);
    expect(raycastColor('raycast-blue', _dark), HeyoColors.dark.raycast['blue']);
    expect(raycastColor('raycast-primary-text', _dark), HeyoColors.dark.text);
    expect(raycastColor('raycast-secondary-text', _light), HeyoColors.light.textMuted);
    expect(raycastColor('#ff0000', _light), const Color(0xFFFF0000));
    expect(raycastColor({'light': '#000', 'dark': '#fff'}, _light), const Color(0xFF000000));
    expect(raycastColor({'light': '#000', 'dark': '#fff'}, _dark), const Color(0xFFFFFFFF));
    // Only the dark one given: it serves both.
    expect(raycastColor({'dark': '#fff'}, _light), const Color(0xFFFFFFFF));
    expect(raycastColor(null, _light), isNull);
    expect(raycastColor(12, _light), isNull);
  });

  test('CSS colours are read as CSS writes them', () {
    expect(parseCssColor('#abc'), const Color(0xFFAABBCC));
    expect(parseCssColor('#A1B2C3'), const Color(0xFFA1B2C3));
    // The alpha comes last in CSS.
    expect(parseCssColor('#ff000080'), const Color(0x80FF0000));
    expect(parseCssColor('rgb(255, 0, 0)'), const Color(0xFFFF0000));
    expect(parseCssColor('rgba(0, 0, 0, 0.5)')!.a, closeTo(0.5, 0.01));
    expect(parseCssColor('rgb(0 128 255 / 50%)')!.a, closeTo(0.5, 0.01));
    expect(parseCssColor('white'), const Color(0xFFFFFFFF));
    expect(parseCssColor('#12'), isNull);
    expect(parseCssColor('chartreuse-ish'), isNull);
  });

  test('a tile has the same colour for the same name, every time', () {
    // (What src/ItemIcon.tsx gives the same names.)
    expect(tileColorName('Firefox'), 'blue');
    expect(tileColorName('AI Chat'), 'purple');
    expect(tileColorName('测试启动器'), 'red');
    expect(tileColorName('Clipboard History'), 'magenta');
    expect(tileColorName('😀 emoji'), 'red');
    expect(tileColorName('A very long name that overflows thirty two bits surely'), 'red');
  });

  testWidgets('a named icon is drawn from the icon font, in the tint or the text\'s colour', (tester) async {
    await tester.pumpWidget(_in(_light, const HeyoImage('star-16', size: 18)));
    final plain = tester.widget<Icon>(find.byType(Icon));
    expect((plain.size, plain.color), (18, HeyoColors.light.text));
    await tester.pumpWidget(_in(_dark, const HeyoImage({'source': 'star-16', 'tintColor': 'raycast-red'})));
    expect(tester.widget<Icon>(find.byType(Icon)).color, HeyoColors.dark.raycast['red']);
  });

  testWidgets('a number, an emoji and nothing at all each take the size asked', (tester) async {
    await tester.pumpWidget(_in(_light, const HeyoImage('number-07-16', size: 20)));
    expect(find.text('07'), findsOneWidget);
    await tester.pumpWidget(_in(_light, const HeyoImage('😀', size: 20)));
    expect(find.text('😀'), findsOneWidget);
    expect(tester.getSize(find.byType(HeyoImage)), const Size(20, 20));
    await tester.pumpWidget(_in(_light, const HeyoImage(null, size: 20)));
    expect(tester.getSize(find.byType(HeyoImage)), const Size(20, 20));
    // An asset's name means nothing outside an extension.
    await tester.pumpWidget(_in(_light, const HeyoImage('icon.png', size: 20)));
    expect(find.byType(Image), findsNothing);
  });

  testWidgets('an asset is looked for in the running extension', (tester) async {
    await tester.pumpWidget(_in(_light, const ExtensionDir(path: '/ext/todo', child: HeyoImage('icon.png'))));
    final image = tester.widget<Image>(find.byType(Image)).image as ResizeImage;
    expect((image.imageProvider as FileImage).file.path, '/ext/todo/assets/icon.png');
  });

  testWidgets('the icon of a row is a picture, a tile, a plain symbol or an initial', (tester) async {
    await tester.pumpWidget(_in(_light, const ItemIcon(icon: null, title: ' firefox')));
    expect(find.text('F'), findsOneWidget);
    expect(tester.getSize(find.byType(ItemIcon)), const Size(22, 22));

    await tester.pumpWidget(_in(_light, const ItemIcon(icon: '', title: '')));
    expect(find.text('?'), findsOneWidget);

    // A command's symbol: white, on a tile.
    await tester.pumpWidget(_in(_light, const ItemIcon(icon: 'star-16', title: 'Favorites')));
    var symbol = tester.widget<Icon>(find.byType(Icon));
    expect((symbol.size, symbol.color), (15, HeyoColors.white));
    expect(tester.getSize(find.byType(ItemIcon)), const Size(22, 22));

    // Inside a command's own list: plain, muted until its row is selected.
    await tester.pumpWidget(_in(_light, const ItemIcon(icon: 'star-16', title: 'Favorites', variant: ItemIconVariant.plain)));
    symbol = tester.widget<Icon>(find.byType(Icon));
    expect((symbol.size, symbol.color), (18, HeyoColors.light.textMuted));
    await tester.pumpWidget(_in(_light, const ItemIcon(icon: 'star-16', title: 'Favorites', variant: ItemIconVariant.plain, selected: true)));
    expect(tester.widget<Icon>(find.byType(Icon)).color, HeyoColors.light.text);

    // An application's own icon, as it comes.
    await tester.pumpWidget(_in(_light, const ItemIcon(icon: '/usr/share/icons/firefox.png', title: 'Firefox')));
    expect(find.byType(Image), findsOneWidget);
    expect(tester.getSize(find.byType(ItemIcon)), const Size(22, 22));
  });
}
