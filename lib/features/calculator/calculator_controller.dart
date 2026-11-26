import 'dart:async';
import 'dart:io' show pid;
import 'dart:math';

import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import 'calculator_client.dart';

/// Configurable integer interpretation settings sent on `preview`.
class CalculatorSettings {
  const CalculatorSettings({
    this.bitWidth = 32,
    this.signed = false,
    this.overflow = 'error',
  });

  final int bitWidth;
  final bool signed;
  final String overflow;

  static const bitWidths = [8, 16, 32, 64, 128];

  Map<String, Object?> toJson() => {
    'bitWidth': bitWidth,
    'signed': signed,
    'overflow': overflow,
  };

  CalculatorSettings copyWith({
    int? bitWidth,
    bool? signed,
    String? overflow,
  }) => CalculatorSettings(
    bitWidth: bitWidth ?? this.bitWidth,
    signed: signed ?? this.signed,
    overflow: overflow ?? this.overflow,
  );

  CalculatorSettings nextBitWidth() {
    final index = bitWidths.indexOf(bitWidth);
    final next = bitWidths[(index + 1) % bitWidths.length];
    return copyWith(bitWidth: next);
  }

  String get shortLabel => '${signed ? 'i' : 'u'}$bitWidth · $overflow';
}

/// UTF-16 range replacement supplied by completion or correction edits.
class CalculatorTextEdit {
  const CalculatorTextEdit({
    required this.startUtf16,
    required this.endUtf16,
    required this.text,
  });

  final int startUtf16;
  final int endUtf16;
  final String text;

  static CalculatorTextEdit? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final start = raw['startUtf16'] ?? raw['start_utf16'];
    final end = raw['endUtf16'] ?? raw['end_utf16'];
    final text = raw['text'];
    if (start is! int || end is! int || text is! String) return null;
    return CalculatorTextEdit(startUtf16: start, endUtf16: end, text: text);
  }

  /// Applies this edit to [current] if the UTF-16 span is valid and non-splitting.
  TextEditingValue? applyTo(TextEditingValue current) {
    final source = current.text;
    if (startUtf16 < 0 || endUtf16 < startUtf16 || endUtf16 > source.length) {
      return null;
    }
    if (_splitsSurrogate(source, startUtf16) ||
        _splitsSurrogate(source, endUtf16)) {
      return null;
    }
    final nextText = source.replaceRange(startUtf16, endUtf16, text);
    return TextEditingValue(
      text: nextText,
      selection: TextSelection.collapsed(offset: startUtf16 + text.length),
    );
  }

  bool wouldChange(String source) {
    if (startUtf16 < 0 || endUtf16 < startUtf16 || endUtf16 > source.length) {
      return false;
    }
    return source.substring(startUtf16, endUtf16) != text;
  }

  static bool _splitsSurrogate(String text, int index) {
    if (index <= 0 || index >= text.length) return false;
    final prev = text.codeUnitAt(index - 1);
    final curr = text.codeUnitAt(index);
    return prev >= 0xD800 && prev <= 0xDBFF && curr >= 0xDC00 && curr <= 0xDFFF;
  }
}

/// Alternate display and copy format for a [CalculatorCandidate].
class CalculatorFormat {
  const CalculatorFormat({
    required this.id,
    required this.label,
    required this.copyText,
  });

  final String id;
  final String label;
  final String copyText;

  static CalculatorFormat? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final id = raw['id']?.toString();
    final label = raw['label']?.toString();
    final copyText = (raw['copyText'] ?? raw['copy_text'] ?? label)?.toString();
    if (id == null || label == null || copyText == null) return null;
    return CalculatorFormat(id: id, label: label, copyText: copyText);
  }
}

/// Safe replacement query suggested by a diagnostic.
class CalculatorSuggestion {
  const CalculatorSuggestion({
    required this.id,
    required this.label,
    this.replacement,
  });

  final String id;
  final String label;
  final CalculatorTextEdit? replacement;

  static CalculatorSuggestion? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final id = raw['id']?.toString() ?? '';
    final label = raw['label']?.toString();
    if (label == null || label.isEmpty) return null;
    return CalculatorSuggestion(
      id: id.isEmpty ? label : id,
      label: label,
      replacement: CalculatorTextEdit.fromJson(raw['replacement']),
    );
  }
}

/// Structured error or warning diagnostic emitted by the calculator backend.
class CalculatorDiagnostic {
  const CalculatorDiagnostic({
    required this.code,
    required this.severity,
    this.messageKey,
    this.message,
    this.parameters = const {},
    this.suggestions = const [],
  });

  final String code;
  final String severity;
  final String? messageKey;
  final String? message;
  final Map<String, Object?> parameters;
  final List<CalculatorSuggestion> suggestions;

  bool get isError => severity == 'error';
  bool get isWarning => severity == 'warning';

  static CalculatorDiagnostic? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final code = raw['code']?.toString();
    if (code == null || code.isEmpty) return null;
    final severity = raw['severity']?.toString() ?? 'error';
    final paramsRaw = raw['parameters'];
    final parameters = paramsRaw is Map
        ? paramsRaw.map((k, v) => MapEntry(k.toString(), v))
        : const <String, Object?>{};
    final suggestionsRaw = raw['suggestions'];
    final suggestions = suggestionsRaw is List
        ? suggestionsRaw
              .map(CalculatorSuggestion.fromJson)
              .whereType<CalculatorSuggestion>()
              .toList(growable: false)
        : const <CalculatorSuggestion>[];
    return CalculatorDiagnostic(
      code: code,
      severity: severity,
      messageKey: (raw['messageKey'] ?? raw['message_key'])?.toString(),
      message: raw['message']?.toString(),
      parameters: parameters,
      suggestions: suggestions,
    );
  }

  String get displayMessage {
    if (message case final msg? when msg.isNotEmpty) return msg;
    final paramMsg = parameters['message'] ?? parameters['detail'];
    if (paramMsg is String && paramMsg.isNotEmpty) return paramMsg;
    if (parameters['original'] case final original?) {
      if (parameters['corrected'] case final corrected?) {
        return 'Corrected: $original → $corrected';
      }
    }
    return switch (code) {
      'invalid_syntax' => 'Invalid expression syntax',
      'incomplete_input' => 'Incomplete expression',
      'unknown_identifier' => switch (parameters['identifier'] ??
          parameters['token']) {
        final name? => 'Unknown identifier: $name',
        null => 'Unknown identifier',
      },
      'dimension_mismatch' => switch ((
        parameters['sourceDimension'] ?? parameters['source_dimension'],
        parameters['targetDimension'] ?? parameters['target_dimension'],
      )) {
        (final src?, final dst?) => 'Cannot convert $src to $dst',
        _ => 'Incompatible unit dimensions',
      },
      'unit_corrected' => switch ((
        parameters['originalToken'],
        parameters['correctedToken'],
      )) {
        (final original?, final corrected?) => 'Read “$original” as $corrected',
        _ => 'Unit spelling corrected',
      },
      'ambiguous_unit' => 'Ambiguous unit',
      'ambiguous_currency' => 'Ambiguous currency symbol',
      'ambiguous_timezone' => 'Ambiguous timezone',
      'nonexistent_local_time' => 'Nonexistent local time (DST gap)',
      'ambiguous_local_time' => 'Ambiguous local time (DST fold)',
      'numeric_overflow' => 'Numeric overflow',
      'resource_limit' => 'Expression exceeds complexity or size limit',
      'interrupted' => 'Calculation interrupted',
      'rate_unavailable' => 'Exchange rate unavailable',
      'rate_stale' => 'Exchange rate is stale',
      'unsupported_format' => 'Unsupported format',
      'backend_unavailable' => 'Calculator backend unavailable',
      'internal_error' => 'Internal calculator error',
      'variable_cycle' => 'Circular variable definition',
      _ => _humanizeCode(code),
    };
  }

  static String _humanizeCode(String code) {
    final text = code.replaceAll('_', ' ').trim();
    if (text.isEmpty) return 'Calculation error';
    return text[0].toUpperCase() + text.substring(1);
  }
}

