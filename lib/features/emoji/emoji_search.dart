import 'dart:convert';

import 'package:emoji_picker_i18n/emoji_picker_i18n.dart'
    show Emoji, EmojiSearch;
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:unorm_dart/unorm_dart.dart' as unorm;

class EmojiResult {
  const EmojiResult({
    required this.emoji,
    required this.name,
    required this.group,
    required this.codePoints,
    this.matchedTerm,
  });

  final String emoji;
  final String name;
  final String group;
  final String codePoints;
  final String? matchedTerm;
}

class EmojiCatalog {
  EmojiCatalog._({
    required this.unicodeVersion,
    required this._engine,
    required this._rows,
    required this._locales,
    required this._children,
    required this._parents,
    required this._qualifiers,
    required this._groups,
    required this._englishNeighbors,
    required this._shortcodes,
  });

  final String unicodeVersion;
  final EmojiSearch _engine;
  final Map<String, int> _rows;
  final Map<String, List<String>> _locales;
  final Map<String, List<String>> _children;
  final Map<String, String> _parents;
  final Map<String, String> _qualifiers;
  final List<String> _groups;
  final Map<String, List<String>> _englishNeighbors;
  final Map<String, List<String>> _shortcodes;

  static Future<EmojiCatalog> load({AssetBundle? bundle}) async {
    final data = await (bundle ?? rootBundle).load('assets/emoji/catalog.json');
    return compute(_parse, Uint8List.sublistView(data));
  }

  static EmojiCatalog _parse(Uint8List data) {
    final json = jsonDecode(utf8.decode(data)) as Map<String, dynamic>;
    final common = (json['common'] as List).cast<String>();
    final locales = _stringLists(json['locales']);
    if (common.isEmpty ||
        !locales.containsKey('en') ||
        locales.values.any((rows) => rows.length != common.length)) {
      throw const FormatException('Invalid emoji catalog');
    }

    final children = _stringLists(json['variants']);
    final parents = {
      for (final MapEntry(key: parent, value: children) in children.entries)
        for (final child in children) child: parent,
    };
    final characters = [for (final row in common) row.split('|').first];
    final rows = {for (var i = 0; i < characters.length; i++) characters[i]: i};
    return EmojiCatalog._(
      unicodeVersion: json['version'] as String,
      engine: EmojiSearch(
        common: common,
        locales: _searchableLocales(locales, characters, rows, parents),
      ),
      rows: rows,
      locales: locales,
      children: children,
      parents: parents,
      qualifiers: (json['qualifiers'] as Map).cast<String, String>(),
      groups: (json['groups'] as List).cast<String>(),
      englishNeighbors: _neighborIndex(locales['en']!),
      shortcodes: _stringLists(json['shortcodes']),
    );
  }

  List<EmojiResult> search(
    String query, {
    String locale = 'en',
    int limit = 60,
  }) {
    if (limit <= 0 || query.length > 256 || query.runes.length > 128) {
      return const [];
    }
    final language = _language(locale);
    final raw = query.trim();
    if (raw.isEmpty) {
      return _browse(language).take(limit).toList();
    }

    final direct = _directMatch(raw);
    if (direct != null) return [_result(direct, language)];

    final queryNfkc = unorm.nfkc(raw).toLowerCase();
    final shortcode = _shortcut.firstMatch(queryNfkc)?.group(1) ?? queryNfkc;
    final shortcutHits = _shortcodes[shortcode] ?? const <String>[];
    if (queryNfkc.endsWith(':') && shortcutHits.isNotEmpty) {
      return [
        for (final emoji in shortcutHits.take(limit))
          _result(_engine.emojis[_rows[emoji]!], language, shortcode),
      ];
    }

    var term = _normalize(shortcode);
    var matches = _engine.search(term, limit: _rows.length);
    if (matches.isEmpty) {
      final folded = _foldLatin(term);
      if (term != folded) {
        term = folded;
        matches = _engine.search(term, limit: _rows.length);
      }
    }

    if (matches.isEmpty && _fuzzyWord.hasMatch(term) && !raw.contains(':')) {
      final alternatives = <String>{
        ...?_englishNeighbors['^${term.substring(0, 2)}'],
        ...?_englishNeighbors['${term.substring(term.length - 2)}\$'],
      };
      for (final candidate in alternatives) {
        if (!_oneEditApart(term, candidate)) continue;
        term = candidate;
        matches = _engine.search(term, limit: _rows.length);
        break;
      }
    }

    final comparison = _comparisonKey(term);
    final seen = <String>{...shortcutHits.take(limit)};
    final results = [
      for (final emoji in seen)
        _result(_engine.emojis[_rows[emoji]!], language, shortcode),
    ];
    if (results.length == limit) return results;
    for (var entry in matches) {
      var match = _match(entry, language, comparison);
      final parent = _parents[entry.char];
      if (parent != null) {
        final base = _engine.emojis[_rows[parent]!];
        final baseMatch = _match(base, language, comparison);
        if (baseMatch != null &&
            (match == null || baseMatch.rank <= match.rank)) {
          entry = base;
          match = baseMatch;
        }
      }
      if (!seen.add(entry.char)) continue;
      results.add(_result(entry, language, match?.term));
      if (results.length == limit) break;
    }
    return results;
  }

