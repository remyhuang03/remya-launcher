import 'dart:async';
import 'dart:io' show ProcessException;

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../calculator/calculator_client.dart';
import '../calculator/calculator_controller.dart';
import '../calculator/calculator_results.dart';
import '../launcher/launcher_controls.dart';
import '../launcher/launcher_details.dart';
import '../launcher/launcher_panel.dart';
import '../launcher/launcher_results.dart';
import 'clipboard_actions.dart';
import 'clipboard_client.dart';
import 'clipboard_color.dart';
import 'clipboard_history.dart';
import 'clipboard_live_preview.dart';
import 'clipboard_platform.dart';
import 'clipboard_preview.dart';

class ClipboardLauncher extends StatefulWidget {
  const ClipboardLauncher({
    required this.startInClipboard,
    required this.client,
    required this.platform,
    this.calculatorClient,
    super.key,
  });

  final bool startInClipboard;
  final ClipboardClient client;
  final ClipboardPlatform platform;
  final CalculatorClient? calculatorClient;

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
  final _search = TextEditingController();
  final _searchFocus = FocusNode();
  final _actionsFocus = FocusNode();
  final _historyScroll = ScrollController();
  StreamSubscription<void>? _subscription;
  Timer? _refreshDelay;
  Timer? _queryDelay;
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

  bool get _inClipboard => _scope == .clipboard;
  bool get _inActions => _scope == .run;
  bool get _inCalculator => _scope == .calculator;

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
    if (widget.startInClipboard) {
      _scope = .clipboard;
      _startWatch();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _search.removeListener(_onSearchSelectionChanged);
    _subscription?.cancel();
    _refreshDelay?.cancel();
    _queryDelay?.cancel();
    _calculator.dispose();
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
    _subscription?.cancel();
    _subscription = widget.client.watch().listen(
      (_) {
        _refreshDelay?.cancel();
        _refreshDelay = Timer(const Duration(milliseconds: 50), _refresh);
      },
      onError: (_) => _setUnavailable(),
      onDone: _setUnavailable,
    );
    unawaited(_refresh());
  }

  void _setUnavailable([String message = 'Clipboard service is unavailable.']) {
    if (!mounted || !(_inClipboard || _inActions)) return;
    _maskSecret();
    setState(() => _error = message);
  }