/// Tagged value payload inside a [CalculatorCandidate].
class CalculatorValue {
  const CalculatorValue({
    required this.tag,
    required this.display,
    this.text,
    this.raw = const {},
  });

  final String tag;
  final String display;
  final String? text;
  final Map<String, Object?> raw;

  String get copyFallback =>
      text ??
      (display.isNotEmpty
          ? display
          : (raw['decimal'] ??
                    raw['storedDecimal'] ??
                    raw['bitsHex'] ??
                    raw['amount'] ??
                    '')
                .toString());

  String? str(String key, [String? altKey]) =>
      (raw[key] ?? (altKey == null ? null : raw[altKey]))?.toString();

  bool? flag(String key) => switch (raw[key]) {
    final bool value => value,
    _ => null,
  };

  static CalculatorValue? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final map = raw.map((k, v) => MapEntry(k.toString(), v));
    final tag = map['tag']?.toString() ?? 'display_only';
    final text = map['text']?.toString();
    final display =
        (map['display'] ??
                text ??
                map['storedDecimal'] ??
                map['targetCivil'] ??
                map['localDateTime'] ??
                map['decimal'] ??
                map['amount'] ??
                map['bitsHex'] ??
                '')
            .toString();
    return CalculatorValue(tag: tag, display: display, text: text, raw: map);
  }
}

/// One candidate interpretation returned by `preview` or stored in history.
class CalculatorCandidate {
  const CalculatorCandidate({
    required this.id,
    required this.kind,
    required this.normalized,
    required this.interpretation,
    this.assumptions = const [],
    this.value,
    this.formats = const [],
    this.diagnostics = const [],
    this.provenance,
    this.raw = const {},
  });

  final String id;
  final String kind;
  final String normalized;
  final String interpretation;
  final List<String> assumptions;
  final CalculatorValue? value;
  final List<CalculatorFormat> formats;
  final List<CalculatorDiagnostic> diagnostics;
  final Map<String, Object?>? provenance;
  final Map<String, Object?> raw;

  CalculatorFormat? formatById(String? id) {
    if (id == null) return null;
    for (final format in formats) {
      if (format.id == id) return format;
    }
    return null;
  }

  List<CalculatorValue> get floatValues => [
    if (value case final primary? when primary.tag == 'float_bits') ...[
      primary,
      if (primary.raw['alternatives'] case final List alternatives)
        ...alternatives
            .map(CalculatorValue.fromJson)
            .whereType<CalculatorValue>(),
    ],
  ];

  List<CalculatorFormat> get numberedFormats => floatValues.isEmpty
      ? formats
      : [
          for (final id in ['fp32', 'fp16', 'bf16']) ?formatById(id),
        ];

  CalculatorValue? floatValue(String? formatId) {
    final values = floatValues;
    return values.where((v) => v.str('format') == formatId).firstOrNull ??
        values.firstOrNull;
  }

  String primaryDisplay([String? formatId]) {
    if (floatValue(formatId)?.str('storedDecimal') case final stored?) {
      return stored;
    }
    if (formatById(formatId) case final format?) return format.label;
    final val = value?.display ?? '';
    if (val.isNotEmpty) return val;
    if (formats.isNotEmpty) return formats.first.label;
    return normalized;
  }

  String copyText([String? formatId]) {
    if (formatById(formatId) case final format?) return format.copyText;
    if (formats.isNotEmpty) return formats.first.copyText;
    final val = value?.copyFallback ?? '';
    if (val.isNotEmpty) return val;
    return normalized;
  }

  static CalculatorCandidate? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final map = raw.map((k, v) => MapEntry(k.toString(), v));
    final id = map['id']?.toString();
    if (id == null || id.isEmpty) return null;
    final assumptionsRaw = map['assumptions'];
    final assumptions = assumptionsRaw is List
        ? assumptionsRaw.map((e) => e.toString()).toList(growable: false)
        : const <String>[];
    final formatsRaw = map['formats'];
    final formats = formatsRaw is List
        ? formatsRaw
              .map(CalculatorFormat.fromJson)
              .whereType<CalculatorFormat>()
              .toList(growable: false)
        : const <CalculatorFormat>[];
    final diagsRaw = map['diagnostics'];
    final diagnostics = diagsRaw is List
        ? diagsRaw
              .map(CalculatorDiagnostic.fromJson)
              .whereType<CalculatorDiagnostic>()
              .toList(growable: false)
        : const <CalculatorDiagnostic>[];
    final provRaw = map['provenance'];
    final provenance = provRaw is Map
        ? provRaw.map((k, v) => MapEntry(k.toString(), v))
        : null;
    return CalculatorCandidate(
      id: id,
      kind: map['kind']?.toString() ?? 'math',
      normalized: map['normalized']?.toString() ?? '',
      interpretation: map['interpretation']?.toString() ?? '',
      assumptions: assumptions,
      value: CalculatorValue.fromJson(map['value']),
      formats: formats,
      diagnostics: diagnostics,
      provenance: provenance,
      raw: map,
    );
  }
}

/// Autocomplete or auto-correction item returned by `preview` or `complete`.
class CalculatorCompletion {
  const CalculatorCompletion({
    required this.id,
    required this.label,
    required this.kind,
    this.scoreTier,
    this.edit,
    this.previewCandidateId,
    this.subtitle,
  });

  final String id;
  final String label;
  final String kind;
  final String? scoreTier;
  final CalculatorTextEdit? edit;
  final String? previewCandidateId;
  final String? subtitle;

  static CalculatorCompletion? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final id = raw['id']?.toString() ?? '';
    final label = raw['label']?.toString();
    if (label == null || label.isEmpty) return null;
    return CalculatorCompletion(
      id: id.isEmpty ? label : id,
      label: label,
      kind: raw['kind']?.toString() ?? 'completion',
      scoreTier: (raw['scoreTier'] ?? raw['score_tier'])?.toString(),
      edit: CalculatorTextEdit.fromJson(raw['edit']),
      previewCandidateId:
          (raw['previewCandidateId'] ?? raw['preview_candidate_id'])
              ?.toString(),
      subtitle: (raw['subtitle'] ?? raw['description'] ?? raw['preview'])
          ?.toString(),
    );
  }
}

/// Catalog entry returned by `catalogSearch`.
class CalculatorCatalogEntry {
  const CalculatorCatalogEntry({
    required this.id,
    required this.kind,
    required this.symbol,
    required this.name,
    required this.description,
    this.example,
    this.definition,
  });

  final String id;
  final String kind;
  final String symbol;
  final String name;
  final String description;
  final String? example;
  final String? definition;

  String get insertText => symbol.isNotEmpty ? symbol : name;

  static CalculatorCatalogEntry? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final id = (raw['stableId'] ?? raw['stable_id'] ?? raw['id'])?.toString();
    final symbol =
        (raw['canonicalSymbol'] ?? raw['canonical_symbol'] ?? raw['symbol'])
            ?.toString() ??
        '';
    final name =
        (raw['englishName'] ??
                raw['english_name'] ??
                raw['name'] ??
                raw['label'])
            ?.toString() ??
        symbol;
    if (name.isEmpty && symbol.isEmpty) return null;
    final examples = raw['exampleQueries'] ?? raw['example_queries'];
    final example = examples is List && examples.isNotEmpty
        ? examples.first.toString()
        : raw['example']?.toString();
    final description =
        (raw['description'] ??
                raw['exactDefinition'] ??
                raw['exact_definition'] ??
                raw['summary'] ??
                raw['descriptionKey'] ??
                '')
            .toString();
    return CalculatorCatalogEntry(
      id: id ?? (symbol.isNotEmpty ? symbol : name),
      kind: raw['kind']?.toString() ?? 'catalog',
      symbol: symbol,
      name: name,
      description: description,
      example: example,
      definition: raw['exactDefinition']?.toString(),
    );
  }
}

