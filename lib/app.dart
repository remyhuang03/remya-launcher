import 'package:material_ui/material_ui.dart';

import 'features/clipboard/clipboard_client.dart';
import 'features/clipboard/clipboard_launcher.dart';
import 'features/clipboard/clipboard_platform.dart';
import 'features/preview/launcher_examples.dart';

class RemyaLauncherApp extends StatelessWidget {
  const RemyaLauncherApp({
    this.example,
    this.clipboardType = 'command',
    this.startInClipboard = false,
    this.clipboardClient = const ClipboardClient(),
    this.clipboardPlatform = const ClipboardPlatform(),
    super.key,
  });

  final LauncherExample? example;
  final String clipboardType;
  final bool startInClipboard;
  final ClipboardClient clipboardClient;
  final ClipboardPlatform clipboardPlatform;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Remya Launcher',
      debugShowCheckedModeBanner: false,
      theme: _launcherTheme(),
      home: switch (example) {
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
      },
    );
  }
}

ThemeData _launcherTheme() {
  final colors =
      ColorScheme.fromSeed(
        seedColor: const Color(0xff6750a4),
        brightness: .dark,
        dynamicSchemeVariant: .expressive,
      ).copyWith(
        primary: const Color(0xffd0bcff),
        primaryContainer: const Color(0xff4f378b),
        onPrimaryContainer: const Color(0xffeaddff),
        secondaryContainer: const Color(0xff4a4458),
        onSecondaryContainer: const Color(0xffe8def8),
        surface: const Color(0xff141218),
        onSurface: const Color(0xffe6e0e9),
        surfaceContainer: const Color(0xff211f26),
        surfaceContainerHigh: const Color(0xff2b2930),
        surfaceContainerHighest: const Color(0xff36343b),
        onSurfaceVariant: const Color(0xffcac4d0),
        outlineVariant: const Color(0xff49454f),
        outline: const Color(0xff938f99),
        error: const Color(0xfff2b8b5),
        tertiary: const Color(0xffefb8c8),
        tertiaryContainer: const Color(0xff633b48),
        onTertiaryContainer: const Color(0xffffd8e4),
      );
  return ThemeData(
    useMaterial3: true,
    colorScheme: colors,
    fontFamily: 'Inter',
    fontFamilyFallback: const ['Noto Sans CJK SC', 'sans-serif'],
    textTheme: TextTheme(
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
  );
}
