import 'dart:convert';
import 'dart:io';

import 'package:emoji_picker_i18n/emoji_picker_i18n.dart';
import 'package:emoji_picker_i18n/locales/en.dart';
import 'package:emoji_picker_i18n/locales/ja.dart';
import 'package:emoji_picker_i18n/locales/zh.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:remya_launcher/features/emoji/emoji_search.dart';

const emoji18Names = {
  '🫫': 'cracking face',
  '🫹': 'leftwards thumb sign',
  '🫺': 'rightwards thumb sign',
  '🫌': 'monarch butterfly',
  '🫝': 'pickle',
  '🛙': 'lighthouse',
  '🪋': 'meteor',
  '🪌': 'eraser',
  '🪍': 'net with handle',
};

const queries = [
  ('yellow heart', 'en', '💛'),
  ('yellow hea', 'en', '💛'),
  (':yellow_heart:', 'en', '💛'),
  ('ｙｅｌｌｏｗ　ｈｅａｒｔ', 'en', '💛'),
  ('黄心', 'zh-Hans', '💛'),
  ('黃色愛心', 'zh-Hant', '💛'),
  ('huangse aixin', 'en', '💛'),
  ('猫咪', 'zh-Hans', '🐱'),
  ('貓', 'en', '🐱'),
  ('cat', 'ja', '🐱'),
  ('neko', 'ja', '🐱'),
  ('ねこ', 'ja', '🐱'),
  ('ネコ', 'en', '🐱'),
  ('gato', 'es', '🐈'),
  ('Katze', 'de', '🐈'),
  ('кошка', 'ru', '🐱'),
  ('泪目', 'zh-Hans', '🥹'),
  ('笑死', 'zh-Hans', '🤣'),
  ('rofl', 'en', '🤣'),
  (':sob:', 'en', '😭'),
  (':joy:', 'en', '😂'),
  ('joy:', 'en', '😂'),
  ('erazer', 'en', '🪌'),
  ('thumb light skin', 'en', '👍🏻'),
  ('+1', 'en', '👍'),
  ('-1', 'en', '👎'),
  ('1faeb', 'en', '🫫'),
  ('U+1F469 U+200D U+1F4BB', 'en', '👩‍💻'),
  ('woman technologist', 'en', '👩‍💻'),
  ('man health worker: medium skin tone', 'en', '👨🏽‍⚕️'),
  ('cafe', 'es', '☕'),
  ('heissgetrank', 'de', '☕'),
];

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test(
    'released library provides search but its embedded data predates Emoji 18',
    () {
      final upstream = EmojiSearch(
        common: kEmojiCommon,
        locales: [kEmojiLocaleEn, kEmojiLocaleZh, kEmojiLocaleJa],
      );
      for (final query in ['cat', '猫', 'ねこ']) {
        expect(upstream.search(query), isNotEmpty, reason: query);
      }
      expect(upstream.findByChar('👩‍💻')?.char, '👩‍💻');
      for (final emoji in emoji18Names.keys) {
        expect(upstream.findByChar(emoji), isNull, reason: emoji);
      }
      for (final thumb in ['🫹', '🫺']) {
        for (var tone = 0x1F3FB; tone <= 0x1F3FF; tone++) {
          expect(
            upstream.findByChar(thumb + String.fromCharCode(tone)),
            isNull,
          );
        }
      }
    },
  );

  group('bundled search', () {
    late EmojiCatalog catalog;

    setUpAll(() async {
      final elapsed = Stopwatch()..start();
      catalog = await EmojiCatalog.load();
      debugPrint('Emoji catalog loaded in ${elapsed.elapsedMilliseconds} ms');
    });

    for (final (query, locale, emoji) in queries) {
      test('finds $emoji for "$query" ($locale)', () {
        final results = catalog.search(query, locale: locale, limit: 5);
        expect(results.map((result) => result.emoji), contains(emoji));
        expect(results.length, lessThanOrEqualTo(5));
      });
    }

    test('finds Emoji 18 names and all ten official thumb variants', () {
      expect(catalog.unicodeVersion, '18.0');
      for (final MapEntry(key: emoji, value: name) in emoji18Names.entries) {
        final result = catalog.search(name).first;
        expect(result.emoji, emoji);
        expect(result.name, name);
      }
      for (final thumb in ['🫹', '🫺']) {
        expect(
          catalog
              .variants(catalog.search(thumb).single)
              .map((item) => item.emoji),
          [
            thumb,
            for (var tone = 0x1F3FB; tone <= 0x1F3FF; tone++)
              thumb + String.fromCharCode(tone),
          ],
        );
      }
    });

    test('preserves official payload and rejects invented qualification', () {
      const england =
          '🏴\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}';
      for (final emoji in [
        '👩‍💻',
        '👨🏽‍⚕️',
        '🏳️‍🌈',
        '❤️‍🔥',
        '🇯🇵',
        '1️⃣',
        '#️⃣',
        england,
        '👨‍👩‍👧‍👦',
      ]) {
        expect(catalog.search(emoji).single.emoji, emoji);
      }
      expect(catalog.search('☺').single.emoji, '☺️');
      expect(catalog.search('☺︎'), isEmpty);
      expect(catalog.search('👩️‍💻'), isEmpty);
      expect(catalog.search('👩‍‍💻'), isEmpty);
      expect(catalog.search('👍').single.name, 'thumbs up');
    });

    test(
      'every bundled RGI record survives parsing and exact lookup',
      () async {
        final raw = await rootBundle.loadString('assets/emoji/catalog.json');
        final data = jsonDecode(raw) as Map<String, dynamic>;
        final entries = (data['common'] as List).cast<String>();
        expect(entries, hasLength(3963));
        for (final row in entries) {
          final emoji = row.split('|').first;
          expect(catalog.search(emoji).single.emoji, emoji);
        }
        expect(
          catalog.browse().map((item) => item.group).toSet(),
          (data['groups'] as List).cast<String>().toSet(),
        );
      },
    );

    test(
      'variant families preserve gender branches and mixed-tone handshakes',
      () {
        expect(
          catalog
              .variants(catalog.search('🧑‍⚕️').single)
              .map((item) => item.emoji),
          contains('👩‍⚕️'),
        );
        expect(
          catalog
              .variants(catalog.search('👩‍⚕️').single)
              .map((item) => item.emoji),
          contains('👩🏽‍⚕️'),
        );
        expect(
          catalog
              .variants(catalog.search('🤝').single)
              .map((item) => item.emoji),
          contains('🫱🏻‍🫲🏼'),
        );
      },
    );

    test('uses UI locale for display and leaves native match terms intact', () {
      final hans = catalog.search('yellow heart', locale: 'zh-CN').first;
      final hant = catalog.search('yellow heart', locale: 'zh-TW').first;
      expect(hans.name, '黄心');
      expect(hant.name, '黃心');
      expect(hans.matchedTerm, 'yellow heart');
      expect(
        catalog.search('yellow heart', locale: 'fr-FR').first.name,
        'yellow heart',
      );
    });

    test('limits long input and leaves unrelated terms unmatched', () {
      expect(catalog.search('x' * 1000), isEmpty);
      expect(catalog.search('a\u0301' * 200), isEmpty);
      expect(catalog.search('randomnonexistentemojiwordzz'), isEmpty);
      expect(catalog.search('U+FFFFFF'), isEmpty);
      expect(
        catalog
            .search('thumb', limit: 20)
            .any(
              (result) => result.emoji.runes.any(
                (rune) => rune >= 0x1F3FB && rune <= 0x1F3FF,
              ),
            ),
        isFalse,
      );
    });

    test('evaluates the curated search corpus at rank 10', () {
      final fixture = File('test/fixtures/emoji-search-golden.jsonl');
      final statistics = <String, (int cases, int hits, double rrs)>{};
      final failures = <String>[];
      for (final line in fixture.readAsLinesSync().where(
        (line) => line.trim().isNotEmpty,
      )) {
        final row = jsonDecode(line) as Map<String, dynamic>;
        final locale = row['ui_locale'] as String;
        final query = row['query'] as String;
        final expected = switch (row['expected']) {
          final String emoji => {emoji},
          final List list => list.cast<String>().toSet(),
          _ => throw FormatException('Missing expected emoji for $query'),
        };
        final actual = catalog.search(query, locale: locale, limit: 10);
        final rank = actual.indexWhere((item) => expected.contains(item.emoji));
        if (rank == -1) {
          failures.add(
            '$locale "$query": expected $expected; got ${actual.map((item) => item.emoji).toList()}',
          );
        }
        final (count, hits, rrs) = statistics[locale] ?? (0, 0, 0.0);
        statistics[locale] = (
          count + 1,
          hits + (rank == -1 ? 0 : 1),
          rrs + (rank == -1 ? 0 : 1 / (rank + 1)),
        );
      }
      var total = 0;
      var totalHits = 0;
      var totalRrs = 0.0;
      for (final MapEntry(key: locale, value: (count, hits, rrs))
          in statistics.entries) {
        debugPrint(
          '$locale: Recall@10 $hits/$count, MRR@10 ${(rrs / count).toStringAsFixed(3)}',
        );
        total += count;
        totalHits += hits;
        totalRrs += rrs;
      }
      expect(total, greaterThanOrEqualTo(150));
      debugPrint(
        'Overall: Recall@10 $totalHits/$total, MRR@10 ${(totalRrs / total).toStringAsFixed(3)}',
      );
      expect(failures, isEmpty, reason: failures.join('\n'));
    });

    test('measures warmed searches in the Flutter test VM', () {
      for (final (query, locale, _) in queries) {
        catalog.search(query, locale: locale, limit: 20);
      }
      final elapsed = <int>[];
      for (var pass = 0; pass < 5; pass++) {
        for (final (query, locale, _) in queries) {
          final clock = Stopwatch()..start();
          catalog.search(query, locale: locale, limit: 20);
          elapsed.add(clock.elapsedMicroseconds);
        }
      }
      elapsed.sort();
      int percentile(double p) => elapsed[(elapsed.length * p).ceil() - 1];
      debugPrint(
        'Emoji search ${elapsed.length} warm samples: '
        'P50 ${percentile(.50)} us, P95 ${percentile(.95)} us, '
        'P99 ${percentile(.99)} us',
      );
    });
  });
}
