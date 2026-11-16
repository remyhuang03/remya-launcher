import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import 'apps_client.dart';

/// A themed desktop icon, or a monogram seeded by [id] when it cannot load.
class AppsIcon extends StatelessWidget {
  const AppsIcon({
    required this.id,
    required this.name,
    this.icon,
    this.size = 32,
    super.key,
  });

  AppsIcon.app(LauncherAppEntry app, {double size = 32, Key? key})
    : this(id: app.id, name: app.name, icon: app.icon, size: size, key: key);

  final String id, name;
  final String? icon;
  final double size;

  static const _window = MethodChannel('top.remya.launcher/window');
  static final _icons = <(String, int), Future<Uint8List?>>{};

  @override
  Widget build(BuildContext context) {
    final fallback = _AppMonogram(id, name, size: size);
    final icon = this.icon;
    if (icon == null || icon.isEmpty) return fallback;

    final pixels = (size * MediaQuery.devicePixelRatioOf(context)).ceil().clamp(
      1,
      256,
    );
    return FutureBuilder<Uint8List?>(
      future: _cachedIcon(icon, pixels),
      builder: (context, snapshot) {
        final bytes = snapshot.connectionState == .done ? snapshot.data : null;
        return bytes == null
            ? fallback
            : Image.memory(
                bytes,
                width: size,
                height: size,
                fit: .contain,
                excludeFromSemantics: true,
                errorBuilder: (context, error, stackTrace) => fallback,
              );
      },
    );
  }

  static Future<Uint8List?> _cachedIcon(String name, int size) {
    final key = (name, size);
    if (_icons[key] case final cached?) return cached;
    if (_icons.length >= 128) _icons.remove(_icons.keys.first);
    return _icons[key] = _loadIcon(name, size);
  }

  static Future<Uint8List?> _loadIcon(String name, int size) async {
    try {
      return await _window.invokeMethod<Uint8List>('resolveAppIcon', {
        'name': name,
        'size': size,
      });
    } on PlatformException {
      return null;
    } on MissingPluginException {
      return null;
    }
  }
}

class _AppMonogram extends StatelessWidget {
  const _AppMonogram(this.id, this.name, {required this.size});

  final String id, name;
  final double size;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final hash = id.codeUnits.fold(
      0x811c9dc5,
      (hash, code) => ((hash ^ code) * 0x01000193) & 0xffffffff,
    );
    final (background, foreground) = switch (hash % 3) {
      0 => (colors.primaryContainer, colors.onPrimaryContainer),
      1 => (colors.secondaryContainer, colors.onSecondaryContainer),
      _ => (colors.tertiaryContainer, colors.onTertiaryContainer),
    };
    final letter = name.trim().characters.firstOrNull ?? '?';
    return ExcludeSemantics(
      child: Container(
        width: size,
        height: size,
        alignment: .center,
        decoration: BoxDecoration(
          color: background,
          borderRadius: .circular(size * 10 / 32),
        ),
        child: Text(
          letter.toUpperCase(),
          style: theme.textTheme.labelLarge?.copyWith(
            color: foreground,
            fontSize: size * 15 / 32,
            fontWeight: .w700,
          ),
        ),
      ),
    );
  }
}
