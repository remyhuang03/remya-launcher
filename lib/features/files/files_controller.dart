import 'dart:async';

import 'package:flutter/foundation.dart';

import 'files_client.dart';
import 'files_models.dart';
import 'files_preferences.dart';
import 'files_query.dart';

class FilesController extends ChangeNotifier {
  FilesController({required this.client, required this.preferences});

  final FilesClient? client;
  final FilesPreferences preferences;
  final filters = <String>[];
  final _expanded = <String>{};
  Map<String, Map<String, dynamic>> _preferences = {};
  Future<void>? _loadingPreferences;
  final _preferenceWatches = <StreamSubscription<Map<String, dynamic>>>[];
  FilesSearchResult? result;
  Map<String, dynamic>? preview;
  String? selectedKey, diagnostic, status, undoId;
  String? _directoryId, _lastDirectoryId;
  bool _browseOpaqueDirectory = false;
  bool _includeIgnored = false;
  String query = '';
  bool dedicated = false;
  bool active = false;
  bool contents = false;
  bool showPreview = true;
  bool showHidden = false;
  bool loading = false;
  bool previewLoading = false;
  bool busy = false;
  bool failed = false;
  bool _pickedResult = false;
  bool _disposed = false;
  Timer? _searchDelay, _previewDelay, _undoTimer;
  int _searchGeneration = 0, _previewGeneration = 0;
  (String, int?, Object?)? _previewRequest;

  bool get available => client != null;
  bool get historyEnabled =>
      _preferences[FilesPreferences.historyId]?['enabled'] == true;
  bool get contentsEnabled =>
      _preferences['files-search']?['contents_enabled'] != false;
  FilesSearchMode get mode =>
      result?.mode ??
      (contents
          ? .contents
          : filesStartsPath(query) || _browseOpaqueDirectory
          ? .path
          : query.isEmpty && filters.isEmpty
          ? .empty
          : .names);

  List<FileGroup> get groups {
    final entries = result?.entries ?? const [];
    if (!dedicated) {
      if (query.trim().isEmpty) {
        return [
          if (_group(entries, 'pinned', limit: 6) case final pinned
              when pinned.isNotEmpty)
            FileGroup('PINNED FILES', pinned),
        ];
      }
      final choices = _group(
        entries.where((e) => !e.isPlace && e.line == null),
        'recent',
        limit: 3,
      );
      final ids = choices.map((c) => c.entry!.id).toSet();
      choices.addAll(
        entries
            .where((e) => !e.isPlace && e.line == null && ids.add(e.id))
            .take(3 - choices.length)
            .map(FileChoice.entry),
      );
      return choices.isEmpty ? const [] : [FileGroup('FILES', choices)];
    }

    final places = _group(entries, 'places', limit: 6);
    final pinned = _group(entries, 'pinned');
    final recent = _group(entries, 'recent');
    final regular = [
      for (final entry in entries)
        if (entry.group == 'results') FileChoice.entry(entry),
    ];
    final createName = result?.createName;
    final createFirst =
        createName != null &&
        (regular.isEmpty || result?.createDirectoryOnly == true);
    final create = createName == null
        ? null
        : FileGroup('CREATE', [
            if (result?.createDirectoryOnly != true)
              FileChoice(
                .createFile,
                'Create $createName',
                detail: result?.createPath ?? '',
              ),
            FileChoice(
              .createDirectory,
              'Create folder $createName/',
              detail: result?.createPath ?? '',
            ),
          ]);
    final candidates = _filterCandidates;
    return [
      if (candidates.isNotEmpty)
        FileGroup('ADD FILTER', candidates, hint: 'Tab accept'),
      if (places.isNotEmpty)
        FileGroup(
          'PLACES',
          places,
          hint: 'Alt + 1–${places.length}',
          tiles: true,
        ),
      if (pinned.isNotEmpty) FileGroup('PINNED', pinned),
      if (recent.isNotEmpty) FileGroup('RECENT', recent),
      if (createFirst && create != null) create,
      if (regular.isNotEmpty && mode == .contents)
        ..._contentsGroups(regular)
      else if (regular.isNotEmpty)
        FileGroup(
          createFirst
              ? 'SIMILAR NAMES'
              : mode == .path
              ? result?.directory ?? regular.first.entry?.location ?? 'PATH'
              : filters.isEmpty
              ? 'EVERYWHERE'
              : 'MATCHES',
          regular,
          hint: mode == .path && result?.totalCount != null
              ? '${regular.length} of ${result!.totalCount}'
              : mode == .names
              ? 'Filesystem'
              : null,
        ),
      if (!createFirst && create != null) create,
      if (regular.firstOrNull?.entry?.location case final location?
          when mode == .names && candidates.isEmpty && filters.isNotEmpty)
        FileGroup('ADD FILTER', [
          if (!filters.any((f) => f.startsWith('in:')))
            FileChoice(
              .filter,
              'in:${filesQuoteFilter(location)}',
              detail: 'Search within this folder',
            ),
          const FileChoice(.filter, 'type:file', detail: 'Show only files'),
        ], hint: 'key:value + space'),
      if (contentsEnabled &&
          !loading &&
          !failed &&
          candidates.isEmpty &&
          (createName != null ||
              (mode == .names &&
                  query.isNotEmpty &&
                  regular.isEmpty &&
                  recent.isEmpty)))
        FileGroup('ELSEWHERE', [
          FileChoice(
            .contents,
            'Search contents for “${createName ?? query.trim()}”',
            detail: 'Search text in files',
          ),
        ], hint: 'Ctrl + G'),
    ];
  }