/// Committed calculation history item returned by `historyList`.
class CalculatorHistoryEntry {
  const CalculatorHistoryEntry({
    required this.historyId,
    required this.query,
    this.candidate,
    this.formatId,
    this.committedAt,
    this.isPinned = false,
  });

  final String historyId;
  final String query;
  final CalculatorCandidate? candidate;
  final String? formatId;
  final String? committedAt;
  final bool isPinned;

  String get retainedResult =>
      candidate?.floatValue(formatId)?.str('storedDecimal') ??
      candidate?.primaryDisplay(formatId) ??
      query;
  String get copyText =>
      candidate?.floatValue(formatId)?.str('storedDecimal') ??
      candidate?.copyText(formatId) ??
      query;

  static CalculatorHistoryEntry? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final historyId = (raw['historyId'] ?? raw['history_id'] ?? raw['id'])
        ?.toString();
    final query = (raw['query'] ?? raw['rawQuery'] ?? raw['raw_query'])
        ?.toString();
    if (historyId == null || query == null) return null;
    return CalculatorHistoryEntry(
      historyId: historyId,
      query: query,
      candidate: CalculatorCandidate.fromJson(raw['candidate']),
      formatId: (raw['formatId'] ?? raw['format_id'])?.toString(),
      committedAt: (raw['committedAt'] ?? raw['committed_at_utc'])?.toString(),
      isPinned: raw['isPinned'] == true || raw['is_pinned'] == 1,
    );
  }
}

/// Saved user variable returned by `variablesList`.
class CalculatorVariableEntry {
  const CalculatorVariableEntry({required this.name, required this.expression});

  final String name;
  final String expression;

  static CalculatorVariableEntry? fromJson(Object? raw) {
    if (raw is! Map) return null;
    final name = raw['name']?.toString();
    final expression = raw['expression']?.toString();
    if (name == null || name.isEmpty || expression == null) return null;
    return CalculatorVariableEntry(name: name, expression: expression);
  }
}

/// Parsed response data for a `preview` request.
class CalculatorPreview {
  const CalculatorPreview({
    required this.status,
    this.revision,
    this.primaryCandidateId,
    this.candidates = const [],
    this.diagnostics = const [],
    this.completions = const [],
  });

  final String status;
  final int? revision;
  final String? primaryCandidateId;
  final List<CalculatorCandidate> candidates;
  final List<CalculatorDiagnostic> diagnostics;
  final List<CalculatorCompletion> completions;

  static CalculatorPreview fromData(
    Map<String, Object?> data, {
    int? revision,
  }) {
    final candidatesRaw = data['candidates'];
    final candidates = candidatesRaw is List
        ? candidatesRaw
              .map(CalculatorCandidate.fromJson)
              .whereType<CalculatorCandidate>()
              .take(16)
              .toList(growable: false)
        : const <CalculatorCandidate>[];
    final diagnosticsRaw = data['diagnostics'];
    final diagnostics = diagnosticsRaw is List
        ? diagnosticsRaw
              .map(CalculatorDiagnostic.fromJson)
              .whereType<CalculatorDiagnostic>()
              .toList(growable: false)
        : const <CalculatorDiagnostic>[];
    final completionsRaw = data['completions'];
    final completions = completionsRaw is List
        ? completionsRaw
              .map(CalculatorCompletion.fromJson)
              .whereType<CalculatorCompletion>()
              .take(12)
              .toList(growable: false)
        : const <CalculatorCompletion>[];
    return CalculatorPreview(
      status: data['status']?.toString() ?? 'empty',
      revision: revision,
      primaryCandidateId:
          (data['primaryCandidateId'] ?? data['primary_candidate_id'])
              ?.toString(),
      candidates: candidates,
      diagnostics: diagnostics,
      completions: completions,
    );
  }
}

/// An action presented in the calculator actions menu (`Ctrl+K`).
class CalculatorMenuAction {
  const CalculatorMenuAction({
    required this.id,
    required this.label,
    required this.icon,
    this.keys = const [],
    this.destructive = false,
    required this.run,
  });

  final String id;
  final String label;
  final LauncherIconName icon;
  final List<String> keys;
  final bool destructive;
  final Future<void> Function() run;
}

/// Coordinates calculator revisions, completions, error delay, history, and actions.
class CalculatorController extends ChangeNotifier {
  CalculatorController({
    CalculatorClient? client,
    this.errorDelay = const Duration(milliseconds: 600),
    this.debounceDelay = const Duration(milliseconds: 25),
  }) : _injectedClient = client;

  final CalculatorClient? _injectedClient;
  final Duration errorDelay;
  final Duration debounceDelay;

  CalculatorClient? _ownedClient;
  bool _disposed = false;
  LauncherScope _scope = .all;
  String _query = '';
  int _cursorUtf16 = 0;
  int _revision = 0;
  int? _inFlightRevision;
  final String _mutationSession =
      '$pid-${DateTime.now().microsecondsSinceEpoch}-${Random.secure().nextInt(1 << 32)}';
  int _idempotencyCounter = 0;
  bool _refreshingRates = false;
  bool _rememberChoice = false;

  String _mutationKey() => '$_mutationSession-${++_idempotencyCounter}';

  Timer? _debounceTimer;
  Timer? _errorTimer;

  static const _defaultSettings = CalculatorSettings();
  CalculatorSettings _settings = _defaultSettings;
  Future<void>? _loadingPreferences;

  Future<void> _loadPreferences() async {
    try {
      final envelope = await _ensureClient().request('status');
      final data = envelope['data'];
      if (data is! Map ||
          _disposed ||
          !identical(_settings, _defaultSettings)) {
        return;
      }
      final programmer = data['programmer'];
      if (programmer is! Map ||
          !CalculatorSettings.bitWidths.contains(programmer['bitWidth'])) {
        return;
      }
      _settings = CalculatorSettings(
        bitWidth: programmer['bitWidth'] as int,
        signed: programmer['signed'] == true,
        overflow: programmer['overflow'] == 'wrap' ? 'wrap' : 'error',
      );
    } on Object {
      // The preview request reports transport failures.
    } finally {
      _loadingPreferences = null;
    }
  }

  CalculatorPreview? _preview;
  List<CalculatorCompletion> _extraCompletions = const [];
  List<CalculatorCatalogEntry> _catalogEntries = const [];
  List<CalculatorHistoryEntry> _historyEntries = const [];
  List<CalculatorVariableEntry> _variableEntries = const [];

  String? _selectedCandidateId;
  String? _selectedFormatId;
  String? _selectedHistoryId;
  int _selectedCompletionIndex = 0;
  int selectedSuggestionIndex = 0;

  List<CalculatorSuggestion> get suggestions => [
    for (final diagnostic in errorDiagnostics) ...diagnostic.suggestions,
  ];

  void moveSuggestion(int delta) {
    if (suggestions.isEmpty) return;
    selectedSuggestionIndex = (selectedSuggestionIndex + delta).clamp(
      0,
      suggestions.length - 1,
    );
    notifyListeners();
  }

  String? get searchHint => switch (selectedCandidate?.kind) {
    'programmer' =>
      'bitwise · ${_settings.signed ? 'i' : 'u'}${_settings.bitWidth}',
    'time_conversion' => 'time zones',
    'currency' => 'reference rates',
    'unit_conversion' => selectedCandidate?.value?.str('unitSymbol'),
    'float_format' => switch (selectedCandidate?.floatValue(selectedFormatId)) {
      final value? =>
        '1 · ${value.str('exponentBits')?.length ?? 0} · ${value.str('fractionBits')?.length ?? 0} bits',
      _ => null,
    },
    _ when hasPendingCompletionOrCorrection => '→ to complete',
    _ when catalogEntries.isNotEmpty => 'catalog',
    _ when query.isEmpty => 'units · bases · time · fx',
    _ => null,
  };
  int _selectedCatalogIndex = 0;

