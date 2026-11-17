import 'package:unorm_dart/unorm_dart.dart' as unorm;

import 'emoji_search.dart';

abstract final class SymbolCatalog {
  static final _symbols = [
    for (final MapEntry(key: group, value: rows) in _data.entries)
      for (final (codePoint, name, aliases) in rows)
        _Symbol(codePoint, name, aliases, group),
  ];
  static final _byCharacter = {
    for (final symbol in _symbols) symbol.result.emoji: symbol.result,
  };
  static final _byCodePoint = {
    for (final symbol in _symbols) symbol.codePoint: symbol.result,
  };

  static List<EmojiResult> browse() => [
    for (final symbol in _symbols) symbol.result,
  ];

  static List<EmojiResult> search(String query, {int limit = 60}) {
    if (limit <= 0 || query.length > 256 || query.runes.length > 128) {
      return const [];
    }
    final raw = query.trim();
    if (raw.isEmpty) return browse().take(limit).toList();
    if (_byCharacter[raw] case final result?) return [result];
    final hex = _hex.firstMatch(raw);
    final codePoint = hex?.group(1) ?? hex?.group(2);
    if (codePoint != null) {
      if (_byCodePoint[codePoint.toUpperCase().padLeft(4, '0')]
          case final result?) {
        return [result];
      }
      return const [];
    }

    final term = _normalize(raw);
    final ranked = <(int, int, EmojiResult)>[];
    for (final (index, symbol) in _symbols.indexed) {
      if (symbol.match(term) case final match?) {
        ranked.add((match.rank, index, symbol.withMatch(match.term)));
      }
    }
    ranked.sort((a, b) {
      final rank = a.$1.compareTo(b.$1);
      return rank == 0 ? a.$2.compareTo(b.$2) : rank;
    });
    return [for (final (_, _, result) in ranked.take(limit)) result];
  }
}

final _hex = RegExp(
  r'^(?:(?:u\+|0x)0*([0-9a-f]{1,6})|([0-9a-f]{4,6}))$',
  caseSensitive: false,
);
final _spaces = RegExp(r'\s+');

String _normalize(String value) =>
    unorm.nfkc(value).toLowerCase().replaceAll(_spaces, ' ').trim();

class _Symbol {
  _Symbol(this.codePoint, String name, String aliases, String group)
    : result = EmojiResult(
        emoji: String.fromCharCode(int.parse(codePoint, radix: 16)),
        name: name,
        group: group,
        codePoints: 'U+$codePoint',
        kind: .symbol,
      ),
      _terms = {
        for (final term in [name, ...aliases.split('|'), group])
          _normalize(term): term,
      };

  final String codePoint;
  final EmojiResult result;
  final Map<String, String> _terms;

  ({int rank, String term})? match(String query) {
    ({int rank, String term})? best;
    for (final MapEntry(key: key, value: term) in _terms.entries) {
      if (!key.contains(query)) continue;
      final rank =
          (key == query
              ? 0
              : key.startsWith(query)
              ? 2
              : 4) +
          (term == result.name ? 0 : 1);
      if (best == null || rank < best.rank) best = (rank: rank, term: term);
    }
    return best;
  }

  EmojiResult withMatch(String term) => EmojiResult(
    emoji: result.emoji,
    name: result.name,
    group: result.group,
    codePoints: result.codePoints,
    matchedTerm: term,
    kind: .symbol,
  );
}