  List<FileChoice> _group(
    Iterable<FileEntry> entries,
    String group, {
    int? limit,
  }) {
    final matching = entries.where((entry) => entry.group == group);
    return (limit == null ? matching : matching.take(limit))
        .map(FileChoice.entry)
        .toList();
  }

  List<FileChoice> get _filterCandidates {
    final prefix = RegExp(r'(?:^|\s)(\w+):([^\s"]*)$').firstMatch(query);
    if (prefix == null) return const [];
    final key = prefix[1]!;
    final value = prefix[2]!;
    final location = result?.entries.firstOrNull?.location;
    return [
      for (final candidate in filesFilterValues(location)[key] ?? const [])
        if (candidate.startsWith(value))
          FileChoice(
            .filter,
            '$key:${filesQuoteFilter(candidate)}',
            detail: 'Refine file results',
          ),
    ];
  }

  List<FileGroup> _contentsGroups(List<FileChoice> choices) {
    final byFolder = <String, Map<String, List<FileChoice>>>{};
    for (final choice in choices) {
      final entry = choice.entry!;
      (byFolder
              .putIfAbsent(entry.location, () => {})
              .putIfAbsent(entry.id, () => []))
          .add(choice);
    }
    return [
      for (final folder in byFolder.entries)
        FileGroup(folder.key, [
          for (final matches in folder.value.values) ...[
            if (matches.any((choice) => choice.entry!.line == null))
              matches.firstWhere((choice) => choice.entry!.line == null),
            ..._visibleMatches(matches),
          ],
        ]),
    ];
  }

  Iterable<FileChoice> _visibleMatches(List<FileChoice> choices) sync* {
    final hits = choices.where((choice) => choice.entry!.line != null).toList();
    final entry = choices.first.entry!;
    if (_expanded.contains(entry.id) || hits.length <= 3) {
      yield* hits;
      return;
    }
    yield* hits.take(3);
    final count = hits.length - 3;
    yield FileChoice(
      .more,
      (entry.hitCount ?? hits.length) > hits.length
          ? 'Show $count more available hits'
          : '+$count more',
      entry: entry,
    );
  }

  List<FileChoice> get choices => [
    for (final group in groups) ...group.choices,
  ];

  FileChoice? get selected {
    final items = choices;
    return items.where((choice) => choice.key == selectedKey).firstOrNull ??
        items.firstOrNull;
  }

