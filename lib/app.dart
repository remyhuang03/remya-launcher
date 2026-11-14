import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_ui/remya_ui.dart';

import 'features/clipboard/clipboard_client.dart';
import 'features/clipboard/clipboard_launcher.dart';
import 'features/clipboard/clipboard_platform.dart';
import 'features/clipboard/clipboard_settings.dart';
import 'features/files/file_icon_palette.dart';
import 'features/preview/launcher_examples.dart';

class RemyaLauncherApp extends StatelessWidget {
  const RemyaLauncherApp({
    this.example,
    this.settingsPage,
    this.clipboardType = 'command',
    this.startInClipboard = false,
    this.clipboardClient = const ClipboardClient(),
    this.clipboardPlatform = const ClipboardPlatform(),
    this.theme,
    this.settingsClient,
    super.key,
  });

  final LauncherExample? example;
  final String? settingsPage;
  final String clipboardType;
  final bool startInClipboard;
  final ClipboardClient clipboardClient;
  final ClipboardPlatform clipboardPlatform;

  /// Shared Remya theme; it keeps the last valid theme when reads fail.
  final SettingsThemeController? theme;

  /// Remya Core connection for the settings window; null connects on demand.
  final SettingsClient? settingsClient;

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: Listenable.merge([?theme]),
    builder: (context, _) {
      final shared = theme?.theme ?? const RemyaTheme();
      return MaterialApp(
        title: settingsPage == null
            ? 'Remya Launcher'
            : 'Remya Launcher Settings',
        debugShowCheckedModeBanner: false,
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
          client: settingsClient,
          clipboard: clipboardClient,
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
            body: ClipboardLauncher(
              startInClipboard: startInClipboard,
              client: clipboardClient,
              platform: clipboardPlatform,
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
