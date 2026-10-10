// What stands in a window whose view has not been written yet.

import 'package:flutter/widgets.dart';

import '../theme/theme.dart';
import 'text.dart';

class ViewPlaceholder extends StatelessWidget {
  const ViewPlaceholder(this.name, {super.key, this.note});
  final String name;
  final String? note;

  @override
  Widget build(BuildContext context) {
    final theme = HeyoTheme.of(context);
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          HeyoText(name, size: TextSize.lg, weight: Weights.semibold),
          SizedBox(height: space(1)),
          HeyoText(note ?? 'This view is not drawn by Flutter yet.', size: TextSize.sm, color: theme.colors.textMuted, maxLines: null, align: TextAlign.center),
        ],
      ),
    );
  }
}