  String? get completion {
    final candidate = selected?.entry;
    if (candidate?.completion case final path?
        when mode == .path || candidate?.isDirectory == true) {
      if (query.startsWith('~') || mode != .path) {
        final display = candidate?.displayPath;
        if (display != null && display.startsWith('~')) {
          return candidate!.isDirectory && !display.endsWith('/')
              ? '$display/'
              : display;
        }
      }
      return path;
    }
    return null;
  }

  String? get completionSuffix {
    if (mode != .path || loading) return null;
    final path = completion;
    return path != null && path.length > query.length && path.startsWith(query)
        ? path.substring(query.length)
        : null;
  }

  String? get hint {
    if (loading) return contents ? 'Searching contents…' : 'Searching…';
    if (failed) return 'Search unavailable';
    if (completionSuffix != null) return null;
    if (selected?.isCreate == true) return 'Path not found';
    if (query.isEmpty && filters.isNotEmpty) return null;
    if (mode == .empty) return 'Filesystem + recent';
    if (mode == .contents && query.trim().runes.length < 3) {
      return 'Type at least 3 characters';
    }
    if (result case final result?) {
      final count = result.matchedCount ?? result.entries.length;
      if (mode == .contents) {
        final hits =
            result.hitCount ??
            result.entries.where((entry) => entry.line != null).length;
        final files = result.entries.map((entry) => entry.id).toSet().length;
        return 'rg · $hits hits in $files files';
      }
      return '${result.partial ? 'At least ' : ''}$count ${count == 1 ? 'match' : 'matches'}';
    }
    return null;
  }

  void update({
    required bool active,
    required bool dedicated,
    required String query,
    bool immediate = false,
    bool refresh = false,
  }) {
    if (_disposed || !available) return;
    this.active = active;
    if (this.dedicated && !dedicated) {
      contents = false;
      filters.clear();
      _directoryId = null;
      _browseOpaqueDirectory = false;
      _includeIgnored = false;
    }
    if (!refresh) {
      _expanded.clear();
      _pickedResult = false;
      if (undoId == null) status = null;
      selectedKey = null;
    }
    this.dedicated = dedicated;
    this.query = query;
    _searchDelay?.cancel();
    final generation = ++_searchGeneration;
    _clearPreview();
    failed = false;
    diagnostic = null;
    if (!active) {
      loading = false;
      _cancelSearch();
      return;
    }
    if (!refresh) result = null;
    loading = true;
    notifyListeners();
    void run() => unawaited(_search(generation));
    if (immediate || query.isEmpty && filters.isEmpty) {
      run();
    } else {
      _searchDelay = Timer(Duration(milliseconds: contents ? 200 : 90), run);
    }
  }

  void _cancelSearch() {
    if (_loadingPreferences == null) return;
    unawaited(
      client!.request('cancel').then<void>((_) {}).catchError((Object _) {}),
    );
  }

  Future<void> _loadPreferences() => _loadingPreferences ??= () async {
    _preferences = await preferences.read();
    showHidden = _preferences['files-search']?['show_hidden'] == true;
    showPreview = _preferences['files-preview']?['show_preview'] != false;
    if (_disposed) return;
    for (final id in FilesPreferences.ids) {
      _preferenceWatches.add(
        preferences.watch(id).listen(
          (reply) {
            final value = reply['value'];
            if (reply['availability'] != 'available' ||
                value is! Map<String, dynamic>) {
              _applyPreferences(id, FilesPreferences.defaults[id]!);
              return;
            }
            try {
              FilesPreferences.validateSection(id, value);
              _applyPreferences(id, value);
            } on FormatException {
              _applyPreferences(id, FilesPreferences.defaults[id]!);
            }
          },
          onError: (Object _) =>
              _applyPreferences(id, FilesPreferences.defaults[id]!),
        ),
      );
    }
  }();