// Code points and lowercased character names come from UnicodeData.txt.
// See tool/emoji/README.md for the source and the independent catalog check.
const _data = <String, List<(String, String, String)>>{
  "Legal & currency": [
    ("00A9", "copyright sign", "copyright|copr|版权|版權"),
    (
      "00AE",
      "registered sign",
      "registered trademark|registered mark|registered trade mark|注册商标|註冊商標",
    ),
    ("2122", "trade mark sign", "trademark|trade mark|商标|商標"),
    ("2120", "service mark", "service mark"),
    ("00A7", "section sign", "section|paragraph"),
    ("00B6", "pilcrow sign", "pilcrow|paragraph sign"),
    ("00A2", "cent sign", "cent"),
    ("20AC", "euro sign", "euro"),
    ("00A3", "pound sign", "pound sterling|sterling"),
    ("00A5", "yen sign", "yen|yuan"),
    ("20A9", "won sign", "won"),
    ("20B9", "indian rupee sign", "rupee|indian rupee"),
    ("20BF", "bitcoin sign", "bitcoin|btc"),
    ("20BD", "ruble sign", "ruble|rouble"),
    ("20BA", "turkish lira sign", "lira"),
    ("20B4", "hryvnia sign", "hryvnia"),
    ("20AB", "dong sign", "dong"),
  ],
  "Arrows": [
    ("2192", "rightwards arrow", "right arrow|rightwards arrow|向右箭头|向右箭頭|右矢印"),
    ("2190", "leftwards arrow", "left arrow|leftwards arrow|向左箭头|向左箭頭|左矢印"),
    ("2191", "upwards arrow", "up arrow|upwards arrow|向上箭头|向上箭頭"),
    ("2193", "downwards arrow", "down arrow|downwards arrow|向下箭头|向下箭頭"),
    ("2194", "left right arrow", "left right arrow|horizontal arrow"),
    ("2195", "up down arrow", "up down arrow|vertical arrow"),
    ("2196", "north west arrow", "north west arrow|up left arrow"),
    ("2197", "north east arrow", "north east arrow|up right arrow"),
    ("2198", "south east arrow", "south east arrow|down right arrow"),
    ("2199", "south west arrow", "south west arrow|down left arrow"),
    ("21D2", "rightwards double arrow", "right double arrow|implies"),
    (
      "21D4",
      "left right double arrow",
      "left right double arrow|if and only if|iff",
    ),
    ("21A9", "leftwards arrow with hook", "return arrow|reply|enter"),
    (
      "21B5",
      "downwards arrow with corner leftwards",
      "return|line break|carriage return|enter",
    ),
    ("21E7", "upwards white arrow", "shift key|shift"),
  ],
  "Math & science": [
    ("00B1", "plus-minus sign", "plus minus|plus or minus|+/-|正负号|正負號"),
    ("2213", "minus-or-plus sign", "minus plus|minus or plus|-/+"),
    ("00D7", "multiplication sign", "times|multiply|multiplication|乘号|乘號"),
    ("00F7", "division sign", "divide|division|除号|除號"),
    ("2212", "minus sign", "minus|subtraction"),
    ("221E", "infinity", "infinity|infinite|無限|无限"),
    ("2211", "n-ary summation", "summation|sum|sigma notation|求和"),
    ("220F", "n-ary product", "product|product notation"),
    ("221A", "square root", "square root|radical|根号|根號"),
    ("221B", "cube root", "cube root"),
    ("222B", "integral", "integral|integrate"),
    ("2202", "partial differential", "partial derivative"),
    ("2206", "increment", "delta|increment"),
    ("2248", "almost equal to", "almost equal|approximately equal"),
    ("2260", "not equal to", "not equal|unequal|not equals"),
    ("2264", "less-than or equal to", "less than or equal|at most"),
    ("2265", "greater-than or equal to", "greater than or equal|at least"),
    ("2261", "identical to", "identical|equivalence|congruent"),
    ("2208", "element of", "element of|member of"),
    ("2209", "not an element of", "not element of"),
    ("2205", "empty set", "empty set|null set"),
    ("2229", "intersection", "intersection|set intersection"),
    ("222A", "union", "union|set union"),
    ("2282", "subset of", "proper subset|subset"),
    ("2286", "subset of or equal to", "subset or equal|subset of"),
    ("2283", "superset of", "proper superset|superset"),
    ("2287", "superset of or equal to", "superset or equal|superset of"),
    ("00B0", "degree sign", "degree|angle|degrees"),
    ("2032", "prime", "prime|foot|minutes"),
    ("2033", "double prime", "double prime|inch|seconds"),
    ("2109", "degree fahrenheit", "fahrenheit|degrees fahrenheit"),
    ("2103", "degree celsius", "celsius|degrees celsius"),
    ("00B5", "micro sign", "micro|mu"),
    ("2030", "per mille sign", "per mille|permille"),
    ("03C0", "greek small letter pi", "pi"),
  ],
  "Typography": [
    (
      "2026",
      "horizontal ellipsis",
      "ellipsis|horizontal dots|three dots|...|省略号|省略號",
    ),
    ("22EF", "midline horizontal ellipsis", "midline ellipsis|centered dots"),
    ("00B7", "middle dot", "middle dot|interpunct|center dot"),
    ("2022", "bullet", "bullet|list bullet"),
    ("2023", "triangular bullet", "triangular bullet"),
    ("2013", "en dash", "en dash|medium dash"),
    ("2014", "em dash", "em dash|long dash"),
    (
      "201C",
      "left double quotation mark",
      "opening double quote|left double quote",
    ),
    (
      "201D",
      "right double quotation mark",
      "closing double quote|right double quote",
    ),
    (
      "2018",
      "left single quotation mark",
      "opening single quote|left single quote",
    ),
    (
      "2019",
      "right single quotation mark",
      "closing single quote|right single quote|apostrophe",
    ),
    (
      "00AB",
      "left-pointing double angle quotation mark",
      "opening guillemet|left angle quote",
    ),
    (
      "00BB",
      "right-pointing double angle quotation mark",
      "closing guillemet|right angle quote",
    ),
    ("2020", "dagger", "dagger|obelisk"),
    ("2021", "double dagger", "double dagger"),
    ("2042", "asterism", "asterism|section break"),
    ("2116", "numero sign", "number sign|numero|number abbreviation"),
    ("2713", "check mark", "tick|check|checkmark"),
    ("2717", "ballot x", "ballot cross|x mark|cross mark"),
  ],
  "Shapes & music": [
    ("2605", "black star", "black star|filled star"),
    ("2606", "white star", "white star|outline star"),
    ("25C6", "black diamond", "black diamond|filled diamond"),
    ("25C7", "white diamond", "white diamond|outline diamond"),
    ("25CB", "white circle", "white circle|outline circle"),
    ("25CF", "black circle", "black circle|filled circle"),
    ("25A0", "black square", "black square|filled square"),
    ("25A1", "white square", "white square|outline square"),
    ("25B2", "black up-pointing triangle", "triangle|up triangle"),
    ("25BC", "black down-pointing triangle", "triangle|down triangle"),
    ("266A", "eighth note", "eighth note|musical note"),
    ("266B", "beamed eighth notes", "beamed eighth notes|music"),
    ("266F", "music sharp sign", "sharp|music sharp"),
    ("266D", "music flat sign", "flat|music flat"),
    ("266E", "music natural sign", "natural|music natural"),
  ],
  "Computing": [
    ("2318", "place of interest sign", "command key|command|mac command"),
    ("2325", "option key", "option key|option|alt"),
    ("2303", "up arrowhead", "control key|control|ctrl"),
    ("232B", "erase to the left", "backspace|delete left"),
    ("2326", "erase to the right", "forward delete|delete right"),
    ("238B", "broken circle with northwest arrow", "escape key|escape|esc"),
    ("23CE", "return symbol", "return key|enter|return"),
  ],
};
