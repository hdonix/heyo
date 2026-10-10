// Draws a Raycast `Image.ImageLike`: an icon name, URL, file path, asset
// name, emoji, or `{ source, tintColor, mask, fallback }`. The rules are the
// web view's (src/raycast/Image.tsx, glyph.ts and `cssColor` in model.ts),
// kept in the same order so that the two can be read side by side.

import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../theme/theme.dart';
import 'raycast_icons.g.dart';

/// The directory of the running extension, for resolving asset names.
class ExtensionDir extends InheritedWidget {
  const ExtensionDir({super.key, required this.path, required super.child});
  final String path;

  static String of(BuildContext context) => context.dependOnInheritedWidgetOfExactType<ExtensionDir>()?.path ?? '';

  @override
  bool updateShouldNotify(ExtensionDir old) => old.path != path;
}

// --- Telling an emoji or a short symbol from the name of an image ----------

bool _pictographic(int rune) =>
    (rune >= 0x1F000 && rune <= 0x1FAFF) || // emoji, symbols, pictographs
    (rune >= 0x2600 && rune <= 0x27BF) || // miscellaneous symbols, dingbats
    (rune >= 0x2190 && rune <= 0x21FF) || // arrows
    (rune >= 0x2300 && rune <= 0x23FF) || // technical
    (rune >= 0x2B00 && rune <= 0x2BFF) || // more arrows and stars
    (rune >= 0x2900 && rune <= 0x297F) ||
    rune == 0xA9 || rune == 0xAE || rune == 0x203C || rune == 0x2049 || rune == 0x2122 || rune == 0x2139 || rune == 0x24C2 || rune == 0x3030 || rune == 0x303D || rune == 0x3297 || rune == 0x3299 ||
    (rune >= 0x25A0 && rune <= 0x25FF);

bool _regional(int rune) => rune >= 0x1F1E6 && rune <= 0x1F1FF;

/// What may follow the start of an emoji: more of the same joined by ZWJ,
/// skin tones, variation selectors, the keycap mark, and the tag characters
/// that spell a flag's region.
bool _continues(int rune) =>
    _pictographic(rune) || _regional(rune) || (rune >= 0x1F3FB && rune <= 0x1F3FF) || rune == 0x200D || rune == 0xFE0E || rune == 0xFE0F || rune == 0x20E3 || (rune >= 0xE0020 && rune <= 0xE007F);

/// An emoji can be many code points long (a family is seven), so its length
/// says nothing: what it is made of does.
bool isEmoji(String text) {
  final runes = text.runes.toList();
  if (runes.isEmpty) return false;
  final first = runes.first;
  final keycap = (first >= 0x30 && first <= 0x39) || first == 0x23 || first == 0x2A;
  if (keycap) {
    // A digit, # or * is an emoji only as a keycap.
    final next = runes.skip(1).where((rune) => rune != 0xFE0F);
    if (next.isEmpty || next.first != 0x20E3) return false;
  } else if (!_pictographic(first) && !_regional(first)) {
    return false;
  }
  return runes.skip(1).every(_continues);
}

final _location = RegExp(r'^(https?|data|blob):|^/');

/// Whether a source names a file on this computer: `/usr/share/…`, or
/// `C:\Users\…` on Windows.
bool isFilePath(String source) => source.startsWith('/') || RegExp(r'^[A-Za-z]:[\\/]').hasMatch(source);

/// Whether `source` is drawn as text: an emoji, or a symbol of a few characters.
bool isGlyph(String source) {
  if (source.isEmpty || _location.hasMatch(source) || source.contains('.')) return false;
  return isEmoji(source) || source.runes.length <= 4;
}

final _iconName = RegExp(r'^[a-z0-9-]+-16$');
final _number = RegExp(r'^number-(\d+)-16$');

// --- Colours -----------------------------------------------------------------

/// The colour of a Raycast `ColorLike`: a name of Raycast's, a CSS colour
/// (`#rgb`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`), or `{ light, dark }`.
Color? raycastColor(Object? color, HeyoThemeData theme) {
  if (color is String) {
    final name = color.trim();
    if (name.startsWith('raycast-')) {
      final own = name.substring(8);
      if (own == 'primary-text') return theme.colors.text;
      if (own == 'secondary-text') return theme.colors.textMuted;
      final named = theme.colors.raycast[own];
      if (named != null) return named;
    }
    return parseCssColor(name);
  }
  if (color is Map && color.containsKey('dark')) {
    final dark = raycastColor(color['dark'], theme);
    final light = raycastColor(color['light'], theme) ?? dark;
    return theme.isDark ? dark : light;
  }
  return null;
}

