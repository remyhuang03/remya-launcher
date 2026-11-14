import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:material_color_utilities/material_color_utilities.dart';
import 'package:material_ui/material_ui.dart';

/// Theme extension carrying the active Remya Core source seed color.
///
/// When attached to [ThemeData.extensions], [FileIconPalette] harmonizes file
/// icon container tints toward [seedColor] instead of approximating it from
/// [ColorScheme.primary].
@immutable
class RemyaSourceSeed extends ThemeExtension<RemyaSourceSeed> {
  const RemyaSourceSeed(this.seedColor);

  final Color seedColor;

  @override
  RemyaSourceSeed copyWith({Color? seedColor}) =>
      RemyaSourceSeed(seedColor ?? this.seedColor);

  @override
  RemyaSourceSeed lerp(
    covariant ThemeExtension<RemyaSourceSeed>? other,
    double t,
  ) {
    if (other is! RemyaSourceSeed) return this;
    return RemyaSourceSeed(
      Color.lerp(seedColor, other.seedColor, t) ?? seedColor,
    );
  }

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is RemyaSourceSeed && seedColor == other.seedColor;

  @override
  int get hashCode => seedColor.hashCode;
}

/// Parsed icon seed colors loaded from `material-icon-theme` seed metadata.
class FileIconSeeds {
  FileIconSeeds(Map<String, Color?> seeds) : _seeds = Map.unmodifiable(seeds);

  const FileIconSeeds.empty() : _seeds = const {};

  static const defaultAssetPath = 'assets/material_icon_theme/seeds.json';
  static final _hexPattern = RegExp(r'^#[0-9a-fA-F]{6}$');

  final Map<String, Color?> _seeds;

  /// Parses decoded JSON metadata into [FileIconSeeds].
  ///
  /// Null entries remain `null`. Invalid hex strings report a diagnostic and
  /// are treated as `null` without throwing.
  factory FileIconSeeds.fromJson(
    Map<String, dynamic> json, {
    void Function(String message)? onDiagnostic,
  }) {
    final rawSeeds = json['seeds'];
    if (rawSeeds is! Map<Object?, Object?>) {
      _reportDiagnostic('Expected a JSON object for "seeds".', onDiagnostic);
      return const FileIconSeeds.empty();
    }

    final parsed = <String, Color?>{};
    for (final entry in rawSeeds.entries) {
      final key = entry.key;
      if (key is! String) {
        _reportDiagnostic('Ignoring non-string icon key "$key".', onDiagnostic);
        continue;
      }
      final value = entry.value;
      if (value == null) {
        parsed[key] = null;
        continue;
      }
      if (value is! String || !_hexPattern.hasMatch(value)) {
        _reportDiagnostic(
          'Invalid hex seed "$value" for icon "$key"; treating as null.',
          onDiagnostic,
        );
        parsed[key] = null;
        continue;
      }
      final rgb = int.parse(value.substring(1), radix: 16);
      parsed[key] = Color(0xFF000000 | rgb);
    }
    return FileIconSeeds(parsed);
  }

  /// Decodes a raw `seeds.json` string into [FileIconSeeds].
  ///
  /// Never throws on malformed JSON; reports a diagnostic and returns an empty
  /// seed map instead.
  factory FileIconSeeds.fromJsonString(
    String source, {
    void Function(String message)? onDiagnostic,
  }) {
    try {
      final decoded = jsonDecode(source);
      if (decoded is! Map<String, dynamic>) {
        _reportDiagnostic(
          'Expected top-level JSON object in icon seeds.',
          onDiagnostic,
        );
        return const FileIconSeeds.empty();
      }
      return FileIconSeeds.fromJson(decoded, onDiagnostic: onDiagnostic);
    } on Object catch (error) {
      _reportDiagnostic(
        'Failed to parse icon seeds JSON: $error',
        onDiagnostic,
      );
      return const FileIconSeeds.empty();
    }
  }

