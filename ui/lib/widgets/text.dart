// Text at a size of the scale.
//
// Use this rather than `Text`: besides the size's style it fixes the line
// the text is set on. Chinese, Japanese and Korean come from other fonts
// than Latin does, with other heights, and a line left to work its own
// height out moves by a pixel or two when one of their characters is in it.

import 'package:flutter/widgets.dart';

import '../theme/theme.dart';

class HeyoText extends StatelessWidget {
  const HeyoText(
    this.text, {
    super.key,
    this.size = TextSize.md,
    this.weight = Weights.regular,
    this.color,
    this.tabular = false,
    this.mono = false,
    this.maxLines = 1,
    this.fade = false,
    this.align,
  });
  final String text;
  final TextSize size;
  final FontWeight weight;

  /// The text's colour, unless given.
  final Color? color;

  /// Numbers that keep their columns.
  final bool tabular;
  final bool mono;

  /// One line, cut off with an ellipsis, unless more are allowed; null for
  /// as many as it takes.
  final int? maxLines;

  /// One line that fades out at the end instead of being cut off.
  final bool fade;
  final TextAlign? align;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    return Text(
      text,
      maxLines: fade ? 1 : maxLines,
      softWrap: !fade && maxLines != 1,
      overflow: fade ? TextOverflow.fade : (maxLines == null ? TextOverflow.clip : TextOverflow.ellipsis),
      textAlign: align,
      style: theme.text(size, weight: weight, color: color, tabular: tabular, mono: mono),
      strutStyle: theme.strut(size),
    );
  }
}
