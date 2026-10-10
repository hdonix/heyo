// What a command of an extension shows: lists, grids, details and forms
// (src/raycast/ in the web view), drawn from what the runtime sends as
// `heyo://runtime` events. A placeholder: the palette shows this when a
// command has taken it over.

import 'package:flutter/widgets.dart';

import '../widgets/placeholder.dart';

class ExtensionView extends StatelessWidget {
  const ExtensionView({super.key});

  @override
  Widget build(BuildContext context) => const ViewPlaceholder('Command', note: 'What commands show is not drawn by Flutter yet. Escape goes back.');
}