  bool _loadingPreview = false;
  bool _loadingHistory = false;
  int _historyRead = 0;
  int _variablesRead = 0;
  bool _errorVisible = false;
  String? _backendError;
  String? _fxStatusMessage;

  bool _actionsOpen = false;
  String _actionFilter = '';
  int _actionIndex = 0;

  static final _urlPattern = RegExp(r'://|^www\.', caseSensitive: false);
  static final _windowsPathPattern = RegExp(r'^[a-zA-Z]:[\\/]');
  static final _fileExtPattern = RegExp(r'\.[a-zA-Z][a-zA-Z0-9]{1,5}$');
  static final _hyphenatedWordPattern = RegExp(
    r'^[a-zA-Z_][a-zA-Z0-9_]*(?:-[a-zA-Z0-9_]+)+(?:\.[a-zA-Z0-9]+)?$',
  );
  static final _radixLiteralPattern = RegExp(
    r'\b0[xX][0-9a-fA-F_]+\b|\b0[bB][01_]+\b|\b0[oO][0-7_]+\b',
  );
  static final _floatCastPattern = RegExp(
    r'\bas\s+(?:fp32|f32|fp16|f16|bf16|fp8|e4m3fn|e5m2|fp8_e4m3fn|fp8_e5m2)\b',
    caseSensitive: false,
  );
  static final _conversionStartPattern = RegExp(
    r'^(?:[¥€£$]?\s*[\d.(+-]|\d{1,2}(?::\d{2})?\s*(?:am|pm)\b|\d{4}-\d{2}-\d{2}\b)',
    caseSensitive: false,
  );
  static final _conversionJoinerPattern = RegExp(
    r'\s+(?:to|in|->)\s+\S+',
    caseSensitive: false,
  );
  static final _mathFunctionPattern = RegExp(
    r'^(?:sqrt|cbrt|abs|sin|cos|tan|asin|acos|atan|sinh|cosh|tanh|log|log10|log2|ln|exp|floor|ceil|round)\s*(?:\([^)]+\)|[\dπ]|pi\b)',
    caseSensitive: false,
  );
  static final _arithmeticOpPattern = RegExp(
    r'[+\-*/×÷^%&|!]|<<|>>|\d\s*\(|\)\s*\d|\)\s*\(',
  );
  static final _implicitMulPattern = RegExp(
    r'\d\s*(?:pi|π|sqrt)\b',
    caseSensitive: false,
  );

  /// Lightweight local classifier gating All-scope inline calculation requests.
  static bool isHighConfidenceAllQuery(String raw) {
    final q = raw.trim();
    if (q.isEmpty || q.length > 512) return false;
    if (q.startsWith('>') ||
        q.startsWith('/') ||
        q.startsWith('~/') ||
        q.startsWith('./') ||
        q.startsWith('../') ||
        _windowsPathPattern.hasMatch(q) ||
        _urlPattern.hasMatch(q)) {
      return false;
    }
    if (!q.contains(' ') && _fileExtPattern.hasMatch(q)) return false;
    if (_hyphenatedWordPattern.hasMatch(q)) return false;

    if (_radixLiteralPattern.hasMatch(q)) return true;
    if (_floatCastPattern.hasMatch(q)) return true;
    if (_conversionStartPattern.hasMatch(q) &&
        _conversionJoinerPattern.hasMatch(q)) {
      return true;
    }
    if (_mathFunctionPattern.hasMatch(q)) return true;
    if (_implicitMulPattern.hasMatch(q)) return true;

    final hasNumeric = RegExp(r'\d').hasMatch(q);
    if (!hasNumeric) return false;
    if (RegExp(r'[a-zA-Z]-[a-zA-Z0-9]').hasMatch(q) &&
        !RegExp(r'\s-\s').hasMatch(q)) {
      return false;
    }
    return _arithmeticOpPattern.hasMatch(q);
  }

  CalculatorSettings get settings => _settings;
  LauncherScope get scope => _scope;
  String get query => _query;
  int get revision => _revision;
  bool get loadingPreview => _loadingPreview;
  bool get loadingHistory => _loadingHistory;
  String? get backendError => _backendError;
  String? get fxStatusMessage => _fxStatusMessage;

  List<CalculatorCandidate> get candidates => _preview?.candidates ?? const [];
  List<CalculatorHistoryEntry> get historyEntries => _historyEntries;
  List<CalculatorVariableEntry> get variableEntries => _variableEntries;
  List<CalculatorCatalogEntry> get catalogEntries => _catalogEntries;
  String? get selectedFormatId => _selectedFormatId;
  String? get selectedHistoryId => _selectedHistoryId;
  int get selectedCompletionIndex => _selectedCompletionIndex;
  int get selectedCatalogIndex => _selectedCatalogIndex;

  bool get actionsOpen => _actionsOpen;
  String get actionFilter => _actionFilter;
  int get actionIndex => _actionIndex;

  List<CalculatorCompletion> get completions {
    final previewCompletions = _preview?.completions ?? const [];
    return previewCompletions.isNotEmpty
        ? previewCompletions
        : _extraCompletions;
  }

  CalculatorCandidate? get selectedCandidate {
    final list = candidates;
    if (list.isEmpty) return null;
    if (_selectedCandidateId case final id?) {
      for (final item in list) {
        if (item.id == id) return item;
      }
    }
    if (_preview?.primaryCandidateId case final primaryId?) {
      for (final item in list) {
        if (item.id == primaryId) return item;
      }
    }
    return list.first;
  }

  CalculatorHistoryEntry? get selectedHistory {
    if (_selectedHistoryId case final id?) {
      for (final entry in _historyEntries) {
        if (entry.historyId == id) return entry;
      }
    }
    return null;
  }

  CalculatorCatalogEntry? get selectedCatalogEntry {
    if (_catalogEntries.isEmpty) return null;
    final index = _selectedCatalogIndex.clamp(0, _catalogEntries.length - 1);
    return _catalogEntries[index];
  }

  CalculatorCompletion? get selectedCompletion {
    final list = completions;
    if (list.isEmpty) return null;
    final index = _selectedCompletionIndex.clamp(0, list.length - 1);
    return list[index];
  }

  List<CalculatorDiagnostic> get allDiagnostics => [
    ...?_preview?.diagnostics,
    ...?selectedCandidate?.diagnostics,
  ];

  List<CalculatorDiagnostic> get errorDiagnostics => allDiagnostics
      .where((d) => d.isError && d.code != 'incomplete_input')
      .toList(growable: false);

  List<CalculatorDiagnostic> get warningDiagnostics =>
      allDiagnostics.where((d) => d.isWarning).toList(growable: false);

  bool get hasPendingError =>
      _scope == .calculator &&
      _query.trim().isNotEmpty &&
      !_errorVisible &&
      (_preview?.status == 'error' || errorDiagnostics.isNotEmpty);

  bool get shouldShowError =>
      _scope == .calculator &&
      _query.trim().isNotEmpty &&
      _errorVisible &&
      (_preview?.status == 'error' || errorDiagnostics.isNotEmpty);

  /// Active correction edit from warning suggestions or correction completions.
  CalculatorTextEdit? get pendingCorrectionEdit {
    for (final completion in completions) {
      if (completion.kind == 'correction' &&
          completion.edit != null &&
          completion.edit!.wouldChange(_query)) {
        return completion.edit;
      }
    }
    for (final diag in warningDiagnostics.where(
      (diagnostic) => diagnostic.code == 'unit_corrected',
    )) {
      for (final suggestion in diag.suggestions) {
        if (suggestion.replacement != null &&
            suggestion.replacement!.wouldChange(_query)) {
          return suggestion.replacement;
        }
      }
    }
    return null;
  }