  List<EmojiResult> browse({String locale = 'en'}) =>
      _browse(_language(locale)).toList();

  Iterable<EmojiResult> _browse(String language) => _engine.emojis
      .where((entry) => !_parents.containsKey(entry.char))
      .map((entry) => _result(entry, language));

  bool hasVariants(EmojiResult item) =>
      _children.containsKey(item.emoji) || _parents.containsKey(item.emoji);

  List<EmojiResult> variants(EmojiResult item, {String locale = 'en'}) {
    final base = _children.containsKey(item.emoji)
        ? item.emoji
        : _parents[item.emoji] ?? item.emoji;
    final children = _children[base];
    if (children == null) return const [];
    final language = _language(locale);
    return [
      for (final emoji in [base, ...children])
        _result(_engine.emojis[_rows[emoji]!], language),
    ];
  }

  Emoji? _directMatch(String raw) {
    var index = _rows[_qualifiers[raw] ?? raw];
    if (index == null && _codePointQuery.hasMatch(raw)) {
      final points = [
        for (final token in raw.toLowerCase().split(_codePointSeparator))
          int.parse(token.replaceFirst('u+', ''), radix: 16),
      ];
      if (points.every((point) => point <= 0x10FFFF)) {
        final sequence = String.fromCharCodes(points);
        index = _rows[_qualifiers[sequence] ?? sequence];
      }
    }
    return index == null ? null : _engine.emojis[index];
  }

  String _language(String locale) {
    final parts = locale.toLowerCase().split(RegExp('[-_]'));
    if (parts.first == 'zh') {
      return parts.any({'hant', 'tw', 'hk', 'mo'}.contains)
          ? 'zh-Hant'
          : 'zh-Hans';
    }
    return _locales.containsKey(parts.first) ? parts.first : 'en';
  }

  EmojiResult _result(Emoji entry, String language, [String? term]) {
    final row = _rows[entry.char]!;
    final value = _locales[language]?[row] ?? _locales['en']![row];
    final label = value.split('|').first;
    return EmojiResult(
      emoji: entry.char,
      name: label.isEmpty ? _locales['en']![row].split('|').first : label,
      group: _groups[entry.group],
      codePoints: entry.char.runes
          .map(
            (point) =>
                'U+${point.toRadixString(16).toUpperCase().padLeft(4, '0')}',
          )
          .join(' '),
      matchedTerm: term,
    );
  }

  ({String term, int rank})? _match(
    Emoji entry,
    String language,
    String query,
  ) {
    if (query.isEmpty) return null;
    final row = _rows[entry.char]!;
    ({String term, int rank})? best;
    for (final locale in {language, ..._locales.keys}) {
      final terms = _locales[locale]![row].split('|');
      for (var index = 0; index < terms.length; index++) {
        final term = terms[index];
        final key = _comparisonKey(term);
        var rank = _rank(key, query);
        if (rank == null && (locale == 'de' || locale == 'es')) {
          final folded = _rank(_foldLatin(key), query);
          if (folded != null) rank = folded + 1;
        }
        if (rank == null) continue;
        rank += index == 0 ? 0 : 1;
        if (rank == 0) return (term: term, rank: rank);
        if (best == null || rank < best.rank) best = (term: term, rank: rank);
      }
    }
    return best;
  }
}

Map<String, List<String>> _stringLists(Object? value) => {
  for (final MapEntry(:key, :value) in (value as Map<String, dynamic>).entries)
    key: (value as List).cast<String>(),
};

