import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_ui/remya_ui.dart';

import 'features/apps/apps_client.dart';
import 'features/clipboard/clipboard_client.dart';
import 'features/clipboard/clipboard_launcher.dart';
import 'features/clipboard/clipboard_platform.dart';
import 'features/clipboard/clipboard_settings.dart';
import 'features/emoji/emoji_results.dart';
import 'features/emoji/emoji_settings.dart';
import 'features/files/file_icon_palette.dart';
import 'features/files/files_client.dart';
import 'features/files/files_preferences.dart';
import 'features/launcher/launcher_controls.dart';
import 'features/launcher/resident_launcher.dart';
import 'features/preview/launcher_examples.dart';
import 'features/web/web_client.dart';

class RemyaLauncherApp extends StatelessWidget {
  const RemyaLauncherApp({
    this.example,
    this.settingsPage,
    this.clipboardType = 'command',
    this.initialScope = .all,
    this.resident = false,
    this.clipboardClient = const ClipboardClient(),
    this.clipboardPlatform = const ClipboardPlatform(),
    this.theme,
    this.settingsClient,
    this.appsClient,
    this.webClient,
    this.filesClient,
    super.key,
  });

  final LauncherExample? example;
  final String? settingsPage;
  final String clipboardType;
  final LauncherScope initialScope;
  final bool resident;
  final ClipboardClient clipboardClient;
  final ClipboardPlatform clipboardPlatform;

  /// Shared Remya theme; it keeps the last valid theme when reads fail.
  final SettingsThemeController? theme;

  /// Remya Core connection for app preferences; null connects on demand.
  final SettingsClient? settingsClient;
  final AppsClient? appsClient;
  final WebClient? webClient;
  final FilesClient? filesClient;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: Listenable.merge([?theme]),
    builder: (context, _) {
      final shared = theme?.theme ?? const RemyaTheme();
      return MaterialApp(
        title: settingsPage == null
            ? 'Remy\'a Launcher'
            : 'Remy\'a Launcher Settings',
        debugShowCheckedModeBanner: false,
        localizationsDelegates: GlobalMaterialLocalizations.delegates,
        supportedLocales: const [
          Locale('en'),
          Locale.fromSubtags(languageCode: 'zh', scriptCode: 'Hans'),
          Locale.fromSubtags(languageCode: 'zh', scriptCode: 'Hant'),
          Locale('ja'),
          Locale('ru'),
          Locale('de'),
          Locale('es'),
        ],
        localeListResolutionCallback: (locales, supported) =>
            basicLocaleListResolution(
              locales?.map((locale) {
                if (locale.languageCode != 'zh' || locale.scriptCode != null) {
                  return locale;
                }
                return Locale.fromSubtags(
                  languageCode: 'zh',
                  scriptCode: emojiLocaleTag(locale).split('-').last,
                  countryCode: locale.countryCode,
                );
              }).toList(),
              supported,
            ),
        // The settings window uses the shared theme unchanged, like Remya Settings.
        theme: settingsPage == null
            ? _launcherTheme(shared.theme(.light), shared.seedColor)
            : shared.theme(.light),
        darkTheme: settingsPage == null
            ? _launcherTheme(shared.theme(.dark), shared.seedColor)
            : shared.theme(.dark),
        themeMode: shared.mode,
        home: _home(),
      );
    },
  );

  Widget _home() => settingsPage != null
      ? ClipboardSettingsWindow(
          initialPage: settingsPage,
          client: settingsClient,
          clipboard: clipboardClient,
          apps: appsClient,
          web: webClient,
        )
      : switch (example) {
          final LauncherExample example => Scaffold(
            body: Center(
              child: Padding(
                padding: const .all(16),
                child: LauncherExampleView(
                  example: example,
                  clipboardType: clipboardType,
                ),
              ),
            ),
          ),
          null => Scaffold(
            backgroundColor: Colors.transparent,
            body: resident
                ? ResidentLauncher(clipboard: clipboardClient)
                : ClipboardLauncher(
                    initialScope: initialScope,
                    client: clipboardClient,
                    appsClient: appsClient,
                    webClient: webClient,
                    filesClient: filesClient,
                    filesPreferences: FilesPreferences(client: settingsClient),
                    platform: clipboardPlatform,
                    loadEmojiPreferences: settingsClient == null
                        ? null
                        : () => EmojiPreferences.read(settingsClient),
                  ),
          ),
        };
}

/// Keeps the launcher's typography and source seed on top of the shared Remya color scheme.
ThemeData _launcherTheme(ThemeData base, Color sourceSeed) {
  final colors = base.colorScheme;
  return base.copyWith(
    extensions: [...base.extensions.values, RemyaSourceSeed(sourceSeed)],
    textTheme: base.textTheme
        .apply(fontFamily: 'Inter')
        .merge(
          TextTheme(
            bodyMedium: TextStyle(letterSpacing: 0, color: colors.onSurface),
            bodySmall: TextStyle(
              fontSize: 12,
              height: 16 / 12,
              letterSpacing: 0,
              color: colors.onSurfaceVariant,
            ),
            labelSmall: TextStyle(
              fontSize: 11,
              height: 14 / 11,
              fontWeight: .w600,
              letterSpacing: .6,
              color: colors.onSurfaceVariant,
            ),
          ),
        ),
  );
}