  /// Active completion edit (excluding no-op edits).
  CalculatorTextEdit? get pendingCompletionEdit {
    final comp = selectedCompletion;
    if (comp?.edit != null && comp!.edit!.wouldChange(_query)) {
      return comp.edit;
    }
    return null;
  }

  bool get hasPendingCompletionOrCorrection =>
      pendingCorrectionEdit != null || pendingCompletionEdit != null;

  bool get isCopyableResult {
    final candidate = selectedCandidate;
    if (candidate == null || _preview?.revision != _revision) return false;
    if (candidate.kind == 'catalog_search') return false;
    if (candidate.formats.isEmpty ||
        errorDiagnostics.isNotEmpty && _preview?.status == 'error') {
      return false;
    }
    return candidate.copyText(_selectedFormatId).isNotEmpty;
  }

  bool get hasInlineResult {
    if (_scope != .all || _backendError != null) return false;
    if (_preview == null || _preview?.status == 'error') return false;
    if (_preview?.status == 'partial' ||
        _preview?.status == 'incomplete_input') {
      return false;
    }
    return isCopyableResult;
  }

  bool get hasRateDiagnostic => allDiagnostics.any(
    (d) => d.code == 'rate_unavailable' || d.code == 'rate_stale',
  );

  bool get canOpenActions {
    if (_scope == .calculator && _query.trim().isEmpty) {
      return selectedHistory != null || _historyEntries.isNotEmpty;
    }
    return selectedCandidate != null ||
        _catalogEntries.isNotEmpty ||
        hasRateDiagnostic;
  }

  CalculatorClient _ensureClient() {
    if (_injectedClient case final injected?) return injected;
    return _ownedClient ??= CalculatorClient();
  }

  @override
  void dispose() {
    _disposed = true;
    _debounceTimer?.cancel();
    _errorTimer?.cancel();
    if (_ownedClient case final owned?) {
      unawaited(owned.close().catchError((_) {}));
    }
    super.dispose();
  }

  void setScope(LauncherScope scope, {String query = '', int? cursorUtf16}) {
    if (_disposed) return;
    _scope = scope;
    _actionsOpen = false;
    _actionFilter = '';
    _actionIndex = 0;
    _fxStatusMessage = null;
    _backendError = null;
    _query = query;
    _cursorUtf16 = (cursorUtf16 ?? query.length).clamp(0, query.length);
    _debounceTimer?.cancel();
    _errorTimer?.cancel();
    _errorVisible = false;

    final previous = _inFlightRevision;
    final rev = ++_revision;
    if (previous != null) unawaited(_cancelRevision(previous));

    if (scope == .calculator) {
      if (query.trim().isEmpty) {
        _clearPreviewState();
        unawaited(loadHistory());
        unawaited(loadVariables());
      } else {
        unawaited(loadHistory());
        unawaited(
          _runPreview(rev, query, cursorUtf16: _cursorUtf16, scope: 'calc'),
        );
      }
    } else if (scope == .all) {
      _historyEntries = const [];
      _variableEntries = const [];
      _selectedHistoryId = null;
      if (isHighConfidenceAllQuery(query)) {
        unawaited(
          _runPreview(rev, query, cursorUtf16: _cursorUtf16, scope: 'all'),
        );
      } else {
        _clearPreviewState();
      }
    } else {
      _clearPreviewState();
      _historyEntries = const [];
      _variableEntries = const [];
      _selectedHistoryId = null;
    }
    notifyListeners();
  }

  void updateQuery(String query, {int? cursorUtf16}) {
    if (_disposed) return;
    final nextCursor = (cursorUtf16 ?? query.length).clamp(0, query.length);
    if (_query == query && _cursorUtf16 == nextCursor) return;

    final textChanged = _query != query;
    _query = query;
    _cursorUtf16 = nextCursor;
    _fxStatusMessage = null;
    _backendError = null;

    if (!textChanged && _scope == .calculator && query.trim().isNotEmpty) {
      if (isCopyableResult) return;
      unawaited(_fetchCompletions(_revision, query, nextCursor));
      return;
    }

    _debounceTimer?.cancel();
    _errorTimer?.cancel();
    _errorVisible = false;

    final previous = _inFlightRevision;
    final rev = ++_revision;
    if (previous != null) unawaited(_cancelRevision(previous));

    if (_scope == .calculator) {
      if (query.trim().isEmpty) {
        _clearPreviewState();
        unawaited(loadHistory());
        unawaited(loadVariables());
        notifyListeners();
        return;
      }
      if (debounceDelay == .zero) {
        unawaited(
          _runPreview(rev, query, cursorUtf16: nextCursor, scope: 'calc'),
        );
      } else {
        _debounceTimer = Timer(debounceDelay, () {
          unawaited(
            _runPreview(rev, query, cursorUtf16: nextCursor, scope: 'calc'),
          );
        });
      }
      notifyListeners();
      return;
    }

    if (_scope == .all) {
      if (!isHighConfidenceAllQuery(query)) {
        _clearPreviewState();
        notifyListeners();
        return;
      }
      if (debounceDelay == .zero) {
        unawaited(
          _runPreview(rev, query, cursorUtf16: nextCursor, scope: 'all'),
        );
      } else {
        _debounceTimer = Timer(debounceDelay, () {
          unawaited(
            _runPreview(rev, query, cursorUtf16: nextCursor, scope: 'all'),
          );
        });
      }
      notifyListeners();
    }
  }

  void updateCursor(int cursorUtf16) {
    if (_disposed || _scope != .calculator || _query.trim().isEmpty) return;
    final clamped = cursorUtf16.clamp(0, _query.length);
    if (clamped == _cursorUtf16) return;
    _cursorUtf16 = clamped;
    if (isCopyableResult) return;
    unawaited(_fetchCompletions(_revision, _query, clamped));
  }

  void retry() {
    _backendError = null;
    final rev = ++_revision;
    if (_scope == .calculator && _query.trim().isEmpty) {
      unawaited(loadHistory());
      unawaited(loadVariables());
    } else if (_scope == .calculator && _query.trim().isNotEmpty) {
      unawaited(
        _runPreview(rev, _query, cursorUtf16: _cursorUtf16, scope: 'calc'),
      );
    } else if (_scope == .all && isHighConfidenceAllQuery(_query)) {
      unawaited(
        _runPreview(rev, _query, cursorUtf16: _cursorUtf16, scope: 'all'),
      );
    }
    notifyListeners();
  }

  void dismissError() {
    _errorVisible = false;
    notifyListeners();
  }

  void revealErrorNow() {
    _errorTimer?.cancel();
    if (!_errorVisible) {
      _errorVisible = true;
      notifyListeners();
    }
  }

  void selectCandidate(String candidateId) {
    if (_selectedCandidateId == candidateId) return;
    _selectedCandidateId = candidateId;
    _rememberChoice = true;
    final candidate = selectedCandidate;
    if (candidate != null && candidate.formatById(_selectedFormatId) == null) {
      _selectedFormatId = candidate.formats.firstOrNull?.id;
    }
    notifyListeners();
  }

  void cycleCandidate(int delta) {
    final list = candidates;
    if (list.length <= 1) return;
    final current = list.indexWhere((c) => c.id == selectedCandidate?.id);
    final base = current < 0 ? 0 : current;
    final next = (base + delta) % list.length;
    selectCandidate(list[next < 0 ? next + list.length : next].id);
  }

  void selectFormat(String formatId) {
    if (_selectedFormatId == formatId) return;
    _selectedFormatId = formatId;
    notifyListeners();
  }

  void cycleFormat(int delta) {
    final candidate = selectedCandidate;
    final formats =
        candidate?.numberedFormats
            .where((format) => !format.id.startsWith('pinned:'))
            .toList() ??
        const [];
    if (formats.length <= 1) return;
    final current = formats.indexWhere((f) => f.id == _selectedFormatId);
    final base = current < 0 ? 0 : current;
    final next = (base + delta) % formats.length;
    _selectedFormatId = formats[next < 0 ? next + formats.length : next].id;
    notifyListeners();
  }

