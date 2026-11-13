import 'dart:async';
import 'dart:io' show ProcessException;

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
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
    super.key,
  });

  final bool startInClipboard;
  final ClipboardClient client;
  final ClipboardPlatform platform;

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
  final _search = TextEditingController();
  final _searchFocus = FocusNode();
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

  List<ClipboardEntry> get _visibleEntries => _entries
      .where((entry) => _matchesQuery(entry) && _filter.accepts(entry.kind))
      .toList(growable: false);

  ClipboardEntry? get _selected =>
      _visibleEntries.where((entry) => entry.id == _selectedId).firstOrNull;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    if (widget.startInClipboard) {
      _scope = .clipboard;
      _startWatch();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _subscription?.cancel();
    _refreshDelay?.cancel();
    _queryDelay?.cancel();
    _search.dispose();
    _searchFocus.dispose();
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

  void _setScope(LauncherScope scope) {
    if (_scope == scope) return;
    final wasWatching = _inClipboard || _inActions;
    setState(() {
      _scope = scope;
      _search.clear();
      _queryIds = null;
      _status = null;
      _filter = .all;
      _select(null);
      if (scope == .clipboard) _preserveSelection(0);
    });
    _searchFocus.requestFocus();
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
    }
    setState(() {
      _queryIds = null;
      _preserveSelection(0);
    });
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
    if (ctrl && key == LogicalKeyboardKey.digit6 && event is KeyDownEvent) {
      _setScope(.clipboard);
      return .handled;
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
          final top = height >= 940 ? height * .22 : inset;
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
                    onTap: () {},
                    excludeFromSemantics: true,
                    child: LauncherPanel(
                      width: _inClipboard ? 960 : 720,
                      scope: _scope,
                      enabledScopes: const {.all, .clipboard},
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
                            : _inActions
                            ? 'Filter actions…'
                            : 'Search launcher…',
                        onChanged: _searchChanged,
                        trailing: _inClipboard && _error == null
                            ? Text(
                                '${_visibleEntries.length} items',
                                style: Theme.of(context).textTheme.bodySmall,
                              )
                            : null,
                      ),
                      body: _inClipboard
                          ? _clipboardBody(selected, actions, compact: compact)
                          : _shellBody(),
                      footer: _footer(
                        actions,
                        selected,
                        singleLine: constraints.maxWidth < 540 || height < 520,
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
    return LauncherResultRow(
      title: const TextSpan(text: 'Clipboard history'),
      subtitle: 'Search and paste recently copied items',
      leading: const LauncherSymbolTile(.clipboard),
      trailing: const LauncherKeyHint(['Ctrl', '6']),
      onPressed: () => _setScope(.clipboard),
    );
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
