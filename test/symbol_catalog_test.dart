import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:remya_launcher/features/emoji/emoji_search.dart';
import 'package:remya_launcher/features/emoji/symbol_catalog.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('finds symbols by name, alias, character and code point', () {
    const cases = [
      ('copyright', '©'),
      ('trademark', '™'),
      ('right   arrow', '→'),
      ('infinity', '∞'),
      ('＋／－', '±'),
      ('n-ary summation', '∑'),
      ('省略號', '…'),
      ('u+a9', '©'),
      ('0x2122', '™'),
      ('2192', '→'),
      ('…', '…'),
    ];
    for (final (query, expected) in cases) {
      final result = SymbolCatalog.search(query).first;
      expect(result.emoji, expected, reason: query);
      expect(result.kind, EmojiResultKind.symbol);
    }
    expect(
      SymbolCatalog.search('right arrow').first.matchedTerm,
      'right arrow',
    );
    expect(SymbolCatalog.search('U+2211').single.codePoints, 'U+2211');
  });

  test('supports group browsing, limits and invalid input', () {
    expect(
      SymbolCatalog.search('   ', limit: 3),
      SymbolCatalog.browse().take(3),
    );
    expect(SymbolCatalog.search('Typography', limit: 2), hasLength(2));
    expect(SymbolCatalog.search('x' * 257), isEmpty);
    expect(SymbolCatalog.search('', limit: 0), isEmpty);
    expect(SymbolCatalog.search('U+D800'), isEmpty);
    expect(SymbolCatalog.search('U+110000'), isEmpty);
    expect(SymbolCatalog.search('👩‍💻'), isEmpty);
    expect(SymbolCatalog.search('©️'), isEmpty);
  });

  test(
    'keeps every symbol distinct and separate from fully qualified Emoji',
    () async {
      final entries = SymbolCatalog.browse();
      final characters = {for (final entry in entries) entry.emoji};
      final catalog = jsonDecode(
        await rootBundle.loadString('assets/emoji/catalog.json'),
      ) as Map<String, dynamic>;
      final emoji = {
        for (final row in (catalog['common'] as List).cast<String>())
          row.split('|').first,
      };
      expect(characters, hasLength(entries.length));
      expect(characters.intersection(emoji), isEmpty);
      for (final symbol in entries) {
        expect(SymbolCatalog.search(symbol.emoji).single, same(symbol));
        expect(SymbolCatalog.search(symbol.codePoints).single, same(symbol));
      }
    },
  );
}