  void moveHistorySelection(int delta) {
    if (_historyEntries.isEmpty) return;
    if (_selectedHistoryId == null) {
      _selectedHistoryId = _historyEntries.first.historyId;
    } else {
      final current = _historyEntries.indexWhere(
        (e) => e.historyId == _selectedHistoryId,
      );
      final base = current < 0 ? 0 : current;
      final next = (base + delta).clamp(0, _historyEntries.length - 1);
      _selectedHistoryId = _historyEntries[next].historyId;
    }
    notifyListeners();
  }

  void selectHistory(String historyId) {
    if (_selectedHistoryId == historyId) return;
    _selectedHistoryId = historyId;
    notifyListeners();
  }

  void moveCompletionSelection(int delta) {
    final list = completions;
    if (list.isEmpty) return;
    _selectedCompletionIndex = (_selectedCompletionIndex + delta).clamp(
      0,
      list.length - 1,
    );
    notifyListeners();
  }

  void moveCatalogSelection(int delta) {
    if (_catalogEntries.isEmpty) return;
    _selectedCatalogIndex = (_selectedCatalogIndex + delta).clamp(
      0,
      _catalogEntries.length - 1,
    );
    notifyListeners();
  }

  void setProgrammerMode(int width, {required bool signed}) {
    _settings = _settings.copyWith(bitWidth: width, signed: signed);
    unawaited(setPreference('programmer', _settings.toJson()));
    _reevaluateCurrent();
  }

  void cycleBitWidth() {
    _settings = _settings.nextBitWidth();
    unawaited(setPreference('programmer', _settings.toJson()));
    _reevaluateCurrent();
  }

  void toggleSigned() {
    _settings = _settings.copyWith(signed: !_settings.signed);
    unawaited(setPreference('programmer', _settings.toJson()));
    _reevaluateCurrent();
  }

  void toggleOverflow() {
    _settings = _settings.copyWith(
      overflow: _settings.overflow == 'error' ? 'wrap' : 'error',
    );
    unawaited(setPreference('programmer', _settings.toJson()));
    _reevaluateCurrent();
  }

  /// Persists a calculator preference key/value pair via `preferencesSet`.
  Future<void> setPreference(String key, Object? value) async {
    try {
      await _ensureClient().request('preferencesSet', {
        'key': key,
        'value': value,
        'idempotencyKey': _mutationKey(),
      });
    } on Object {
      // Ignore preference persistence errors if backend is unavailable.
    }
  }

  void _reevaluateCurrent() {
    if (_scope == .calculator && _query.trim().isNotEmpty) {
      final rev = ++_revision;
      unawaited(
        _runPreview(rev, _query, cursorUtf16: _cursorUtf16, scope: 'calc'),
      );
    }
    notifyListeners();
  }

  void openActions() {
    if (!canOpenActions) return;
    _actionsOpen = true;
    _actionFilter = '';
    _actionIndex = 0;
    notifyListeners();
  }

  void closeActions() {
    if (!_actionsOpen) return;
    _actionsOpen = false;
    _actionFilter = '';
    _actionIndex = 0;
    notifyListeners();
  }

  void updateActionFilter(String filter) {
    _actionFilter = filter;
    _actionIndex = 0;
    notifyListeners();
  }

  void moveActionSelection(int delta, int count) {
    if (count <= 0) return;
    final next = (_actionIndex + delta) % count;
    _actionIndex = next < 0 ? next + count : next;
    notifyListeners();
  }

  /// Loads persisted calculation history from the backend.
  Future<void> loadHistory() async {
    if (_disposed) return;
    final request = ++_historyRead;
    bool isCurrent() => !_disposed && request == _historyRead;
    _loadingHistory = true;
    notifyListeners();
    try {
      final envelope = await _ensureClient().request('historyList', {
        'limit': 20,
      });
      if (!isCurrent() || _scope != .calculator) {
        return;
      }
      if (envelope['ok'] == true && envelope['data'] is Map) {
        final data = (envelope['data'] as Map).map(
          (k, v) => MapEntry(k.toString(), v),
        );
        final rawEntries = data['entries'];
        final entries = rawEntries is List
            ? rawEntries
                  .map(CalculatorHistoryEntry.fromJson)
                  .whereType<CalculatorHistoryEntry>()
                  .toList(growable: false)
            : const <CalculatorHistoryEntry>[];
        _historyEntries = entries;
        if (!entries.any((e) => e.historyId == _selectedHistoryId)) {
          _selectedHistoryId = entries.firstOrNull?.historyId;
        }
        _backendError = null;
      }
    } on Object {
      if (isCurrent() && _scope == .calculator && _query.trim().isEmpty) {
        _backendError = 'Calculator service is unavailable.';
      }
    } finally {
      if (isCurrent()) {
        _loadingHistory = false;
        notifyListeners();
      }
    }
  }

  void _historyChanged() {
    _historyRead++;
    _loadingHistory = false;
  }

  /// Deletes a history entry by ID and refreshes the list.
  Future<void> deleteHistory(String historyId) async {
    try {
      final key = _mutationKey();
      final envelope = await _ensureClient().request('historyDelete', {
        'historyId': historyId,
        'idempotencyKey': key,
      });
      if (_disposed) return;
      if (envelope['ok'] == true) {
        _historyChanged();
        _historyEntries = _historyEntries
            .where((e) => e.historyId != historyId)
            .toList(growable: false);
        if (_selectedHistoryId == historyId) {
          _selectedHistoryId = _historyEntries.firstOrNull?.historyId;
        }
        notifyListeners();
      }
    } on Object {
      // Ignore deletion errors if backend is unavailable.
    }
  }

  /// Pins or unpins a history entry by ID.
  Future<void> setHistoryPinned(
    String historyId, {
    required bool pinned,
  }) async {
    try {
      final envelope = await _ensureClient().request('historyPin', {
        'historyId': historyId,
        'pinned': pinned,
        'idempotencyKey': _mutationKey(),
      });
      if (_disposed) return;
      if (envelope['ok'] == true) {
        _historyChanged();
        final updated = [
          for (final entry in _historyEntries)
            if (entry.historyId == historyId)
              CalculatorHistoryEntry(
                historyId: entry.historyId,
                query: entry.query,
                candidate: entry.candidate,
                formatId: entry.formatId,
                committedAt: entry.committedAt,
                isPinned: pinned,
              )
            else
              entry,
        ];
        updated.sort((a, b) {
          if (a.isPinned != b.isPinned) return a.isPinned ? -1 : 1;
          return 0;
        });
        _historyEntries = updated;
        notifyListeners();
      }
    } on Object {
      // Ignore pin errors if backend is unavailable.
    }
  }

  /// Clears calculator history, optionally including pinned entries.
  Future<void> clearHistory({bool includePinned = false}) async {
    try {
      final envelope = await _ensureClient().request('historyClear', {
        'includePinned': includePinned,
        'idempotencyKey': _mutationKey(),
      });
      if (_disposed) return;
      if (envelope['ok'] == true) {
        _historyChanged();
        _historyEntries = includePinned
            ? const []
            : _historyEntries.where((e) => e.isPinned).toList(growable: false);
        if (!_historyEntries.any((e) => e.historyId == _selectedHistoryId)) {
          _selectedHistoryId = _historyEntries.firstOrNull?.historyId;
        }
        notifyListeners();
      }
    } on Object {
      // Ignore clear errors if backend is unavailable.
    }
  }