final _codePointQuery = RegExp(
  r'^(?:U\+)?[0-9A-F]{4,6}(?:[\s-]+(?:U\+)?[0-9A-F]{4,6})*$',
  caseSensitive: false,
);
final _codePointSeparator = RegExp(r'[\s-]+');
final _wordSeparator = RegExp(r'[\s_‐‑‒–—]+');
final _shortcut = RegExp(r'^:?([a-z0-9_+\-]+):$');
final _spacesOrKana = RegExp(r'[\s\u3041-\u3096\u309D\u309E]');
final _diacritics = RegExp(r'[\u0300-\u036F]');
final _nonAscii = RegExp(r'[^\x00-\x7F]');

String _normalize(String query) => query.replaceAll(_wordSeparator, ' ').trim();

String _comparisonKey(String text) =>
    text.toLowerCase().replaceAllMapped(_spacesOrKana, (match) {
      final unit = match[0]!.codeUnitAt(0);
      return unit >= 0x3041 && unit <= 0x309E
          ? String.fromCharCode(unit + 0x60)
          : '';
    });

String _foldLatin(String text) => _nonAscii.hasMatch(text)
    ? unorm.nfd(text).replaceAll(_diacritics, '').replaceAll('ß', 'ss')
    : text;

List<List<String>> _searchableLocales(
  Map<String, List<String>> locales,
  List<String> characters,
  Map<String, int> rows,
  Map<String, String> parents,
) {
  final source = locales.values.toList();
  final searchable = List.generate(source.length, (_) => <String>[]);
  for (var row = 0; row < characters.length; row++) {
    final parent = rows[parents[characters[row]]];
    final inheritedTags = {
      if (parent != null)
        for (final locale in source) ...locale[parent].toLowerCase().split('|'),
    };
    final terms = [for (final locale in source) locale[row].split('|')];
    final labels = {for (final locale in terms) locale.first.toLowerCase()};
    final seen = {...labels, ...inheritedTags};
    for (var locale = 0; locale < terms.length; locale++) {
      final entry = terms[locale];
      final label = entry.first;
      final tags = <String>[
        for (final tag in entry.skip(1))
          if (seen.add(tag.toLowerCase())) tag,
      ];
      searchable[locale].add([label, ...tags].join('|'));
    }

    for (final language in ['de', 'es']) {
      final entry = locales[language]?[row];
      if (entry == null) continue;
      final folded = <String>[
        for (final term in entry.toLowerCase().split('|').map(_foldLatin))
          if (seen.add(term)) term,
      ];
      if (folded.isNotEmpty) {
        searchable.first[row] += '|${folded.join('|')}';
      }
    }
  }
  return searchable;
}

int? _rank(String keyword, String query) {
  if (keyword == query) return 0;
  if (keyword.startsWith(query)) return 2;
  if (keyword.contains(query)) return 4;
  return null;
}

final _fuzzyWord = RegExp(r'^[a-z]{4,24}$');

Map<String, List<String>> _neighborIndex(List<String> english) {
  final labels = <String>{};
  final tags = <String>{};
  for (final row in english) {
    final terms = row.toLowerCase().split('|');
    if (_fuzzyWord.hasMatch(terms.first)) labels.add(terms.first);
    tags.addAll(terms.skip(1).where(_fuzzyWord.hasMatch));
  }
  final bins = <String, List<String>>{};
  // shortcut: one-edit matches share a two-letter edge, broaden only if the corpus needs it.
  for (final word in {...labels, ...tags}) {
    for (final key in [
      '^${word.substring(0, 2)}',
      '${word.substring(word.length - 2)}\$',
    ]) {
      final bin = bins.putIfAbsent(key, () => []);
      if (bin.length < 256) bin.add(word);
    }
  }
  return bins;
}

bool _oneEditApart(String input, String candidate) {
  if ((input.length - candidate.length).abs() > 1) return false;
  final short = input.length <= candidate.length ? input : candidate;
  final long = input.length <= candidate.length ? candidate : input;
  var i = 0;
  while (i < short.length && short.codeUnitAt(i) == long.codeUnitAt(i)) {
    i++;
  }
  if (i == short.length) return short.length != long.length;
  final nextShort = short.length == long.length ? i + 1 : i;
  return short.substring(nextShort) == long.substring(i + 1);
}
