import 'dart:async';
import 'dart:io' show Platform, Process, ProcessException, pid;

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../apps/apps_client.dart';
import '../apps/apps_results.dart';
import '../apps/apps_search_controller.dart';
import '../calculator/calculator_client.dart';
import '../calculator/calculator_controller.dart';
import '../calculator/calculator_results.dart';
import '../emoji/emoji_recents.dart';
import '../emoji/emoji_results.dart';
import '../emoji/emoji_search.dart';
import '../emoji/emoji_settings.dart';
import '../emoji/symbol_catalog.dart';
import '../files/files_actions_menu.dart';
import '../files/files_client.dart';
import '../files/files_controller.dart';
import '../files/files_models.dart';
import '../files/files_preferences.dart';
import '../files/files_preview.dart';
import '../files/files_query.dart';
import '../files/files_results.dart';
import '../launcher/launcher_controls.dart';
import '../launcher/launcher_details.dart';
import '../launcher/launcher_filters.dart';
import '../launcher/launcher_panel.dart';
import '../launcher/launcher_results.dart';
import '../web/web_client.dart';
import '../web/web_results.dart';
import '../web/web_search_controller.dart';
import '../web/web_shortcuts.dart';
import '../windows/windows_controller.dart';
import '../windows/windows_results.dart';
import 'clipboard_actions.dart';
import 'clipboard_client.dart';
import 'clipboard_color.dart';
import 'clipboard_history.dart';
import 'clipboard_live_preview.dart';
import 'clipboard_platform.dart';

part 'clipboard_launcher_calculator.dart';
part 'clipboard_launcher_dialogs.dart';
part 'clipboard_launcher_emoji.dart';
part 'clipboard_launcher_keyboard.dart';
part 'clipboard_launcher_files.dart';
part 'clipboard_launcher_files_keys.dart';
part 'clipboard_launcher_view.dart';
part 'clipboard_launcher_web.dart';

class ClipboardLauncher extends StatefulWidget {
  const ClipboardLauncher({
    this.initialScope = .all,
    required this.client,
    required this.platform,
    this.listenForWindowRequests = true,
    this.scopeRequest,
    this.calculatorClient,
    this.appsClient,
    this.webClient,
    this.filesClient,
    this.filesPreferences,
    this.loadEmojiCatalog = EmojiCatalog.load,
    this.loadEmojiPreferences,
    this.emojiRecents,
    super.key,
  });

  final LauncherScope initialScope;
  final ClipboardClient client;
  final ClipboardPlatform platform;
  final bool listenForWindowRequests;
  final (LauncherScope, int)? scopeRequest;
  final CalculatorClient? calculatorClient;
  final AppsClient? appsClient;
  final WebClient? webClient;
  final FilesClient? filesClient;
  final FilesPreferences? filesPreferences;
  final Future<EmojiCatalog> Function() loadEmojiCatalog;
  final Future<EmojiPreferences> Function()? loadEmojiPreferences;
  final EmojiRecents? emojiRecents;

  @override
  State<ClipboardLauncher> createState() => _ClipboardLauncherState();
}

enum _ClipboardFilter {
  all('All'),
  text('Text'),
  image('Images'),
  link('Links'),
  files('Files'),
  color('Colors');

  const _ClipboardFilter(this.label);

  final String label;

  bool accepts(ClipboardKind kind) => switch (this) {
    .all => true,
    .text => const {
      ClipboardKind.text,
      ClipboardKind.command,
      ClipboardKind.code,
    }.contains(kind),
    .image => kind == .image,
    .link => kind == .link,
    .files => kind == .files,
    .color => kind == .color,
  };
}