const _cssNames = {
  'black': Color(0xFF000000),
  'white': Color(0xFFFFFFFF),
  'red': Color(0xFFFF0000),
  'green': Color(0xFF008000),
  'blue': Color(0xFF0000FF),
  'yellow': Color(0xFFFFFF00),
  'orange': Color(0xFFFFA500),
  'purple': Color(0xFF800080),
  'gray': Color(0xFF808080),
  'grey': Color(0xFF808080),
  'transparent': Color(0x00000000),
};

/// A colour as CSS writes the common ones. Null for anything else.
Color? parseCssColor(String text) {
  final value = text.trim().toLowerCase();
  if (value.startsWith('#')) {
    var hex = value.substring(1);
    if (hex.length == 3 || hex.length == 4) hex = hex.split('').map((c) => '$c$c').join();
    if (hex.length == 6) hex = '${hex}ff';
    final parsed = hex.length == 8 ? int.tryParse(hex, radix: 16) : null;
    // CSS puts the alpha last; Flutter first.
    return parsed == null ? null : Color(((parsed & 0xFF) << 24) | (parsed >> 8));
  }
  final call = RegExp(r'^rgba?\(([^)]*)\)$').firstMatch(value);
  if (call != null) {
    final parts = call.group(1)!.split(RegExp(r'[\s,/]+')).where((part) => part.isNotEmpty).toList();
    if (parts.length < 3) return null;
    int? channel(String part) => part.endsWith('%') ? ((double.tryParse(part.substring(0, part.length - 1)) ?? 0) * 2.55).round() : double.tryParse(part)?.round();
    final rgb = [for (final part in parts.take(3)) channel(part)];
    if (rgb.contains(null)) return null;
    final alpha = parts.length > 3 ? (parts[3].endsWith('%') ? (double.tryParse(parts[3].substring(0, parts[3].length - 1)) ?? 100) / 100 : double.tryParse(parts[3]) ?? 1) : 1.0;
    return Color.fromRGBO(rgb[0]!.clamp(0, 255), rgb[1]!.clamp(0, 255), rgb[2]!.clamp(0, 255), alpha.clamp(0, 1).toDouble());
  }
  return _cssNames[value];
}

// --- What an image is ----------------------------------------------------------

/// An image, taken apart.
class ResolvedImage {
  const ResolvedImage({this.src, this.tint, this.mask, this.fallback});
  final String? src;

  /// A `ColorLike`, not yet a colour: that depends on the scheme.
  final Object? tint;
  final String? mask;
  final String? fallback;
}

String? _sourceOf(Object? source, bool dark) {
  if (source is String) return source;
  if (source is Map) {
    // The one drawn for the scheme in use.
    final light = source['light'] as String?, darker = source['dark'] as String?;
    return dark ? (darker ?? light) : (light ?? darker);
  }
  return null;
}

ResolvedImage resolveImage(Object? image, {required bool dark}) {
  if (image is String) return ResolvedImage(src: image);
  if (image is! Map) return const ResolvedImage();
  if (image.containsKey('fileIcon')) return const ResolvedImage(src: 'document-16');
  if (image.containsKey('source')) {
    return ResolvedImage(src: _sourceOf(image['source'], dark), tint: image['tintColor'], mask: image['mask'] as String?, fallback: _sourceOf(image['fallback'], dark));
  }
  if (image.containsKey('value')) return resolveImage(image['value'], dark: dark);
  return const ResolvedImage();
}

/// Whether an image is a symbol (a named icon, a glyph, or a template that
/// is tinted) rather than a picture with colours of its own.
bool isSymbol(Object? image) {
  final resolved = resolveImage(image, dark: false);
  final src = resolved.src;
  if (src == null || src.isEmpty) return false;
  return resolved.tint != null || _iconName.hasMatch(src) || isGlyph(src);
}

/// `fallback_text` → `fallbackText`, `grid-3x3` → `grid3x3`.
String _camel(String kebab) {
  final words = kebab.split('-');
  return words.first + words.skip(1).map((word) => word.isEmpty ? '' : word[0].toUpperCase() + word.substring(1)).join();
}

/// The Lucide icon a Raycast icon name stands for, and whether the name
/// asks for it filled. Null when there is none of that name.
({IconData icon, bool filled})? raycastIcon(String name) {
  final base = name.endsWith('-16') ? name.substring(0, name.length - 3) : name;
  final unfilled = base.endsWith('-filled') ? base.substring(0, base.length - 7) : base;
  final target = raycastAliases[base] ?? raycastAliases[unfilled] ?? _camel(unfilled);
  final icon = lucideIcons[target];
  return icon == null ? null : (icon: icon, filled: base != unfilled);
}

/// The image itself.
class HeyoImage extends StatefulWidget {
  const HeyoImage(this.image, {super.key, this.size = 20, this.color});

  /// A Raycast `ImageLike`, as decoded JSON.
  final Object? image;
  final double size;

