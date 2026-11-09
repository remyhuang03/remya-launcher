import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import 'app.dart';
import 'features/preview/launcher_examples.dart';

void main(List<String> arguments) {
  WidgetsFlutterBinding.ensureInitialized();
  LicenseRegistry.addLicense(() async* {
    yield LicenseEntryWithLineBreaks([
      'Remya Launcher',
    ], await rootBundle.loadString('LICENSE'));
    yield LicenseEntryWithLineBreaks([
      'Google Material Symbols',
    ], await rootBundle.loadString('assets/licenses/material-symbols.txt'));
  });
  final examples = {
    for (final example in LauncherExample.values) example.name: example,
  };
  final preview = arguments
      .where((argument) => argument.startsWith('--preview='))
      .map((argument) => argument.substring('--preview='.length))
      .firstOrNull;
  final example = preview == 'quick-pick'
      ? LauncherExample.quickPick
      : examples[preview];
  final clipboardType = switch (preview) {
    'clipboard-text' => 'text',
    'clipboard-code' => 'code',
    'clipboard-image' => 'image',
    'clipboard-link' => 'link',
    'clipboard-color' => 'color',
    'clipboard-files' => 'files',
    'clipboard-hidden' => 'hidden',
    _ => null,
  };
  runApp(
    RemyaLauncherApp(
      example: example ?? (clipboardType == null ? .empty : .clipboard),
      clipboardType: clipboardType ?? 'command',
    ),
  );
}