class _ClipboardLauncherState extends State<ClipboardLauncher>
    with WidgetsBindingObserver {
  late final _clipboardActions = ClipboardActions(
    client: widget.client,
    platform: widget.platform,
    onPasted: _finishPaste,
    onCopied: () {
      if (mounted) setState(() => _status = 'Copied to the clipboard.');
    },
    onChanged: () async {
      await _refresh();
      _scrollToSelection();
    },
    editText: _editText,
  );
  late final _calculator = CalculatorController(
    client: widget.calculatorClient,
  );
  late final _apps = AppsSearchController(
    client: widget.appsClient,
    onClose: _dismiss,
    onCopy: _copyPlainText,
    onInfo: _showAppInfo,
    onWorkspace: _askWorkspace,
    onSearchFocus: _searchFocus.requestFocus,
  );
  late final _windows = WindowsController(
    client: widget.appsClient,
    onClose: _dismiss,
  );
  late final _web = WebSearchController(
    client: widget.webClient,
    onClose: _dismiss,
    onSearchFocus: _searchFocus.requestFocus,
    confirmForget: _confirmWebForget,
  );
  late final _webShortcuts = WebShortcuts(
    web: _web,
    search: _search,
    changeSearch: (text, engine) =>
        _setScope(.web, initialQuery: text, webEngine: engine),
    showEngines: () => unawaited(_showWebEngines()),
  );
  late final _files = FilesController(
    client: widget.filesClient,
    preferences: widget.filesPreferences ?? FilesPreferences(),
  );
  bool? _webSelectedInAll;
  bool? _filesSelectedInAll;
  bool _filesShowIndexes = false;
  bool _filesChoosingApp = false;
  bool _filesMenuLoading = false;
  bool _filesSetDefault = false;
  int _filesMenuIndex = 0;
  int _fileOperation = 0;
  final _fileSession = '$pid-${DateTime.now().microsecondsSinceEpoch}';
  FileEntry? _filesMenuEntry;
  Map<String, dynamic> _filesMenuData = const {};
  final _filesMenuQuery = TextEditingController();
  final _filesMenuFocus = FocusNode();
  final _search = TextEditingController();
  final _searchFocus = FocusNode();
  final _actionsFocus = FocusNode();
  final _historyScroll = ScrollController();
  StreamSubscription<void>? _subscription;
  Timer? _watchRetry;
  Timer? _refreshDelay;
  Timer? _queryDelay;
  int _watchGeneration = 0;
  int _clipboardQueryRevision = 0;
  Timer? _emojiDelay;
  int _emojiRevision = 0;
  Future<EmojiCatalog>? _emojiLoad;
  EmojiCatalog? _emojiCatalog;
  List<EmojiResult> _emojiResults = const [];
  int _emojiSelection = 0;
  bool _emojiLoading = false;
  bool _emojiUnavailable = false;
  bool _emojiCopying = false;
  bool _emojiComposing = false;
  _CharFilter _charFilter = .all;
  bool get _inSymbols => _charFilter == .symbols;
  bool get _inAllChars => _charFilter == .all;
  int _emojiSession = 0;
  EmojiPreferences _emojiPreferences = const EmojiPreferences();
  List<String> _recentEmojiIds = const [];
  late final _emojiRecents = widget.emojiRecents ?? EmojiRecents();
  String _editingText = '';
  String _emojiLocale = 'en';
  bool _loading = false;
  bool _refreshQueued = false;
  bool _enabled = true;
  bool _busy = false;
  bool _revealing = false;
  int _revealGeneration = 0;
  String? _error;
  String? _status;
  String? _revealed;
  String? _selectedId;
  List<ClipboardEntry> _entries = const [];
  Set<String>? _queryIds;
  Future<ClipboardDetail>? _detail;
  _ClipboardFilter _filter = .all;
  LauncherScope _scope = .all;
  int _tileColumns = 6;

  bool get _inClipboard => _scope == .clipboard;
  bool get _inActions => _scope == .actions;
  bool get _watchingClipboard => _inClipboard || _inActions;
  bool get _inCalculator => _scope == .calculator;
  bool get _inApps => _scope == .apps;
  bool get _inWindows => _scope == .windows;
  bool get _inFiles => _scope == .files;
  bool get _inWeb => _scope == .web;
  bool get _inEmoji => _scope == .emoji;
  bool get _webActiveInAll =>
      _scope == .all &&
      !_filesActiveInAll &&
      _web.selected != null &&
      (_webSelectedInAll ??
          (_web.layout.direct != null ||
              (_apps.selected == null && !_calculator.hasInlineResult)));

  bool get _filesActiveInAll =>
      _scope == .all &&
      (_filesSelectedInAll ??
          (_files.selected != null &&
              _apps.selected == null &&
              _web.selected == null &&
              !_calculator.hasInlineResult));

  List<LauncherScope> get _activeScopes => [
    .all,
    if (_apps.available) .apps,
    if (_files.available) .files,
    if (_web.available) .web,
    .calculator,
    .clipboard,
    if (_apps.available) .windows,
    .emoji,
  ];

  List<ClipboardEntry> get _visibleEntries => _entries
      .where((entry) => _matchesQuery(entry) && _filter.accepts(entry.kind))
      .toList(growable: false);

  ClipboardEntry? get _selected =>
      _visibleEntries.where((entry) => entry.id == _selectedId).firstOrNull;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _search.addListener(_onSearchSelectionChanged);
    if (widget.initialScope == .all) {
      _apps.update(_scope, '', immediate: true);
      _updateFiles('', immediate: true);
    } else {
      _setScope(widget.initialScope);
    }
    unawaited(_web.loadEngines());
    if (widget.listenForWindowRequests) {
      unawaited(
        widget.platform.listenForScope((scope) {
          if (mounted) _setScope(scope);
        }),
      );
    }
  }

  @override
  void didUpdateWidget(covariant ClipboardLauncher oldWidget) {
    super.didUpdateWidget(oldWidget);
    final request = widget.scopeRequest;
    if (request != null && request != oldWidget.scopeRequest) {
      _setScope(request.$1);
    }
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final locale = emojiLocaleTag(Localizations.localeOf(context));
    if (_emojiLocale == locale) return;
    _emojiLocale = locale;
    if (_inEmoji) _queueEmojiSearch(_search.text);
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    if (widget.listenForWindowRequests) {
      widget.platform.stopListeningForScope();
    }
    _search.removeListener(_onSearchSelectionChanged);
    _stopWatch();
    _queryDelay?.cancel();
    _emojiDelay?.cancel();
    _calculator.dispose();
    _apps.dispose();
    _windows.dispose();
    _web.dispose();
    _files.dispose();
    _filesMenuQuery.dispose();
    _filesMenuFocus.dispose();
    _search.dispose();
    _searchFocus.dispose();
    _actionsFocus.dispose();
    _historyScroll.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != .resumed) _maskSecret();
  }

  void _startWatch() {
    if (!mounted || !_watchingClipboard) return;
    _stopWatch();
    final generation = _watchGeneration;
    void disconnected() {
      if (!mounted || generation != _watchGeneration) return;
      _stopWatch();
      _setUnavailable();
      _watchRetry = Timer(const Duration(seconds: 1), _startWatch);
    }

    _subscription = widget.client.watch().listen(
      (_) {
        if (generation != _watchGeneration) return;
        _refreshDelay?.cancel();
        _refreshDelay = Timer(const Duration(milliseconds: 50), _refresh);
      },
      onError: (_) => disconnected(),
      onDone: disconnected,
    );
    unawaited(_refresh());
  }

  void _stopWatch() {
    _watchGeneration++;
    _clipboardQueryRevision++;
    unawaited(_subscription?.cancel());
    _subscription = null;
    _watchRetry?.cancel();
    _refreshDelay?.cancel();
    _queryDelay?.cancel();
    _loading = false;
    _refreshQueued = false;
  }

  void _setUnavailable([String message = 'Clipboard service is unavailable.']) {
    if (!mounted || !_watchingClipboard) return;
    _maskSecret();
    setState(() => _error = message);
  }

  Future<void> _refresh() async {
    if (!mounted || !_watchingClipboard) return;
    if (_loading) {
      _refreshQueued = true;
      return;
    }
    final generation = _watchGeneration;
    bool isCurrent() => mounted && generation == _watchGeneration;
    final before = _visibleEntries.indexWhere((item) => item.id == _selectedId);
    setState(() => _loading = true);
    try {
      final (entries, enabled) = await (
        widget.client.list(),
        widget.client.isEnabled(),
      ).wait;
      if (!isCurrent()) return;
      setState(() {
        _entries = entries
          ..sort((left, right) {
            if (left.pinned != right.pinned) return left.pinned ? -1 : 1;
            return right.copiedAt.compareTo(left.copiedAt);
          });
        _enabled = enabled;
        _error = null;
        if (_inClipboard) _preserveSelection(before);
      });
      if (_inClipboard && _search.text.trim().isNotEmpty) {
        _queryDelay?.cancel();
        unawaited(_updateQuery(_search.text));
      }
    } on ParallelWaitError<dynamic, dynamic> catch (error) {
      if (!isCurrent()) return;
      final (AsyncError? history, AsyncError? _) = error.errors;
      history == null
          ? _setUnavailable('Remy\'a Core cannot provide the clipboard policy.')
          : _setUnavailable();
    } on Object {
      if (isCurrent()) _setUnavailable();
    } finally {
      if (isCurrent()) {
        setState(() => _loading = false);
        if (_refreshQueued) {
          _refreshQueued = false;
          unawaited(_refresh());
        }
      }
    }
  }

  bool _matchesQuery(ClipboardEntry entry) {
    final query = _inClipboard ? _search.text.trim().toLowerCase() : '';
    return query.isEmpty ||
        (_queryIds?.contains(entry.id) ??
            '${entry.title} ${entry.summary}'.toLowerCase().contains(query));
  }

  int _cursorUtf16() {
    final offset = _search.selection.extentOffset;
    if (offset < 0) return _search.text.length;
    return offset.clamp(0, _search.text.length);
  }

  void _onSearchSelectionChanged() {
    final value = _search.value;
    final composing = !value.composing.isCollapsed;
    final compositionChanged =
        composing != _emojiComposing && value.text == _editingText;
    _emojiComposing = composing;
    _editingText = value.text;
    if (compositionChanged &&
        (_inEmoji ||
            _inFiles ||
            (!composing &&
                _scope == .all &&
                (value.text.startsWith(':') || filesStartsPath(value.text))))) {
      // EditableText only calls onChanged when the text itself changes.
      scheduleMicrotask(() {
        if (mounted &&
            _search.text == value.text &&
            _search.value.composing == value.composing) {
          _searchChanged(value.text);
        }
      });
    }
    if ((_inWeb || _scope == .all) && _web.result?.input.completion != null) {
      setState(() {});
    }
    if (_inFiles && _files.completion != null) setState(() {});
    if (!_inCalculator || _calculator.actionsOpen) return;
    if (_search.text == _calculator.query) {
      _calculator.updateCursor(_cursorUtf16());
    }
  }

  void _setScope(
    LauncherScope scope, {
    String? initialQuery,
    WebEngine? webEngine,
  }) {
    if (_scope == scope && initialQuery == null) return;
    final nextQuery =
        initialQuery ??
        (scope == .apps ||
                scope == .web ||
                scope == .files ||
                ((_scope == .apps || _scope == .web) && scope == .all)
            ? _search.text
            : '');
    _closeFilesMenu(restoreFocus: false);
    final wasWatching = _watchingClipboard;
    final enteringEmoji = !_inEmoji && scope == .emoji;
    if (_scope != scope) _emojiSession++;
    _emojiDelay?.cancel();
    _emojiRevision++;
    _clipboardQueryRevision++;
    _queryDelay?.cancel();
    setState(() {
      _scope = scope;
      if (enteringEmoji) {
        _charFilter = .all;
        _emojiPreferences = const EmojiPreferences();
        _recentEmojiIds = const [];
      }
      _webSelectedInAll = null;
      _filesSelectedInAll = null;
      if (nextQuery.isEmpty) {
        _search.clear();
      } else {
        _search.value = TextEditingValue(
          text: nextQuery,
          selection: TextSelection.collapsed(offset: nextQuery.length),
        );
      }
      _queryIds = null;
      _status = null;
      _filter = .all;
      _select(null);
      if (scope == .clipboard) _preserveSelection(0);
    });
    _searchFocus.requestFocus();
    _calculator.setScope(
      scope,
      query: _search.text,
      cursorUtf16: _cursorUtf16(),
    );
    _apps.update(scope, nextQuery, immediate: true);
    _web.update(scope, nextQuery, engine: webEngine, immediate: true);
    _updateFiles(nextQuery, immediate: true);
    _windows.update(active: _inWindows, query: _search.text);
    if (_inEmoji) _queueEmojiSearch(nextQuery);
    if (enteringEmoji) unawaited(_loadEmojiOptions(_emojiSession));
    if (_watchingClipboard) {
      if (!wasWatching) _startWatch();
    } else if (wasWatching) {
      _stopWatch();
    }
  }

  void _searchChanged(String value) {
    if (_inFiles) {
      _filesSearchChanged(value);
      return;
    }
    if (_files.available && _scope == .all && !_emojiComposing) {
      if (filesStartsPath(value)) {
        _setScope(.files, initialQuery: value);
        return;
      }
      if (value.startsWith('rg ')) {
        _setScope(.files, initialQuery: value);
        _filesSearchChanged(value);
        return;
      }
    }
    if (_inEmoji) {
      _queueEmojiSearch(value);
      return;
    }
    if (_scope == .all && value.startsWith(':') && !_emojiComposing) {
      _setScope(.emoji, initialQuery: value.substring(1));
      return;
    }
    if (_web.available && (_inWeb || _scope == .all)) {
      if (_scope == .all && value == '?') {
        _setScope(.web, initialQuery: '');
        return;
      }
      final engine = _web.engine == null && _search.value.composing.isCollapsed
          ? _web.keyword(value)
          : null;
      if (engine != null) {
        final query = value.substring(value.indexOf(' ') + 1);
        _setScope(.web, initialQuery: query, webEngine: engine);
        return;
      }
    }
    if (_scope == .all) {
      if (value == ';' || value == '>') {
        _setScope(value == ';' ? .clipboard : .actions);
        return;
      }
      if (value == '#' && _apps.available) {
        _setScope(.windows);
        return;
      }
      if (value.startsWith('=')) {
        _setScope(.calculator, initialQuery: value.substring(1));
        return;
      }
    }
    if (_inCalculator && value.startsWith('=')) {
      final stripped = value.substring(1);
      _search.value = TextEditingValue(
        text: stripped,
        selection: TextSelection.collapsed(offset: stripped.length),
      );
      setState(() => _status = null);
      _calculator.updateQuery(stripped, cursorUtf16: stripped.length);
      return;
    }
    setState(() {
      _queryIds = null;
      if (_inCalculator || _inWeb || _scope == .all) _status = null;
      _webSelectedInAll = null;
      _filesSelectedInAll = null;
      _preserveSelection(0);
    });
    _apps.update(_scope, value);
    _web.update(_scope, value, engine: _web.engine);
    if (!_emojiComposing) _updateFiles(value);
    _windows.update(active: _inWindows, query: value);
    if (_inCalculator || _scope == .all) {
      _calculator.updateQuery(_search.text, cursorUtf16: _cursorUtf16());
    }
    if (_inClipboard) {
      _clipboardQueryRevision++;
      _queryDelay?.cancel();
      if (value.trim().isNotEmpty) {
        _queryDelay = Timer(
          const Duration(milliseconds: 75),
          () => _updateQuery(value),
        );
      }
    }
  }

  Future<void> _updateQuery(String value) async {
    final revision = ++_clipboardQueryRevision;
    try {
      final matches = await widget.client.list(query: value.trim());
      if (!mounted || !_inClipboard || revision != _clipboardQueryRevision) {
        return;
      }
      setState(() {
        _queryIds = matches.map((entry) => entry.id).toSet();
        _preserveSelection(0);
      });
    } on Object {
      // The current metadata still supports immediate local search.
    }
  }

  void _setFilter(_ClipboardFilter filter) {
    setState(() {
      _filter = filter;
      _preserveSelection(0);
    });
    _scrollToSelection();
  }

  void _preserveSelection(int previousIndex) {
    final visible = _visibleEntries;
    if (visible.any((entry) => entry.id == _selectedId)) return;
    _select(
      visible.isEmpty
          ? null
          : visible[previousIndex.clamp(0, visible.length - 1)],
    );
  }

  void _select(ClipboardEntry? entry) {
    if (_selectedId == entry?.id) return;
    _maskSecret(rebuild: false);
    _selectedId = entry?.id;
    _detail = entry == null || entry.kind == .hidden
        ? null
        : widget.client.detail(entry.id);
    // The view attaches a FutureBuilder on the next frame.
    _detail?.ignore();
  }

  void _move(int delta) {
    final visible = _visibleEntries;
    if (visible.isEmpty) return;
    final current = visible.indexWhere((entry) => entry.id == _selectedId);
    setState(() {
      _select(visible[(current + delta).clamp(0, visible.length - 1)]);
    });
    _scrollToSelection();
  }

  void _scrollToSelection() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_historyScroll.hasClients) return;
      ClipboardHistory.showEntry(_historyScroll, _visibleEntries, _selectedId);
    });
  }

  Future<void> _reveal(ClipboardEntry entry) async {
    final generation = ++_revealGeneration;
    _revealing = true;
    try {
      final secret = await widget.client.reveal(entry.id);
      if (!mounted ||
          generation != _revealGeneration ||
          _selectedId != entry.id ||
          entry.expiresAt?.isAfter(DateTime.now()) != true) {
        return;
      }
      setState(() => _revealed = secret);
    } on Object {
      if (generation == _revealGeneration) _maskSecret();
    }
  }

  void _maskSecret({bool rebuild = true}) {
    _revealGeneration++;
    _revealing = false;
    if (_revealed == null) return;
    if (rebuild && mounted) {
      setState(() => _revealed = null);
    } else {
      _revealed = null;
    }
  }

  Future<void> _dismiss() async {
    if (!mounted) return;
    _maskSecret();
    await widget.platform.close();
  }

  Future<void> _perform(Future<void> Function() run) async {
    if (_busy) return;
    setState(() {
      _busy = true;
      _status = null;
    });
    try {
      await run();
    } on Object catch (error) {
      if (!mounted) return;
      final message = switch (error) {
        ClipboardServiceException(:final message) => message,
        FormatException(:final message) => message,
        ProcessException(:final message) => message,
        _ => 'Could not complete the clipboard action.',
      };
      setState(() => _status = message);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _finishPaste() async {
    if (!mounted) return;
    _maskSecret();
    final delivered = await widget.platform.pasteToPreviousWindow();
    if (!delivered && mounted) {
      setState(() => _status = 'Copied. Paste it in the target window.');
    }
  }

  // The resident service keeps ownership after the Launcher window closes.
  Future<bool> _writePlainText(String text) async {
    try {
      await widget.client.publishText(text);
      return true;
    } on Object {
      await Clipboard.setData(ClipboardData(text: text));
      return false;
    }
  }

  Future<void> _copyPlainText(String text) async {
    if (text.isEmpty) return;
    await _writePlainText(text);
    if (mounted) {
      setState(() => _status = 'Copied $text to the clipboard.');
    }
  }

  bool get _showsClear =>
      _inActions &&
      _error == null &&
      !_loading &&
      'clear clipboard history'.contains(_search.text.trim().toLowerCase());

  void _rebuild(VoidCallback update) => setState(update);

  @override
  Widget build(BuildContext context) => _buildLauncher(context);
}