  void _applyPreferences(String id, Map<String, dynamic> value) {
    if (_disposed) return;
    final before = _preferences[id];
    if (before != null &&
        before.length == value.length &&
        value.entries.every((entry) {
          final previous = before[entry.key];
          final current = entry.value;
          return current is List && previous is List
              ? listEquals(current, previous)
              : current == previous;
        })) {
      return;
    }
    _preferences[id] = value;
    if (id == FilesPreferences.searchId) {
      showHidden = value['show_hidden'] == true;
      if (!showHidden) _includeIgnored = false;
      if (!contentsEnabled) contents = false;
    }
    if (id == FilesPreferences.previewId) {
      showPreview = value['show_preview'] != false;
    }
    if (active && id != FilesPreferences.actionsId) {
      update(active: true, dedicated: dedicated, query: query, immediate: true);
    }
  }

  Future<void> _search(int generation) async {
    bool current() => !_disposed && generation == _searchGeneration;
    try {
      await _loadPreferences();
      if (!current()) return;
      void apply(Map<String, dynamic> data) {
        if (!current()) return;
        final before = _pickedResult ? selected?.entry : null;
        final next = FilesSearchResult.fromJson(data);
        if (before != null &&
            next.mode == .names &&
            !next.entries.any((entry) => entry.key == before.key)) {
          next.entries.add(before);
        }
        result = next;
        if (next.mode == .path && next.createName == null) {
          _lastDirectoryId = next.directoryId ?? _lastDirectoryId;
        }
        diagnostic = result?.diagnostic;
        selectedKey = selected?.key;
        _loadPreview();
      }

      final relative = query.startsWith('./') || query.startsWith('../');
      final anchor = contents || _browseOpaqueDirectory
          ? _directoryId
          : relative
          ? _lastDirectoryId
          : null;
      final data = await client!.request(
        'search',
        {
          'query': query,
          'filters': filters,
          if (contents)
            'mode': 'contents'
          else if (_browseOpaqueDirectory)
            'mode': 'path',
          'directoryId': ?anchor,
          if (showHidden !=
              (_preferences['files-search']?['show_hidden'] == true))
            'showHidden': showHidden,
          if (_includeIgnored) 'includeIgnored': true,
          if (!dedicated) ...{'limit': 9, 'compact': true},
          'preferences': _preferences,
        },
        (data) {
          apply(data);
          if (current()) notifyListeners();
        },
      );
      apply(data);
    } on Object catch (error) {
      if (!current()) return;
      failed = true;
      diagnostic = error is FilesException
          ? error.message
          : 'Files search is unavailable.';
    } finally {
      if (current()) {
        loading = false;
        notifyListeners();
      }
    }
  }

  void retry() => update(
    active: active,
    dedicated: dedicated,
    query: query,
    immediate: true,
    refresh: true,
  );

  void select(FileChoice choice) {
    _pickedResult = true;
    if (selectedKey == choice.key) return;
    selectedKey = choice.key;
    status = null;
    _loadPreview();
    notifyListeners();
  }

  void move(int delta, {bool byGroup = false}) {
    final items = choices;
    if (items.isEmpty) return;
    final index = items.indexWhere((choice) => choice.key == selected?.key);
    if (byGroup) {
      final starts =
          [
                for (final group in groups) group.choices.first,
                if (mode == .contents)
                  for (final item in items)
                    if (item.entry != null &&
                        item.entry!.line == null &&
                        item.kind == .entry)
                      item,
              ]
              .map(
                (choice) => items.indexWhere((item) => item.key == choice.key),
              )
              .toSet()
              .toList()
            ..sort();
      final next = delta > 0
          ? starts.where((i) => i > index).firstOrNull
          : starts.reversed.where((i) => i < index).firstOrNull;
      if (next != null) select(items[next]);
      return;
    }
    select(items[(index + delta).clamp(0, items.length - 1)]);
  }

  void expand(FileChoice choice) {
    final id = choice.entry?.id;
    if (id == null) return;
    if (!_expanded.add(id)) _expanded.remove(id);
    final items = choices;
    if (items.every((item) => item.key != selectedKey)) {
      final header = items.where((item) => item.entry?.id == id).firstOrNull;
      if (header != null) {
        select(header);
        return;
      }
    }
    notifyListeners();
  }