  /// The colour of a symbol that names none: the text's, by default.
  final Color? color;

  @override
  State<HeyoImage> createState() => _HeyoImageState();
}

class _HeyoImageState extends State<HeyoImage> {
  var _failed = false;

  @override
  void didUpdateWidget(HeyoImage old) {
    super.didUpdateWidget(old);
    if (old.image != widget.image) _failed = false;
  }

  void _fail(String? fallback) {
    if (fallback != null && !_failed && mounted) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) setState(() => _failed = true);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    final resolved = resolveImage(widget.image, dark: theme.isDark);
    final src = _failed ? resolved.fallback : resolved.src;
    final size = widget.size;
    final empty = SizedBox(width: size, height: size);
    if (src == null || src.isEmpty) return empty;
    final tint = raycastColor(resolved.tint, theme);
    final ink = tint ?? widget.color ?? DefaultTextStyle.of(context).style.color ?? theme.colors.text;

    if (_iconName.hasMatch(src)) {
      final number = _number.firstMatch(src);
      if (number != null) return _glyph(theme, number.group(1)!, size, TextSize.xxs.size, ink);
      final icon = raycastIcon(src);
      // A filled symbol cannot be had from an icon font: it is drawn in
      // outline, as its unfilled namesake.
      if (icon != null) return Icon(icon.icon, size: size, color: ink);
    }

    final Widget picture;
    void failed() => _fail(resolved.fallback);
    Widget broken(BuildContext _, Object _, StackTrace? _) {
      failed();
      return empty;
    }

    final filter = tint == null ? null : ColorFilter.mode(tint, BlendMode.srcIn);
    if (src.startsWith('data:image/svg+xml,') && src.contains('<')) {
      // Raw SVG markup.
      picture = SvgPicture.string(src.substring(19), width: size, height: size, colorFilter: filter, errorBuilder: broken);
    } else if (src.startsWith('data:')) {
      final UriData data;
      try {
        data = UriData.parse(src);
      } on FormatException {
        failed();
        return empty;
      }
      picture = data.mimeType.contains('svg')
          ? SvgPicture.memory(data.contentAsBytes(), width: size, height: size, colorFilter: filter, errorBuilder: broken)
          : Image.memory(data.contentAsBytes(), width: size, height: size, fit: BoxFit.contain, errorBuilder: broken);
    } else if (src.startsWith('http://') || src.startsWith('https://')) {
      picture = _isSvg(src)
          ? SvgPicture.network(src, width: size, height: size, colorFilter: filter, errorBuilder: broken, placeholderBuilder: (_) => empty)
          : Image.network(src, width: size, height: size, fit: BoxFit.contain, errorBuilder: broken);
    } else if (isFilePath(src)) {
      picture = _file(src, size, filter, broken);
    } else if (isGlyph(src)) {
      // An emoji or a short symbol.
      return _glyph(theme, src, size, size * 0.85, ink);
    } else {
      final dir = ExtensionDir.of(context);
      if (dir.isEmpty) return empty;
      picture = _file('$dir/assets/$src', size, filter, broken);
    }

    // A tinted image is a template: its alpha channel masks the tint colour.
    Widget out = filter != null && picture is! SvgPicture ? ColorFiltered(colorFilter: filter, child: picture) : picture;
    final radius = switch (resolved.mask) {
      'circle' => size / 2,
      'roundedRectangle' => size * 0.22,
      _ => 0.0,
    };
    if (radius > 0) out = ClipRRect(borderRadius: BorderRadius.circular(radius), child: out);
    return SizedBox(width: size, height: size, child: out);
  }

  static bool _isSvg(String path) => path.split('?').first.toLowerCase().endsWith('.svg');

  Widget _file(String path, double size, ColorFilter? filter, ImageErrorWidgetBuilder broken) {
    final file = File(path);
    if (_isSvg(path)) return SvgPicture.file(file, width: size, height: size, colorFilter: filter, errorBuilder: broken);
    // Decoded at the size it is shown (times two, for displays that show
    // two pixels to one): application icons come as large as 1024 pixels.
    final pixels = (size * 2 * MediaQuery.devicePixelRatioOf(context)).ceil();
    return Image.file(file, width: size, height: size, fit: BoxFit.contain, cacheWidth: pixels, filterQuality: FilterQuality.medium, errorBuilder: broken);
  }

  Widget _glyph(HeyoThemeData theme, String text, double size, double fontSize, Color color) => SizedBox(
    width: size,
    height: size,
    child: Center(
      child: Text(
        text,
        maxLines: 1,
        softWrap: false,
        overflow: TextOverflow.visible,
        textAlign: TextAlign.center,
        style: theme.text(TextSize.xxs, color: color).copyWith(fontSize: fontSize, height: 1),
      ),
    ),
  );
}
