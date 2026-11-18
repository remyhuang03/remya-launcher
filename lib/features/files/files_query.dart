/// Separates completed filter tokens from editable input. The helper parses the
/// same syntax again and enforces filters before it publishes results.
final _filter = RegExp(
  r'''(?:^|\s)(ext|type|in|mod|size|hidden|git):("(?:\\.|[^"\\])+"|[^\s"]+)(?=\s)(?:\s*$)?''',
);

(String, List<String>) filesCommitFilters(String input) {
  final filters = <String>[];
  final query = input.replaceAllMapped(_filter, (match) {
    final value = match[2]!;
    final valid = switch (match[1]!) {
      'ext' => RegExp(r'^[\w.+-]+(,[\w.+-]+)*$').hasMatch(value),
      'type' => const ['file', 'dir', 'link'].contains(value),
      'in' => value.isNotEmpty,
      'mod' => _validModified(value),
      'size' => RegExp(
        r'^[<>]\d+(?:[KMGkmg][Ii]?[Bb]?|[Bb])?$',
      ).hasMatch(value),
      'hidden' => value == 'yes' || value == 'no',
      'git' => value == 'changed',
      _ => false,
    };
    if (!valid) return match[0]!;
    filters.add('${match[1]}:$value');
    return '';
  });
  return (filters.isEmpty ? query : query.trimLeft(), filters);
}

bool _validModified(String value) {
  if (value == 'today') return true;
  if (RegExp(r'^\d+d$').hasMatch(value)) {
    final days = int.tryParse(value.substring(0, value.length - 1));
    return days != null && days <= 0xffffffff;
  }
  if (!RegExp(r'^>\d{4}-\d{2}-\d{2}$').hasMatch(value)) return false;
  final date = value.substring(1);
  final parsed = DateTime.tryParse(date);
  return parsed != null &&
      parsed.year > 0 &&
      parsed.toIso8601String().startsWith(date);
}

Map<String, List<String>> filesFilterValues(String? place) => {
  'ext': ['pdf', 'md', 'png,jpg', 'rs', 'dart'],
  'type': ['file', 'dir', 'link'],
  'in': ['~/', ?place],
  'mod': ['today', '7d', '30d'],
  'size': ['>10M', '<1K'],
  'hidden': ['yes', 'no'],
  'git': ['changed'],
};

String filesQuoteFilter(String value) => RegExp(r'[\s"\\]').hasMatch(value)
    ? '"${value.replaceAll(r'\', r'\\').replaceAll('"', r'\"')}"'
    : value;

bool filesStartsPath(String query) =>
    query.startsWith('/') ||
    query.startsWith('~') ||
    query.startsWith('./') ||
    query.startsWith('../') ||
    RegExp(r'^\$[A-Za-z_]\w*/').hasMatch(query);