  /// Loads saved calculator variables from the backend.
  Future<void> loadVariables() async {
    if (_disposed) return;
    final request = ++_variablesRead;
    try {
      final envelope = await _ensureClient().request('variablesList');
      if (_disposed ||
          request != _variablesRead ||
          _scope != .calculator ||
          _query.trim().isNotEmpty) {
        return;
      }
      if (envelope['ok'] == true && envelope['data'] is Map) {
        final data = (envelope['data'] as Map).map(
          (k, v) => MapEntry(k.toString(), v),
        );
        final rawEntries = data['entries'];
        _variableEntries = rawEntries is List
            ? rawEntries
                  .map(CalculatorVariableEntry.fromJson)
                  .whereType<CalculatorVariableEntry>()
                  .toList(growable: false)
            : const <CalculatorVariableEntry>[];
        notifyListeners();
      }
    } on Object {
      // Ignore supplemental variable list errors.
    }
  }

  /// Saves a named variable expression and returns an error message on failure.
  Future<String?> saveVariable(String name, String expression) async {
    final trimmedName = name.trim();
    final trimmedExpr = expression.trim();
    if (trimmedName.isEmpty || trimmedExpr.isEmpty) {
      return 'Variable name and expression are required.';
    }
    try {
      final envelope = await _ensureClient().request('saveVariable', {
        'name': trimmedName,
        'expression': trimmedExpr,
        'idempotencyKey': _mutationKey(),
      });
      if (_disposed) return null;
      if (envelope['ok'] == true) {
        _variablesRead++;
        _variableEntries = [
          CalculatorVariableEntry(name: trimmedName, expression: trimmedExpr),
          ..._variableEntries.where((v) => v.name != trimmedName),
        ];
        _reevaluateCurrent();
        return null;
      }
      final err = envelope['error'];
      if (err is Map && err['message'] != null) {
        return err['message'].toString();
      }
      return 'Could not save variable.';
    } on Object {
      return 'Could not save variable.';
    }
  }

  /// Deletes a saved variable by name.
  Future<void> deleteVariable(String name) async {
    try {
      final envelope = await _ensureClient().request('variableDelete', {
        'name': name,
        'idempotencyKey': _mutationKey(),
      });
      if (_disposed) return;
      if (envelope['ok'] == true) {
        _variablesRead++;
        _variableEntries = _variableEntries
            .where((v) => v.name != name)
            .toList(growable: false);
        notifyListeners();
      }
    } on Object {
      // Ignore deletion errors if backend is unavailable.
    }
  }

  /// Commits the current or specified candidate/format and returns text to copy.
  Future<String?> commitSelected({
    String? formatIdOverride,
    bool copyFloatBits = false,
  }) async {
    final candidate = selectedCandidate;
    if (candidate == null || !isCopyableResult) return null;
    final formatId = formatIdOverride ?? _selectedFormatId;
    final copyText = copyFloatBits
        ? candidate.copyText(formatId)
        : candidate.floatValue(formatId)?.str('storedDecimal') ??
              candidate.copyText(formatId);
    final commitRevision = _preview?.revision ?? _revision;
    final idempotencyKey = _mutationKey();
    try {
      final envelope = await _ensureClient().request('commit', {
        'candidateId': candidate.id,
        'revision': commitRevision,
        'idempotencyKey': idempotencyKey,
        'formatId': ?formatId,
        if (_rememberChoice) 'rememberChoice': true,
      });
      if (envelope['ok'] == true && _scope == .calculator) {
        unawaited(loadHistory());
      }
    } on Object {
      // Copy text remains valid even if persistence commit fails.
    }
    return copyText;
  }

  /// Explicitly triggers `fxRefresh` from the actions menu and re-evaluates.
  Future<void> refreshRates() async {
    if (_refreshingRates || _disposed) return;
    _refreshingRates = true;
    _fxStatusMessage = 'Refreshing ECB exchange rates…';
    notifyListeners();
    try {
      final envelope = await _ensureClient().request('fxRefresh', {
        'idempotencyKey': _mutationKey(),
      });
      if (_disposed) return;
      if (envelope['ok'] == true) {
        _fxStatusMessage = 'Exchange rates refreshed.';
        _reevaluateCurrent();
      } else {
        _fxStatusMessage = 'Could not refresh exchange rates.';
        notifyListeners();
      }
    } on Object {
      if (!_disposed) {
        _fxStatusMessage = 'Could not refresh exchange rates.';
        notifyListeners();
      }
    } finally {
      _refreshingRates = false;
    }
  }

  /// Builds the context-aware actions list for the actions overlay.
  List<CalculatorMenuAction> buildActions({
    required Future<void> Function(String text) onCopyText,
    required Future<void> Function() onCommitAndCopy,
    required void Function(String query) onEditQuery,
    Future<void> Function(String expression)? onSaveVariable,
    Future<void> Function()? onPasteResult,
    Future<void> Function()? onClearHistory,
  }) {
    final candidate = selectedCandidate;
    final history = _query.trim().isEmpty ? selectedHistory : null;
    final saveExpr = candidate != null && isCopyableResult
        ? (candidate.normalized.isNotEmpty
              ? candidate.normalized
              : _query.trim())
        : history?.query;
    final actions = <CalculatorMenuAction>[
      if (history != null) ...[
        CalculatorMenuAction(
          id: 'copy_history_result',
          label: 'Copy retained result (${history.retainedResult})',
          icon: .copy,
          keys: const ['↵'],
          run: () => onCopyText(history.copyText),
        ),
        CalculatorMenuAction(
          id: 'edit_history_query',
          label: 'Edit expression',
          icon: .text,
          keys: const [],
          run: () async => onEditQuery(history.query),
        ),
        CalculatorMenuAction(
          id: 'toggle_pin_history',
          label: history.isPinned ? 'Unpin history item' : 'Pin history item',
          icon: .star,
          keys: const ['Ctrl', 'D'],
          run: () =>
              setHistoryPinned(history.historyId, pinned: !history.isPinned),
        ),
        CalculatorMenuAction(
          id: 'delete_history',
          label: 'Delete history item',
          icon: .trash,
          keys: const ['⇧', 'Del'],
          destructive: true,
          run: () => deleteHistory(history.historyId),
        ),
      ],
      if (_scope == .calculator &&
          _query.trim().isEmpty &&
          _historyEntries.isNotEmpty &&
          onClearHistory != null)
        CalculatorMenuAction(
          id: 'clear_history',
          label: 'Clear calculator history…',
          icon: .trash,
          destructive: true,
          run: onClearHistory,
        ),
      if (candidate != null && isCopyableResult)
        CalculatorMenuAction(
          id: 'copy_selected_format',
          label: 'Copy result',
          icon: .copy,
          keys: const ['↵'],
          run: onCommitAndCopy,
        ),
      if (candidate != null && candidate.normalized.isNotEmpty)
        CalculatorMenuAction(
          id: 'copy_expression',
          label: 'Copy expression',
          icon: .code,
          keys: const ['Ctrl', '↵'],
          run: () => onCopyText(candidate.normalized),
        ),
      if (onPasteResult != null && isCopyableResult)
        CalculatorMenuAction(
          id: 'paste_result',
          label: 'Paste to last window',
          icon: .clipboard,
          keys: const ['Alt', '↵'],
          run: onPasteResult,
        ),
      if (onSaveVariable != null && saveExpr != null && saveExpr.isNotEmpty)
        CalculatorMenuAction(
          id: 'save_variable',
          label: 'Save as variable…',
          icon: .dataObject,
          keys: const ['Ctrl', 'S'],
          run: () => onSaveVariable(saveExpr),
        ),
      if (candidate != null)
        for (final (index, format) in candidate.numberedFormats.take(4).indexed)
          CalculatorMenuAction(
            id: 'copy_format_${format.id}',
            label: 'Copy as ${format.label}',
            icon: .swap,
            keys: ['Alt', '${index + 1}'],
            run: () async {
              selectFormat(format.id);
              final text = await commitSelected(
                formatIdOverride: format.id,
                copyFloatBits: true,
              );
              if (text != null) await onCopyText(text);
            },
          ),
      for (final alt in candidates)
        if (alt.id != candidate?.id)
          CalculatorMenuAction(
            id: 'choose_candidate_${alt.id}',
            label:
                'Use: ${alt.interpretation.isEmpty ? alt.primaryDisplay() : alt.interpretation}',
            icon: .tune,
            run: () async => selectCandidate(alt.id),
          ),
      if (candidate?.kind == 'programmer') ...[
        CalculatorMenuAction(
          id: 'cycle_bit_width',
          label: 'Cycle bit width (${_settings.bitWidth}-bit)',
          icon: .tune,
          keys: const ['Ctrl', 'B'],
          run: () async => cycleBitWidth(),
        ),
        CalculatorMenuAction(
          id: 'toggle_signed',
          label: _settings.signed
              ? 'Switch to unsigned integers'
              : 'Switch to signed integers',
          icon: .tune,
          run: () async => toggleSigned(),
        ),
        CalculatorMenuAction(
          id: 'toggle_overflow',
          label: _settings.overflow == 'error'
              ? 'Overflow mode: wrap'
              : 'Overflow mode: error',
          icon: .tune,
          run: () async => toggleOverflow(),
        ),
      ],
      if (candidate?.value?.tag == 'currency' ||
          candidate?.kind == 'currency' ||
          hasRateDiagnostic)
        CalculatorMenuAction(
          id: 'refresh_rates',
          label: 'Refresh exchange rates',
          icon: .history,
          run: refreshRates,
        ),
    ];

    final needle = _actionFilter.trim().toLowerCase();
    if (needle.isEmpty) return actions;
    return actions
        .where(
          (a) =>
              a.label.toLowerCase().contains(needle) ||
              a.id.toLowerCase().contains(needle),
        )
        .toList(growable: false);
  }