  Future<void> _refresh() async {
    if (!mounted || !(_inClipboard || _inActions)) return;
    if (_loading) {
      _refreshQueued = true;
      return;
    }
    final before = _visibleEntries.indexWhere((item) => item.id == _selectedId);
    setState(() => _loading = true);
    try {
      final (entries, enabled) = await (
        widget.client.list(),
        widget.client.isEnabled(),
      ).wait;
      if (!mounted || !(_inClipboard || _inActions)) return;
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
        unawaited(_updateQuery(_search.text));
      }
    } on ParallelWaitError<dynamic, dynamic> catch (error) {
      final (AsyncError? history, AsyncError? _) = error.errors;
      history == null
          ? _setUnavailable('Remya Core cannot provide the clipboard policy.')
          : _setUnavailable();
    } on Object {
      _setUnavailable();
    } finally {
      if (mounted) setState(() => _loading = false);
      if (_refreshQueued) {
        _refreshQueued = false;
        unawaited(_refresh());
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
    if (!_inCalculator || _calculator.actionsOpen) return;
    if (_search.text == _calculator.query) {
      _calculator.updateCursor(_cursorUtf16());
    }
  }

  void _setScope(LauncherScope scope, {String initialQuery = ''}) {
    if (_scope == scope && initialQuery.isEmpty) return;
    final wasWatching = _inClipboard || _inActions;
    setState(() {
      _scope = scope;
      if (initialQuery.isEmpty) {
        _search.clear();
      } else {
        _search.value = TextEditingValue(
          text: initialQuery,
          selection: TextSelection.collapsed(offset: initialQuery.length),
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
    if (_inClipboard || _inActions) {
      if (!wasWatching) _startWatch();
    } else {
      _subscription?.cancel();
      _subscription = null;
      _refreshDelay?.cancel();
      _queryDelay?.cancel();
    }
  }

  void _searchChanged(String value) {
    if (_scope == .all) {
      if (value == ';' || value == '>') {
        _setScope(value == ';' ? .clipboard : .run);
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
      if (_inCalculator || _scope == .all) _status = null;
      _preserveSelection(0);
    });
    if (_inCalculator || _scope == .all) {
      _calculator.updateQuery(_search.text, cursorUtf16: _cursorUtf16());
    }
    if (_inClipboard) {
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
    try {
      final matches = await widget.client.list(query: value.trim());
      if (!mounted || !_inClipboard || _search.text != value) return;
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

  KeyEventResult _onKey(FocusNode node, KeyEvent event) {
    final keys = HardwareKeyboard.instance;
    final key = event.logicalKey;
    final ctrl = keys.isControlPressed;
    final shift = keys.isShiftPressed;
    final alt = keys.isAltPressed;

    if (_revealing &&
        (!ctrl ||
            !keys.logicalKeysPressed.contains(LogicalKeyboardKey.space))) {
      _maskSecret();
    }
    if (event is KeyUpEvent) return .ignored;

    if (key == LogicalKeyboardKey.escape && event is KeyDownEvent) {
      if (_calculator.actionsOpen) {
        _closeCalculatorActions();
        return .handled;
      }
      if (_search.text.isNotEmpty) {
        _search.clear();
        _searchChanged('');
      } else if (_scope != .all) {
        _setScope(.all);
      } else {
        unawaited(_dismiss());
      }
      return .handled;
    }
    if (key == LogicalKeyboardKey.backspace &&
        _search.text.isEmpty &&
        _scope != .all &&
        !ctrl &&
        !alt &&
        event is KeyDownEvent) {
      _setScope(.all);
      return .handled;
    }
    if (ctrl && key == LogicalKeyboardKey.digit5 && event is KeyDownEvent) {
      _setScope(.calculator);
      return .handled;
    }
    if (ctrl && key == LogicalKeyboardKey.digit6 && event is KeyDownEvent) {
      _setScope(.clipboard);
      return .handled;
    }
    if (_scope == .all &&
        _calculator.hasInlineResult &&
        event is KeyDownEvent) {
      if (key == LogicalKeyboardKey.tab && !ctrl && !alt && !shift) {
        _setScope(.calculator, initialQuery: _search.text);
        return .handled;
      }
      if (key == LogicalKeyboardKey.enter && !ctrl && !alt && !shift) {
        unawaited(_copyCalculatorResult());
        return .handled;
      }
    }
    if (_inCalculator && event is KeyDownEvent) {
      return _onCalculatorKey(key, ctrl: ctrl, shift: shift, alt: alt);
    }
    if (_inActions && _showsClear && key == LogicalKeyboardKey.enter) {
      if (event is KeyDownEvent) unawaited(_clear());
      return .handled;
    }
    if (!_inClipboard) return .ignored;
    if (key == LogicalKeyboardKey.arrowUp ||
        key == LogicalKeyboardKey.arrowDown) {
      _move(key == LogicalKeyboardKey.arrowUp ? -1 : 1);
      return .handled;
    }
    if (ctrl && key == LogicalKeyboardKey.keyF && event is KeyDownEvent) {
      final filters = _ClipboardFilter.values;
      _setFilter(filters[(_filter.index + (shift ? -1 : 1)) % filters.length]);
      return .handled;
    }

    final entry = _selected;
    if (entry == null || _error != null || _busy) return .ignored;
    if (ctrl && key == LogicalKeyboardKey.space && entry.kind == .hidden) {
      if (event is KeyDownEvent) unawaited(_reveal(entry));
      return .handled;
    }
    final action = _clipboardActions
        .forEntry(entry)
        .where(
          (action) =>
              action.key == key &&
              action.ctrl == ctrl &&
              action.shift == shift &&
              action.alt == alt,
        )
        .firstOrNull;
    if (action == null) return .ignored;
    if (action.key == LogicalKeyboardKey.keyC &&
        _searchFocus.hasFocus &&
        !_search.selection.isCollapsed) {
      return .ignored;
    }
    if (event is KeyDownEvent) unawaited(_perform(action.run));
    return .handled;
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
    _maskSecret();
    final delivered = await widget.platform.pasteToPreviousWindow();
    if (!delivered && mounted) {
      setState(() => _status = 'Copied. Paste it in the target window.');
    }
  }

  Future<void> _clear() => _perform(() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Clear clipboard history?'),
        content: const Text(
          'This removes all Remya Launcher history, including pinned entries.',
        ),
        actions: [
          TextButton(
            autofocus: true,
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Clear history'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    await widget.client.clear();
    await _refresh();
    if (mounted) setState(() => _status = 'Clipboard history cleared.');
  });

  Future<String?> _editText(String initial) async {
    if (!mounted) return null;
    return showDialog<String>(
      context: context,
      builder: (context) => _EditDialog(initial),
    );
  }

  KeyEventResult _onCalculatorKey(
    LogicalKeyboardKey key, {
    required bool ctrl,
    required bool shift,
    required bool alt,
  }) {
    if (ctrl && key == LogicalKeyboardKey.keyK && !alt) {
      if (_calculator.actionsOpen) {
        _closeCalculatorActions();
      } else {
        _openCalculatorActions();
      }
      return .handled;
    }
    if (key == LogicalKeyboardKey.arrowRight &&
        !ctrl &&
        !shift &&
        !alt &&
        _search.selection.isCollapsed &&
        _search.selection.extentOffset == _search.text.length &&
        _calculator.canOpenActions) {
      _openCalculatorActions();
      return .handled;
    }
    if (ctrl && key == LogicalKeyboardKey.keyB && !shift && !alt) {
      _calculator.cycleBitWidth();
      return .handled;
    }
    if (ctrl && shift && key == LogicalKeyboardKey.keyC && !alt) {
      final expr = _calculator.selectedCandidate?.normalized ?? _search.text;
      if (expr.isNotEmpty) unawaited(_copyPlainText(expr));
      return .handled;
    }
    if (alt && !ctrl && !shift) {
      const formatKeys = [
        LogicalKeyboardKey.digit1,
        LogicalKeyboardKey.digit2,
        LogicalKeyboardKey.digit3,
        LogicalKeyboardKey.digit4,
        LogicalKeyboardKey.digit5,
      ];
      final index = formatKeys.indexOf(key);
      final formats = _calculator.selectedCandidate?.formats ?? const [];
      if (index >= 0 && index < formats.length) {
        final format = formats[index];
        _calculator.selectFormat(format.id);
        unawaited(_copyCalculatorResult(formatIdOverride: format.id));
        return .handled;
      }
    }
    if (key == LogicalKeyboardKey.tab && !ctrl && !alt) {
      return _handleCalculatorTab(reverse: shift);
    }
    if ((key == LogicalKeyboardKey.arrowUp ||
            key == LogicalKeyboardKey.arrowDown) &&
        !ctrl &&
        !alt) {
      final delta = key == LogicalKeyboardKey.arrowUp ? -1 : 1;
      if (_search.text.trim().isEmpty &&
          _calculator.historyEntries.isNotEmpty) {
        _calculator.moveHistorySelection(delta);
        return .handled;
      }
      if (_calculator.completions.length > 1) {
        _calculator.moveCompletionSelection(delta);
        return .handled;
      }
      if (_calculator.catalogEntries.length > 1 &&
          _calculator.selectedCandidate == null) {
        _calculator.moveCatalogSelection(delta);
        return .handled;
      }
      if (_calculator.candidates.length > 1) {
        _calculator.cycleCandidate(delta);
        return .handled;
      }
      return .ignored;
    }
    if (key == LogicalKeyboardKey.enter && !ctrl && !alt && !shift) {
      if (_calculator.hasPendingError) {
        _calculator.revealErrorNow();
        return .handled;
      }
      if (_search.text.trim().isEmpty && _calculator.selectedHistory != null) {
        unawaited(_copyPlainText(_calculator.selectedHistory!.copyText));
        return .handled;
      }
      if (_calculator.isCopyableResult) {
        unawaited(_copyCalculatorResult());
        return .handled;
      }
      if (_calculator.selectedCatalogEntry case final entry?) {
        _fillCalculatorQuery(entry.insertText);
        return .handled;
      }
      return .ignored;
    }
    if (shift &&
        key == LogicalKeyboardKey.delete &&
        !ctrl &&
        !alt &&
        _search.text.trim().isEmpty &&
        _calculator.selectedHistory != null) {
      unawaited(
        _calculator.deleteHistory(_calculator.selectedHistory!.historyId),
      );
      return .handled;
    }
    if (ctrl &&
        key == LogicalKeyboardKey.keyP &&
        !shift &&
        !alt &&
        _search.text.trim().isEmpty &&
        _calculator.selectedHistory != null) {
      final entry = _calculator.selectedHistory!;
      unawaited(
        _calculator.setHistoryPinned(entry.historyId, pinned: !entry.isPinned),
      );
      return .handled;
    }
    return .ignored;
  }

  KeyEventResult _handleCalculatorTab({required bool reverse}) {
    if (!reverse) {
      if (_calculator.pendingCorrectionEdit case final correction?) {
        _applyCalculatorEdit(correction);
        return .handled;
      }
      if (_calculator.pendingCompletionEdit case final completion?) {
        _applyCalculatorEdit(completion);
        return .handled;
      }
    }
    if ((_calculator.selectedCandidate?.formats.length ?? 0) > 1) {
      _calculator.cycleFormat(reverse ? -1 : 1);
      return .handled;
    }
    if (_calculator.candidates.length > 1) {
      _calculator.cycleCandidate(reverse ? -1 : 1);
      return .handled;
    }
    if (!reverse &&
        _search.text.trim().isEmpty &&
        _calculator.selectedHistory != null) {
      _fillCalculatorQuery(_calculator.selectedHistory!.query);
      return .handled;
    }
    if (!reverse && _calculator.selectedCatalogEntry != null) {
      _fillCalculatorQuery(_calculator.selectedCatalogEntry!.insertText);
      return .handled;
    }
    return .ignored;
  }

  void _applyCalculatorEdit(CalculatorTextEdit edit) {
    final next = edit.applyTo(_search.value);
    if (next == null) return;
    _search.value = next;
    _searchFocus.requestFocus();
    _calculator.updateQuery(
      next.text,
      cursorUtf16: next.selection.extentOffset,
    );
  }

  void _fillCalculatorQuery(String query) {
    _search.value = TextEditingValue(
      text: query,
      selection: TextSelection.collapsed(offset: query.length),
    );
    _searchFocus.requestFocus();
    _calculator.updateQuery(query, cursorUtf16: query.length);
  }

  Future<void> _copyPlainText(String text) async {
    if (text.isEmpty) return;
    try {
      await widget.client.publishText(text);
    } on Object {
      await Clipboard.setData(ClipboardData(text: text));
    }
    if (mounted) {
      setState(() => _status = 'Copied $text to the clipboard.');
    }
  }

  Future<void> _copyCalculatorResult({String? formatIdOverride}) async {
    final text = await _calculator.commitSelected(
      formatIdOverride: formatIdOverride,
    );
    if (text == null || text.isEmpty) return;
    await _copyPlainText(text);
  }

  void _openCalculatorActions() {
    if (!_calculator.canOpenActions) return;
    _calculator.openActions();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted && _calculator.actionsOpen) _actionsFocus.requestFocus();
    });
  }

  void _closeCalculatorActions() {
    _calculator.closeActions();
    _searchFocus.requestFocus();
  }

  List<CalculatorMenuAction> _currentCalculatorActions() =>
      _calculator.buildActions(
        onCopyText: _copyPlainText,
        onCommitAndCopy: _copyCalculatorResult,
        onEditQuery: _fillCalculatorQuery,
        onSaveVariable: _promptSaveVariable,
        onClearHistory: _confirmClearCalculatorHistory,
      );

  Future<void> _promptSaveVariable(String expression) async {
    if (!mounted) return;
    final name = await showDialog<String>(
      context: context,
      builder: (context) => _SaveVariableDialog(expression),
    );
    if (name == null || name.isEmpty || !mounted) return;
    final error = await _calculator.saveVariable(name, expression);
    if (!mounted) return;
    setState(() {
      _status = error ?? 'Saved variable $name.';
    });
    _searchFocus.requestFocus();
  }

  Future<void> _confirmClearCalculatorHistory() async {
    if (!mounted) return;
    final hasPinned = _calculator.historyEntries.any((e) => e.isPinned);
    var includePinned = false;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: const Text('Clear calculator history?'),
          content: Column(
            mainAxisSize: .min,
            crossAxisAlignment: .start,
            spacing: 8,
            children: [
              Text(
                hasPinned
                    ? 'Unpinned calculations will be removed.'
                    : 'All calculator history will be removed.',
              ),
              if (hasPinned)
                CheckboxListTile(
                  value: includePinned,
                  contentPadding: .zero,
                  controlAffinity: .leading,
                  title: const Text('Include pinned items'),
                  onChanged: (value) =>
                      setDialogState(() => includePinned = value ?? false),
                ),
            ],
          ),
          actions: [
            TextButton(
              autofocus: true,
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Clear history'),
            ),
          ],
        ),
      ),
    );
    if (confirmed != true || !mounted) return;
    await _calculator.clearHistory(includePinned: includePinned);
    if (!mounted) return;
    setState(() => _status = 'Calculator history cleared.');
    _searchFocus.requestFocus();
  }

  Future<void> _runCalculatorAction(CalculatorMenuAction action) async {
    _closeCalculatorActions();
    await action.run();
  }

  KeyEventResult _onActionsKey(FocusNode node, KeyEvent event) {
    if (event is KeyUpEvent) return .ignored;
    if (event is! KeyDownEvent) return .ignored;
    final keys = HardwareKeyboard.instance;
    final key = event.logicalKey;
    final ctrl = keys.isControlPressed;
    final shift = keys.isShiftPressed;
    final alt = keys.isAltPressed;

    if (key == LogicalKeyboardKey.escape ||
        (ctrl && key == LogicalKeyboardKey.keyK)) {
      _closeCalculatorActions();
      return .handled;
    }
    final actions = _currentCalculatorActions();
    if (key == LogicalKeyboardKey.arrowUp ||
        (key == LogicalKeyboardKey.tab && shift)) {
      _calculator.moveActionSelection(-1, actions.length);
      return .handled;
    }
    if (key == LogicalKeyboardKey.arrowDown ||
        (key == LogicalKeyboardKey.tab && !shift)) {
      _calculator.moveActionSelection(1, actions.length);
      return .handled;
    }
    if (key == LogicalKeyboardKey.enter && actions.isNotEmpty) {
      final index = _calculator.actionIndex.clamp(0, actions.length - 1);
      unawaited(_runCalculatorAction(actions[index]));
      return .handled;
    }
    if (key == LogicalKeyboardKey.backspace) {
      final current = _calculator.actionFilter;
      if (current.isNotEmpty) {
        _calculator.updateActionFilter(
          current.substring(0, current.length - 1),
        );
      }
      return .handled;
    }
    final char = event.character;
    if (char != null &&
        char.isNotEmpty &&
        !ctrl &&
        !alt &&
        char.codeUnitAt(0) >= 32) {
      _calculator.updateActionFilter('${_calculator.actionFilter}$char');
      return .handled;
    }
    return .handled;
  }

  bool get _showsClear =>
      _inActions &&
      _error == null &&
      !_loading &&
      'clear clipboard history'.contains(_search.text.trim().toLowerCase());

  @override
  Widget build(BuildContext context) {
    final selected = _selected;
    final actions = selected == null || _error != null
        ? const <ClipboardAction>[]
        : _clipboardActions.forEntry(selected);
    return Focus(
      onKeyEvent: _onKey,
      onFocusChange: (focused) {
        if (!focused) _maskSecret();
      },
      child: LayoutBuilder(
        builder: (context, constraints) {
          final height = constraints.maxHeight;
          final compact = constraints.maxWidth < 360;
          final inset = compact ? 8.0 : 16.0;
          final top = compact || height < 420 ? inset : 70.0;
          return Stack(
            children: [
              Positioned.fill(
                child: GestureDetector(
                  behavior: .opaque,
                  onTap: _dismiss,
                  excludeFromSemantics: true,
                ),
              ),
              Align(
                alignment: .topCenter,
                child: Padding(
                  padding: .fromLTRB(inset, top, inset, inset),
                  child: GestureDetector(
                    behavior: .opaque,
                    onTap: () {
                      if (_calculator.actionsOpen) _closeCalculatorActions();
                    },
                    excludeFromSemantics: true,
                    child: ListenableBuilder(
                      listenable: _calculator,
                      builder: (context, _) => LauncherPanel(
                        width: _inClipboard ? 960 : 720,
                        scope: _scope,
                        enabledScopes: const {.all, .calculator, .clipboard},
                        onScopeSelected: _setScope,
                        scrollBody: !_inClipboard,
                        onOverlay: true,
                        compact: compact,
                        search: LauncherSearchField(
                          scope: _scope,
                          controller: _search,
                          focusNode: _searchFocus,
                          autofocus: true,
                          compact: compact,
                          placeholder: _inClipboard
                              ? 'Filter clipboard…'
                              : _inCalculator
                              ? 'Calculate, convert units, bases, time, currency…'
                              : _inActions
                              ? 'Filter actions…'
                              : 'Search launcher…',
                          onChanged: _searchChanged,
                          trailing: _inClipboard && _error == null
                              ? Text(
                                  '${_visibleEntries.length} items',
                                  style: Theme.of(context).textTheme.bodySmall,
                                )
                              : _inCalculator
                              ? Text(
                                  _calculator.settings.shortLabel,
                                  style: Theme.of(context).textTheme.bodySmall,
                                )
                              : null,
                        ),
                        body: _inClipboard
                            ? _clipboardBody(
                                selected,
                                actions,
                                compact: compact,
                              )
                            : _inCalculator
                            ? _calculatorBody(compact: compact)
                            : _shellBody(),
                        actions: _calculator.actionsOpen
                            ? _calculatorActionsOverlay()
                            : null,
                        footer: _footer(
                          actions,
                          selected,
                          singleLine:
                              constraints.maxWidth < 540 || height < 520,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
  }

  Widget _calculatorBody({required bool compact}) => CalculatorResults(
    controller: _calculator,
    compact: compact,
    onCopyResult: () => unawaited(_copyCalculatorResult()),
    onCopyText: (text) => unawaited(_copyPlainText(text)),
    onApplyEdit: _applyCalculatorEdit,
    onFillQuery: _fillCalculatorQuery,
    onClearHistory: () => unawaited(_confirmClearCalculatorHistory()),
  );

  Widget _calculatorActionsOverlay() {
    final actions = _currentCalculatorActions();
    return Focus(
      focusNode: _actionsFocus,
      onKeyEvent: _onActionsKey,
      child: LauncherActionsMenu(
        title: _calculator.actionFilter.isEmpty
            ? 'Calculator actions'
            : 'Filter: ${_calculator.actionFilter}',
        actions: [
          for (final action in actions)
            LauncherAction(
              action.label,
              action.icon,
              action.keys,
              destructive: action.destructive,
              onPressed: () => unawaited(_runCalculatorAction(action)),
            ),
        ],
        selectedIndex: actions.isEmpty
            ? 0
            : _calculator.actionIndex.clamp(0, actions.length - 1),
      ),
    );
  }

  Widget _shellBody() {
    if (_inActions) {
      if (_error != null) return _unavailable();
      if (_loading) return const LinearProgressIndicator();
      if (!_showsClear) return const _Message('No matching actions.');
      return LauncherResultRow(
        title: const TextSpan(text: 'Clear clipboard history'),
        subtitle: 'Remya Launcher',
        leading: const LauncherSymbolTile(.trash, selected: true),
        trailing: const LauncherKeyHint(['↵']),
        selected: true,
        onPressed: _busy ? null : _clear,
      );
    }
    final clipboardRow = LauncherResultRow(
      title: const TextSpan(text: 'Clipboard history'),
      subtitle: 'Search and paste recently copied items',
      leading: const LauncherSymbolTile(.clipboard),
      trailing: const LauncherKeyHint(['Ctrl', '6']),
      onPressed: () => _setScope(.clipboard),
    );
    if (_scope == .all && _calculator.hasInlineResult) {
      return Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        spacing: 8,
        children: [
          CalculatorInlineResult(
            controller: _calculator,
            onOpenInCalc: () =>
                _setScope(.calculator, initialQuery: _search.text),
            onCopy: () => unawaited(_copyCalculatorResult()),
          ),
          clipboardRow,
        ],
      );
    }
    if (_scope == .all && _search.text.trim().isEmpty) {
      return Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: [
          LauncherResultRow(
            title: const TextSpan(text: 'Calculator'),
            subtitle: 'Math, unit conversions, programmer bases, floats, time, currency',
            leading: const LauncherSymbolTile(.calculate),
            trailing: const LauncherKeyHint(['Ctrl', '5']),
            onPressed: () => _setScope(.calculator),
          ),
          clipboardRow,
        ],
      );
    }
    return clipboardRow;
  }

  Widget _clipboardBody(
    ClipboardEntry? selected,
    List<ClipboardAction> actions, {
    required bool compact,
  }) {
    if (_error != null) return _unavailable();
    if (_loading && _entries.isEmpty) {
      return const LinearProgressIndicator(
        semanticsLabel: 'Loading clipboard history',
      );
    }
    final visible = _visibleEntries;
    final counts = <String, int>{
      for (final filter in _ClipboardFilter.values)
        filter.label: _entries
            .where(
              (entry) => _matchesQuery(entry) && filter.accepts(entry.kind),
            )
            .length,
    };
    final filters = ClipboardFilters(
      counts: counts,
      selected: _filter.label,
      onSelected: (label) => _setFilter(
        _ClipboardFilter.values.firstWhere((filter) => filter.label == label),
      ),
    );
    if (visible.isEmpty) {
      return Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: [
          filters,
          _Message(
            _entries.isEmpty
                ? _enabled
                      ? 'Clipboard history is empty.'
                      : 'Clipboard history is paused.'
                : 'No matching clipboard entries.',
          ),
        ],
      );
    }

    return ClipboardHistory(
      entries: visible,
      selectedId: _selectedId,
      client: widget.client,
      scroll: _historyScroll,
      filters: filters,
      paused: !_enabled,
      compact: compact,
      preview: selected == null
          ? const SizedBox.shrink()
          : _preview(selected, actions, compact: compact),
      onSelected: (entry) {
        setState(() => _select(entry));
        _searchFocus.requestFocus();
      },
      onPaste: _busy
          ? null
          : (entry) => _perform(() => _clipboardActions.paste(entry)),
    );
  }

  Widget _preview(
    ClipboardEntry selected,
    List<ClipboardAction> actions, {
    required bool compact,
  }) {
    final hints = [
      for (final action in actions)
        if (action.key != LogicalKeyboardKey.keyD && !action.alt) action.hint,
      if (selected.kind == .color && clipboardColor(selected.color) != null)
        const LauncherKeyHint(['Alt', '1–4'], label: 'Paste in that format'),
    ];
    return DecoratedBox(
      decoration: BoxDecoration(
        borderRadius: .circular(20),
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
      ),
      child: SingleChildScrollView(
        key: ValueKey(selected.id),
        child: FutureBuilder<ClipboardDetail>(
          future: _detail,
          builder: (context, snapshot) => snapshot.hasError
              ? const _Message('Preview unavailable.')
              : ClipboardLivePreview(
                  entry: selected,
                  compact: compact,
                  detail: snapshot.connectionState == .done
                      ? snapshot.data
                      : null,
                  hints: hints,
                  revealed: _revealed,
                  onColor: _busy
                      ? null
                      : (value) =>
                            _perform(() => _clipboardActions.pasteText(value)),
                ),
        ),
      ),
    );
  }

  Widget _unavailable() => Wrap(
    crossAxisAlignment: .center,
    children: [
      _Message(_error!),
      TextButton(
        onPressed: _loading ? null : _startWatch,
        child: const Text('Retry'),
      ),
    ],
  );

  LauncherFooter _footer(
    List<ClipboardAction> actions,
    ClipboardEntry? selected, {
    required bool singleLine,
  }) {
    if (_status case final status?) {
      return LauncherFooter(
        leading: [LauncherKeyHint(const [], label: status)],
      );
    }
    if (_inCalculator) {
      if (_calculator.actionsOpen) {
        return LauncherFooter(
          singleLine: singleLine,
          leading: const [
            LauncherKeyHint(['↵'], label: 'Run action'),
            LauncherKeyHint(['Esc'], label: 'Back'),
          ],
          trailing: const [
            LauncherKeyHint(['↑', '↓'], label: 'Navigate'),
          ],
        );
      }
      if (_search.text.trim().isEmpty) {
        final selectedHistory = _calculator.selectedHistory;
        return LauncherFooter(
          singleLine: singleLine,
          leading: [
            if (selectedHistory != null) ...[
              const LauncherKeyHint(['↵'], label: 'Copy result'),
              const LauncherKeyHint(['Tab'], label: 'Edit query'),
              LauncherKeyHint(const [
                'Ctrl',
                'P',
              ], label: selectedHistory.isPinned ? 'Unpin' : 'Pin'),
              const LauncherKeyHint(['⇧', 'Del'], label: 'Delete'),
            ] else
              const LauncherKeyHint(['Esc'], label: 'Close'),
          ],
          trailing: [
            if (_calculator.historyEntries.isNotEmpty)
              const LauncherKeyHint(['↑', '↓'], label: 'History'),
            if (_calculator.canOpenActions)
              const LauncherKeyHint(['Ctrl', 'K'], label: 'Actions'),
          ],
        );
      }
      final formatCount = _calculator.selectedCandidate?.formats.length ?? 0;
      return LauncherFooter(
        singleLine: singleLine,
        leading: [
          if (_calculator.hasPendingCompletionOrCorrection)
            const LauncherKeyHint(['Tab'], label: 'Apply edit')
          else if (formatCount > 1)
            const LauncherKeyHint(['Tab'], label: 'Next format'),
          if (_calculator.isCopyableResult)
            const LauncherKeyHint(['↵'], label: 'Copy'),
          if (_calculator.selectedCandidate?.kind == 'programmer')
            const LauncherKeyHint(['Ctrl', 'B'], label: 'Bit width'),
        ],
        trailing: [
          if (_calculator.canOpenActions)
            const LauncherKeyHint(['Ctrl', 'K'], label: 'Actions'),
        ],
      );
    }
    return LauncherFooter(
      singleLine: singleLine,
      leading: [
        if (_inClipboard)
          for (final action in actions)
            if (const [
              LogicalKeyboardKey.enter,
              LogicalKeyboardKey.keyD,
              LogicalKeyboardKey.delete,
            ].contains(action.key))
              action.hint,
        if (_scope == .all && _calculator.hasInlineResult) ...const [
          LauncherKeyHint(['Tab'], label: 'Open in Calc'),
          LauncherKeyHint(['↵'], label: 'Copy'),
        ],
        if (!_inClipboard) const LauncherKeyHint(['Esc'], label: 'Close'),
      ],
      trailing: [
        if (selected?.kind == .hidden && _error == null)
          const LauncherKeyHint(['Ctrl', 'Space'], label: 'Reveal hidden'),
      ],
    );
  }
}

class _Message extends StatelessWidget {
  const _Message(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const .symmetric(horizontal: 12, vertical: 12),
    child: Text(text, style: Theme.of(context).textTheme.bodySmall),
  );
}

class _EditDialog extends StatefulWidget {
  const _EditDialog(this.initial);

  final String initial;

  @override
  State<_EditDialog> createState() => _EditDialogState();
}

class _EditDialogState extends State<_EditDialog> {
  late final _editor = TextEditingController(text: widget.initial);

  @override
  void dispose() {
    _editor.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Edit before pasting'),
    content: SizedBox(
      width: 480,
      child: TextField(
        autofocus: true,
        controller: _editor,
        minLines: 3,
        maxLines: 10,
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, _editor.text),
        child: const Text('Paste'),
      ),
    ],
  );
}

class _SaveVariableDialog extends StatefulWidget {
  const _SaveVariableDialog(this.expression);

  final String expression;

  @override
  State<_SaveVariableDialog> createState() => _SaveVariableDialogState();
}

class _SaveVariableDialogState extends State<_SaveVariableDialog> {
  final _name = TextEditingController();

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  void _submit() {
    final trimmed = _name.text.trim();
    if (trimmed.isEmpty) return;
    Navigator.pop(context, trimmed);
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Save variable'),
    content: SizedBox(
      width: 360,
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        spacing: 12,
        children: [
          Text(
            'Expression: ${widget.expression}',
            style: Theme.of(context).textTheme.bodySmall,
          ),
          TextField(
            autofocus: true,
            controller: _name,
            decoration: const InputDecoration(
              labelText: 'Variable name',
              hintText: 'e.g. tax_rate',
            ),
            onSubmitted: (_) => _submit(),
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(onPressed: _submit, child: const Text('Save')),
    ],
  );
}
