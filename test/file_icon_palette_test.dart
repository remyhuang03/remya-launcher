import 'dart:math' as math;

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_color_utilities/material_color_utilities.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/features/files/file_icon_palette.dart';

double _hueDistance(double a, double b) {
  final diff = (a - b).abs() % 360.0;
  return diff > 180.0 ? 360.0 - diff : diff;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final lightScheme = ColorScheme.fromSeed(
    seedColor: const Color(0xFF3F51B5),
    brightness: .light,
  );
  final darkScheme = ColorScheme.fromSeed(
    seedColor: const Color(0xFF3F51B5),
    brightness: .dark,
  );

  group('FileIconPalette', () {
    test('returns neutral container when icon seed is null or absent', () {
      final palette = FileIconPalette(
        seeds: FileIconSeeds(const {'editorconfig': null}),
      );

      expect(
        palette.containerColor(
          iconId: 'editorconfig',
          colorScheme: lightScheme,
        ),
        lightScheme.surfaceContainerHighest,
      );
      expect(
        palette.containerColor(
          iconId: 'editorconfig',
          colorScheme: lightScheme,
          selected: true,
        ),
        lightScheme.surfaceContainer,
      );
      expect(
        palette.containerColor(iconId: 'missing', colorScheme: darkScheme),
        darkScheme.surfaceContainerHighest,
      );
      expect(
        palette.containerColor(
          iconId: 'missing',
          colorScheme: darkScheme,
          selected: true,
        ),
        darkScheme.surfaceContainer,
      );
    });

    test('high contrast returns neutral even when icon has chromatic seed', () {
      final palette = FileIconPalette(
        seeds: FileIconSeeds(const {'rust': Color(0xFFFF7043)}),
      );

      expect(
        palette.containerColor(
          iconId: 'rust',
          colorScheme: lightScheme,
          highContrast: true,
        ),
        lightScheme.surfaceContainerHighest,
      );
      expect(
        palette.containerColor(
          iconId: 'rust',
          colorScheme: darkScheme,
          selected: true,
          highContrast: true,
        ),
        darkScheme.surfaceContainer,
      );
    });

    test('uses T90 in light mode, T30 in dark mode, and caps chroma at 24', () {
      const saturatedSeed = Color(0xFFFF0000);
      const sourceSeed = Color(0xFF2196F3);
      final palette = FileIconPalette(
        seeds: FileIconSeeds(const {'saturated': saturatedSeed}),
      );

      final harmonized = Blend.harmonize(
        saturatedSeed.toARGB32(),
        sourceSeed.toARGB32(),
      );
      final hct = Hct.fromInt(harmonized);
      expect(hct.chroma, greaterThan(FileIconPalette.maxPaletteChroma));

      final tonal = TonalPalette.of(
        hct.hue,
        math.min(hct.chroma, FileIconPalette.maxPaletteChroma),
      );
      final expectedLight = Color.lerp(
        lightScheme.surfaceContainerHighest,
        Color(tonal.get(FileIconPalette.lightTone)),
        FileIconPalette.normalTintWeight,
      )!;
      final expectedDark = Color.lerp(
        darkScheme.surfaceContainerHighest,
        Color(tonal.get(FileIconPalette.darkTone)),
        FileIconPalette.normalTintWeight,
      )!;

      final actualLight = palette.containerColor(
        iconId: 'saturated',
        colorScheme: lightScheme,
        appSourceSeed: sourceSeed,
      );
      final actualDark = palette.containerColor(
        iconId: 'saturated',
        colorScheme: darkScheme,
        appSourceSeed: sourceSeed,
      );

      final uncappedTonal = TonalPalette.of(hct.hue, hct.chroma);
      final uncappedDark = Color.lerp(
        darkScheme.surfaceContainerHighest,
        Color(uncappedTonal.get(FileIconPalette.darkTone)),
        FileIconPalette.normalTintWeight,
      )!;

      expect(actualLight, expectedLight);
      expect(actualDark, expectedDark);
      expect(actualDark, isNot(uncappedDark));
      expect(actualLight, isNot(actualDark));
      // 8-bit sRGB quantization can nudge chroma slightly above 24.0, well below
      // the uncapped seed chroma (>100).
      expect(Hct.fromInt(tonal.get(90)).chroma, lessThan(26.0));
      expect(Hct.fromInt(tonal.get(30)).chroma, lessThan(26.0));
    });

    test('harmonizes icon seed hue toward appSourceSeed by at most 15 deg', () {
      const iconSeed = Color(0xFFFF7043);
      const sourceSeed = Color(0xFF00BFA5);

      final origHct = Hct.fromInt(iconSeed.toARGB32());
      final sourceHct = Hct.fromInt(sourceSeed.toARGB32());
      final harmonized = Blend.harmonize(
        iconSeed.toARGB32(),
        sourceSeed.toARGB32(),
      );
      final harmHct = Hct.fromInt(harmonized);

      final initialGap = _hueDistance(origHct.hue, sourceHct.hue);
      final harmonizedGap = _hueDistance(harmHct.hue, sourceHct.hue);
      final shift = _hueDistance(origHct.hue, harmHct.hue);

      expect(harmonizedGap, lessThan(initialGap));
      expect(shift, greaterThan(0));
      expect(shift, lessThanOrEqualTo(15.5));

      final palette = FileIconPalette(
        seeds: FileIconSeeds(const {'rust': iconSeed}),
      );
      final withTealSource = palette.containerColor(
        iconId: 'rust',
        colorScheme: lightScheme,
        appSourceSeed: sourceSeed,
      );
      final withRedSource = palette.containerColor(
        iconId: 'rust',
        colorScheme: lightScheme,
        appSourceSeed: const Color(0xFFD50000),
      );
      expect(withTealSource, isNot(withRedSource));
    });

    test('selected state uses surfaceContainer with 0.08 tint', () {
      const iconSeed = Color(0xFF0288D1);
      const sourceSeed = Color(0xFF3F51B5);
      final palette = FileIconPalette(
        seeds: FileIconSeeds(const {'docker': iconSeed}),
      );

      final harmonized = Blend.harmonize(
        iconSeed.toARGB32(),
        sourceSeed.toARGB32(),
      );
      final hct = Hct.fromInt(harmonized);
      final tonal = TonalPalette.of(
        hct.hue,
        math.min(hct.chroma, FileIconPalette.maxPaletteChroma),
      );
      final expectedSelected = Color.lerp(
        lightScheme.surfaceContainer,
        Color(tonal.get(FileIconPalette.lightTone)),
        FileIconPalette.selectedTintWeight,
      )!;

      final unselected = palette.containerColor(
        iconId: 'docker',
        colorScheme: lightScheme,
        appSourceSeed: sourceSeed,
        selected: false,
      );
      final selected = palette.containerColor(
        iconId: 'docker',
        colorScheme: lightScheme,
        appSourceSeed: sourceSeed,
        selected: true,
      );

      expect(selected, expectedSelected);
      expect(selected, isNot(unselected));
    });

    test('caches results and separates entries across all key dimensions', () {
      final palette = FileIconPalette(
        seeds: FileIconSeeds(const {
          'rust': Color(0xFFFF7043),
          'docker': Color(0xFF0288D1),
        }),
      );

      final first = palette.containerColor(
        iconId: 'rust',
        colorScheme: lightScheme,
        appSourceSeed: const Color(0xFF3F51B5),
      );
      expect(palette.cacheSize, 1);

      // Identical parameters hit the cache without growing it.
      final repeat = palette.containerColor(
        iconId: 'rust',
        colorScheme: lightScheme,
        appSourceSeed: const Color(0xFF3F51B5),
      );
      expect(repeat, first);
      expect(palette.cacheSize, 1);

      // Varying iconId.
      palette.containerColor(
        iconId: 'docker',
        colorScheme: lightScheme,
        appSourceSeed: const Color(0xFF3F51B5),
      );
      expect(palette.cacheSize, 2);

      // Varying appSourceSeed.
      palette.containerColor(
        iconId: 'rust',
        colorScheme: lightScheme,
        appSourceSeed: const Color(0xFF009688),
      );
      expect(palette.cacheSize, 3);

      // Varying brightness.
      palette.containerColor(
        iconId: 'rust',
        colorScheme: darkScheme,
        appSourceSeed: const Color(0xFF3F51B5),
      );
      expect(palette.cacheSize, 4);

      // Varying selected.
      palette.containerColor(
        iconId: 'rust',
        colorScheme: lightScheme,
        appSourceSeed: const Color(0xFF3F51B5),
        selected: true,
      );
      expect(palette.cacheSize, 5);

      // Varying highContrast.
      palette.containerColor(
        iconId: 'rust',
        colorScheme: lightScheme,
        appSourceSeed: const Color(0xFF3F51B5),
        highContrast: true,
      );
      expect(palette.cacheSize, 6);
    });
  });

  group('FileIconSeeds', () {
    test('handles malformed JSON and invalid hex entries without throwing', () {
      final malformedDiagnostics = <String>[];
      final malformed = FileIconSeeds.fromJsonString(
        '{not valid json',
        onDiagnostic: malformedDiagnostics.add,
      );
      expect(malformed.seedFor('rust'), isNull);
      expect(malformedDiagnostics, isNotEmpty);

      final nonMapDiagnostics = <String>[];
      final nonMap = FileIconSeeds.fromJsonString(
        '["rust"]',
        onDiagnostic: nonMapDiagnostics.add,
      );
      expect(nonMap.seedFor('rust'), isNull);
      expect(nonMapDiagnostics, isNotEmpty);

      final badEntryDiagnostics = <String>[];
      final partial = FileIconSeeds.fromJson(const {
        'seeds': {
          'rust': '#FF7043',
          'badShort': '#123',
          'badPrefix': 'FF7043',
          'badChars': '#GGGGGG',
          'badAlpha': '#FF704380',
          'badType': 123,
          'neutral': null,
        },
      }, onDiagnostic: badEntryDiagnostics.add);
      expect(partial.seedFor('rust'), const Color(0xFFFF7043));
      expect(partial.seedFor('badShort'), isNull);
      expect(partial.seedFor('badPrefix'), isNull);
      expect(partial.seedFor('badChars'), isNull);
      expect(partial.seedFor('badAlpha'), isNull);
      expect(partial.seedFor('badType'), isNull);
      expect(partial.seedFor('neutral'), isNull);
      expect(badEntryDiagnostics, hasLength(5));
    });

    test('loadAsset returns empty on missing asset with diagnostic', () async {
      final diagnostics = <String>[];
      final seeds = await FileIconSeeds.loadAsset(
        path: 'assets/does_not_exist.json',
        onDiagnostic: diagnostics.add,
      );
      expect(seeds.seedFor('rust'), isNull);
      expect(diagnostics, isNotEmpty);
    });

    test('loads real assets/material_icon_theme/seeds.json', () async {
      final diagnostics = <String>[];
      final seeds = await FileIconSeeds.loadAsset(
        bundle: rootBundle,
        onDiagnostic: diagnostics.add,
      );

      expect(diagnostics, isEmpty);
      expect(seeds.seedFor('rust'), const Color(0xFFFF7043));
      expect(seeds.seedFor('docker'), const Color(0xFF0288D1));
      expect(seeds.seedFor('python'), const Color(0xFF0288D1));
      expect(seeds.seedFor('editorconfig'), isNull);
    });
  });
}