  String? acceptFilter(FileChoice? choice) {
    if (choice?.kind != .filter) return null;
    final candidate = _filterCandidates.any((item) => item.key == choice!.key);
    filters.add(choice!.label);
    return candidate
        ? query.replaceFirst(RegExp(r'(?:^|\s)\w+:[^\s"]*$'), '').trimRight()
        : query;
  }

  String? acceptFilterCandidate() {
    final candidates = _filterCandidates;
    final current = selected?.key;
    return acceptFilter(
      candidates.where((item) => item.key == current).firstOrNull ??
          candidates.firstOrNull,
    );
  }

  void browseOpaqueDirectory(FileEntry entry) {
    _directoryId = entry.id;
    _browseOpaqueDirectory = true;
  }

  void clearBrowseDirectory() {
    _browseOpaqueDirectory = false;
    _directoryId = null;
  }

  void toggleContents() {
    if (!contentsEnabled && !contents) return;
    if (!contents && mode == .path) _directoryId = result?.directoryId;
    contents = !contents;
    status = null;
  }

  Future<void> toggleHidden() async {
    await _loadPreferences();
    if (_disposed) return;
    showHidden = !showHidden;
    _includeIgnored = showHidden;
    retry();
    await _persist('files-search', 'show_hidden', showHidden);
  }

  Future<void> togglePreview() async {
    await _loadPreferences();
    if (_disposed) return;
    showPreview = !showPreview;
    _loadPreview();
    notifyListeners();
    await _persist('files-preview', 'show_preview', showPreview);
  }

  Future<void> _persist(String page, String key, bool value) async {
    try {
      await preferences.setValue(page, key, value);
      if (!_disposed) {
        _preferences[page] = {...?_preferences[page], key: value};
      }
    } on Object {
      report('Applied for this session. Could not save the preference.');
    }
  }

  void _clearPreview() {
    _previewDelay?.cancel();
    _previewGeneration++;
    preview = null;
    previewLoading = false;
    _previewRequest = null;
  }

  void _loadPreview() {
    final entry = selected?.entry;
    if (!dedicated || !showPreview || entry == null) {
      _clearPreview();
      return;
    }
    final request = (entry.id, entry.line, entry.version);
    if (_previewRequest == request) return;
    _clearPreview();
    _previewRequest = request;
    final generation = _previewGeneration;
    previewLoading = true;
    _previewDelay = Timer(const Duration(milliseconds: 80), () async {
      try {
        final data = await client!.request('preview', {
          'entryId': entry.id,
          'version': ?entry.version,
          'line': ?entry.line,
          'preferences': _preferences,
        });
        if (_disposed || generation != _previewGeneration) return;
        preview = data;
      } on Object catch (error) {
        if (_disposed || generation != _previewGeneration) return;
        preview = {
          'note': error is FilesException
              ? error.message
              : 'Preview unavailable.',
        };
      }
      previewLoading = false;
      notifyListeners();
    });
  }

  void report(String? message) {
    if (_disposed) return;
    status = message;
    notifyListeners();
  }

  Future<Map<String, dynamic>> request(
    String operation, [
    Map<String, Object?> fields = const {},
  ]) => client!.request(operation, {...fields, 'preferences': _preferences});

  void rememberUndo(String? id, String message) {
    _undoTimer?.cancel();
    undoId = id;
    report(id == null ? message : '$message · Ctrl+Z Undo');
    if (id != null) {
      _undoTimer = Timer(const Duration(seconds: 5), () {
        undoId = null;
        if (status?.endsWith('Ctrl+Z Undo') ?? false) report(null);
      });
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _searchDelay?.cancel();
    _clearPreview();
    _undoTimer?.cancel();
    client?.close();
    for (final watch in _preferenceWatches) {
      unawaited(watch.cancel());
    }
    unawaited(preferences.close());
    super.dispose();
  }
}
