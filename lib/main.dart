import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_ui/remya_ui.dart';

import 'app.dart';
import 'features/clipboard/clipboard_platform.dart';
import 'features/files/file_icon_tile.dart';
import 'features/preview/launcher_examples.dart';

Future<void> main(List<String> arguments) async {
  WidgetsFlutterBinding.ensureInitialized();
  LicenseRegistry.addLicense(() async* {
    yield LicenseEntryWithLineBreaks([
      'Remya Launcher',
    ], await rootBundle.loadString('LICENSE'));
    yield LicenseEntryWithLineBreaks([
      'Google Material Symbols',
    ], await rootBundle.loadString('assets/licenses/material-symbols.txt'));
    yield LicenseEntryWithLineBreaks([
      'Material Icon Theme',
    ], await rootBundle.loadString('assets/material_icon_theme/LICENSE'));
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
  final selectedExample = preview == null
      ? null
      : example ?? (clipboardType == null ? .empty : .clipboard);
  final settingsIndex = arguments.indexOf('--settings');
  final settingsPage = settingsIndex < 0
      ? null
      : settingsIndex + 1 < arguments.length &&
            !arguments[settingsIndex + 1].startsWith('--')
      ? arguments[settingsIndex + 1]
      : '';
  if (settingsPage != null && !['', 'clipboard'].contains(settingsPage)) {
    throw ArgumentError.value(settingsPage, 'page', 'Unknown settings page');
  }
  final theme = SettingsThemeController.service();
  // Bound the on-demand startup; a late theme still applies when it arrives.
  await Future.wait([
    theme.start().timeout(const Duration(milliseconds: 500), onTimeout: () {}),
    FileIconAssets.ensureLoaded(),
  ]);
  runApp(
    RemyaLauncherApp(
      theme: theme,
      example: selectedExample,
      settingsPage: settingsPage,
      clipboardType: clipboardType ?? 'command',
      startInClipboard: arguments.contains('--clipboard'),
      clipboardPlatform: selectedExample == null && settingsPage == null
          ? await ClipboardPlatform.capture()
          : const ClipboardPlatform(),
    ),
  );
}