  /// Loads icon seeds from a Flutter asset bundle.
  ///
  /// Never throws on missing or malformed assets; reports a diagnostic and
  /// returns an empty seed map instead.
  static Future<FileIconSeeds> loadAsset({
    AssetBundle? bundle,
    String path = defaultAssetPath,
    void Function(String message)? onDiagnostic,
  }) async {
    try {
      final data = await (bundle ?? rootBundle).load(path);
      final source = utf8.decode(
        data.buffer.asUint8List(data.offsetInBytes, data.lengthInBytes),
      );
      return FileIconSeeds.fromJsonString(source, onDiagnostic: onDiagnostic);
    } on Object catch (error) {
      _reportDiagnostic(
        'Failed to load icon seeds asset "$path": $error',
        onDiagnostic,
      );
      return const FileIconSeeds.empty();
    }
  }

  /// Returns the seed color for [iconId], or `null` when neutral/absent.
  Color? seedFor(String iconId) => _seeds[iconId];

  static void _reportDiagnostic(
    String message,
    void Function(String message)? onDiagnostic,
  ) {
    if (onDiagnostic != null) {
      onDiagnostic(message);
    } else if (kDebugMode) {
      debugPrint('FileIconSeeds: $message');
    }
  }
}

typedef _PaletteCacheKey = (
  String iconId,
  int sourceSeedArgb,
  Brightness brightness,
  bool selected,
  bool highContrast,
  int neutralArgb,
);

/// Computes harmonized, tone-clamped Material 3 container colors for file icons.
class FileIconPalette {
  FileIconPalette({FileIconSeeds? seeds})
    : seeds = seeds ?? const FileIconSeeds.empty();

  final FileIconSeeds seeds;
  final _cache = <_PaletteCacheKey, Color>{};

  static const double maxPaletteChroma = 24.0;
  static const double normalTintWeight = 0.25;
  static const double selectedTintWeight = 0.08;
  static const int lightTone = 90;
  static const int darkTone = 30;

  @visibleForTesting
  int get cacheSize => _cache.length;

  /// Resolves the active app source seed from [context].
  ///
  /// Uses [RemyaSourceSeed.seedColor] when attached by Remya Core; otherwise
  /// falls back to [ColorScheme.primary] as an approximation.
  static Color appSourceSeedOf(BuildContext context) =>
      Theme.of(context).extension<RemyaSourceSeed>()?.seedColor ??
      Theme.of(context).colorScheme.primary;

  /// Computes the file icon container color for [iconId] under [colorScheme].
  Color containerColor({
    required String iconId,
    required ColorScheme colorScheme,
    Color? appSourceSeed,
    bool selected = false,
    bool highContrast = false,
  }) {
    final neutral = selected
        ? colorScheme.surfaceContainer
        : colorScheme.surfaceContainerHighest;
    final effectiveSourceSeed = appSourceSeed ?? colorScheme.primary;
    final key = (
      iconId,
      effectiveSourceSeed.toARGB32(),
      colorScheme.brightness,
      selected,
      highContrast,
      neutral.toARGB32(),
    );
    final cached = _cache[key];
    if (cached != null) return cached;

    final Color result;
    if (highContrast) {
      result = neutral;
    } else {
      final iconSeed = seeds.seedFor(iconId);
      if (iconSeed == null) {
        result = neutral;
      } else {
        final harmonizedArgb = Blend.harmonize(
          iconSeed.toARGB32(),
          effectiveSourceSeed.toARGB32(),
        );
        final hct = Hct.fromInt(harmonizedArgb);
        final palette = TonalPalette.of(
          hct.hue,
          math.min(hct.chroma, maxPaletteChroma),
        );
        final iconTone = Color(
          palette.get(colorScheme.brightness == .dark ? darkTone : lightTone),
        );
        result = Color.lerp(
          neutral,
          iconTone,
          selected ? selectedTintWeight : normalTintWeight,
        )!;
      }
    }

    _cache[key] = result;
    return result;
  }

  /// Computes the container color using theme, source seed, and high-contrast
  /// settings from [context].
  Color containerColorForContext(
    BuildContext context, {
    required String iconId,
    bool selected = false,
  }) => containerColor(
    iconId: iconId,
    colorScheme: Theme.of(context).colorScheme,
    appSourceSeed: appSourceSeedOf(context),
    selected: selected,
    highContrast: MediaQuery.maybeHighContrastOf(context) ?? false,
  );
}
