import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_ui/remya_ui.dart';
import 'package:remya_ui/settings_ui.dart';

import 'app.dart';
import 'features/apps/apps_client.dart';
import 'features/clipboard/clipboard_platform.dart';
import 'features/emoji/emoji_settings.dart';
import 'features/files/file_icon_tile.dart';
import 'features/preview/launcher_examples.dart';
import 'features/web/web_client.dart';

/// Parses the optional `--settings [page]` launch argument.
String? parseSettingsPage(List<String> arguments) {
  final settingsIndex = arguments.indexOf('--settings');
  final settingsPage = settingsIndex < 0
      ? null
      : settingsIndex + 1 < arguments.length &&
            !arguments[settingsIndex + 1].startsWith('--')
      ? arguments[settingsIndex + 1]
      : '';
  if (settingsPage != null &&
      ![
        '',
        'clipboard',
        'apps',
        'apps-preferences',
        'hidden-apps',
        'web',
        emojiSettingsPageId,
        remyaMaterialThemePageId,
      ].contains(settingsPage)) {
    throw ArgumentError.value(settingsPage, 'page', 'Unknown settings page');
  }
  return settingsPage;
}

Future<void> main(List<String> arguments) async {
  WidgetsFlutterBinding.ensureInitialized();
  LicenseRegistry.addLicense(() async* {
    yield LicenseEntryWithLineBreaks([
      'Remy\'a Launcher',
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
  final settingsPage = parseSettingsPage(arguments);
  final theme = SettingsThemeController.service();
  await Future.wait([theme.start(), FileIconAssets.ensureLoaded()]);
  runApp(
    RemyaLauncherApp(
      theme: theme,
      example: selectedExample,
      settingsPage: settingsPage,
      appsClient: selectedExample == null && settingsPage == null
          ? AppsClient()
          : null,
      webClient: selectedExample == null ? WebClient() : null,
      clipboardType: clipboardType ?? 'command',
      startInClipboard: arguments.contains('--clipboard'),
      clipboardPlatform: selectedExample == null && settingsPage == null
          ? await ClipboardPlatform.capture()
          : const ClipboardPlatform(),
    ),
  );
}