  Future<void> _runPreview(
    int rev,
    String query, {
    required int cursorUtf16,
    required String scope,
  }) async {
    if (_disposed || rev != _revision) return;
    _inFlightRevision = rev;
    _loadingPreview = true;
    notifyListeners();

    try {
      if (scope == 'calc' && identical(_settings, _defaultSettings)) {
        await (_loadingPreferences ??= Future<void>.microtask(
          _loadPreferences,
        ));
        if (_disposed || rev != _revision) return;
      }
      final envelope = await _ensureClient().request('preview', {
        'query': query,
        'revision': rev,
        'scope': scope,
        'cursorUtf16': cursorUtf16,
        if (scope == 'calc') 'settings': _settings.toJson(),
      });
      if (_disposed || rev != _revision) return;
      if (envelope['revision'] is int && envelope['revision'] != rev) return;

      if (envelope['ok'] != true) {
        _applyEnvelopeError(rev, scope, envelope['error']);
        return;
      }

      final rawData = envelope['data'];
      if (rawData is! Map) {
        _applyEnvelopeError(rev, scope, 'Malformed calculator preview.');
        return;
      }
      final data = rawData.map((k, v) => MapEntry(k.toString(), v));
      final parsed = CalculatorPreview.fromData(
        data,
        revision: envelope['revision'] is int
            ? envelope['revision'] as int
            : rev,
      );

      _preview = parsed;
      _extraCompletions = const [];
      _selectedCompletionIndex = 0;
      _selectedCatalogIndex = 0;
      selectedSuggestionIndex = 0;
      _backendError = null;

      _syncSelectedCandidateAndFormat(parsed);

      final hasErrorStatus =
          parsed.status == 'error' ||
          (parsed.candidates.isEmpty &&
              parsed.diagnostics.any((d) => d.isError));
      if (hasErrorStatus && scope == 'calc') {
        _errorTimer?.cancel();
        _errorTimer = Timer(errorDelay, () {
          if (!_disposed && _revision == rev) {
            _errorVisible = true;
            notifyListeners();
          }
        });
      } else {
        _errorTimer?.cancel();
        _errorVisible = false;
      }

      final catalog = data['catalogEntries'];
      _catalogEntries = catalog is List
          ? catalog
                .map(CalculatorCatalogEntry.fromJson)
                .whereType<CalculatorCatalogEntry>()
                .toList(growable: false)
          : const [];
    } on Object {
      if (!_disposed && rev == _revision && scope == 'calc') {
        _backendError = 'Calculator service is unavailable.';
      }
    } finally {
      if (_inFlightRevision == rev) _inFlightRevision = null;
      if (!_disposed && rev == _revision) {
        _loadingPreview = false;
        notifyListeners();
      }
    }
  }

  void _syncSelectedCandidateAndFormat(CalculatorPreview parsed) {
    final list = parsed.candidates;
    if (list.isEmpty) {
      _selectedCandidateId = null;
      _rememberChoice = false;
      _selectedFormatId = null;
      return;
    }
    final nextCandidate =
        list.where((c) => c.id == parsed.primaryCandidateId).firstOrNull ??
        list.first;
    _selectedCandidateId = nextCandidate.id;
    _rememberChoice = false;
    if (nextCandidate.formatById(_selectedFormatId) == null) {
      final base = nextCandidate.kind == 'programmer'
          ? switch (nextCandidate.value?.raw['radix']) {
              2 => 'binary',
              8 => 'octal',
              10 => 'decimal',
              16 => 'hex',
              _ => null,
            }
          : null;
      _selectedFormatId = base ?? nextCandidate.formats.firstOrNull?.id;
    }
  }

  void _applyEnvelopeError(int rev, String scope, Object? error) {
    final code = error is Map
        ? (error['code']?.toString() ?? 'internal_error')
        : 'internal_error';
    final msg = error is Map
        ? (error['message']?.toString() ?? error['code']?.toString())
        : error?.toString();
    if (code == 'backend_unavailable') {
      if (scope == 'calc') {
        _backendError = msg ?? 'Calculator service is unavailable.';
      }
      return;
    }
    final diag = CalculatorDiagnostic(
      code: code,
      severity: 'error',
      message: msg,
    );
    _preview = CalculatorPreview(
      status: 'error',
      revision: rev,
      diagnostics: [diag],
    );
    if (scope == 'calc') {
      _errorTimer?.cancel();
      _errorTimer = Timer(errorDelay, () {
        if (!_disposed && _revision == rev) {
          _errorVisible = true;
          notifyListeners();
        }
      });
    }
  }

  Future<void> _fetchCompletions(int rev, String query, int cursorUtf16) async {
    try {
      final envelope = await _ensureClient().request('complete', {
        'query': query,
        'cursorUtf16': cursorUtf16,
        'revision': rev,
      });
      if (_disposed ||
          rev != _revision ||
          _scope != .calculator ||
          cursorUtf16 != _cursorUtf16 ||
          isCopyableResult) {
        return;
      }
      if (envelope['ok'] != true || envelope['data'] is! Map) return;
      final data = (envelope['data'] as Map).map(
        (k, v) => MapEntry(k.toString(), v),
      );
      final raw = data['completions'];
      if (raw is! List) return;
      _extraCompletions = raw
          .map(CalculatorCompletion.fromJson)
          .whereType<CalculatorCompletion>()
          .take(12)
          .toList(growable: false);
      _selectedCompletionIndex = 0;
      notifyListeners();
    } on Object {
      // Ignore supplemental completion errors.
    }
  }

  Future<void> _cancelRevision(int rev) async {
    try {
      await _ensureClient().request('cancel', {'revision': rev + 1});
    } on Object {
      // Cancellation is best-effort.
    }
  }

  void _clearPreviewState() {
    _preview = null;
    _extraCompletions = const [];
    _catalogEntries = const [];
    _selectedCandidateId = null;
    _rememberChoice = false;
    _selectedFormatId = null;
    _selectedCompletionIndex = 0;
    _selectedCatalogIndex = 0;
    _loadingPreview = false;
  }
}
