part of 'clipboard_launcher.dart';

enum _CharFilter {
  all('All'),
  emoji('Emoji'),
  symbols('Symbols');

  const _CharFilter(this.label);
  final String label;
}

extension _LauncherEmoji on _ClipboardLauncherState {
  Future<void> _loadEmojiOptions(int session) async {
    try {
      final load = widget.loadEmojiPreferences;
      final preferences = load == null
          ? await EmojiPreferences.read()
          : await load();
      List<String> ids = const [];
      if (preferences.recents) {
        try {
          ids = await _emojiRecents.read();
        } on Object {
          // A local state error should not disable the paste preference.
        }
      }
      if (!mounted || session != _emojiSession) return;
      _rebuild(() => _emojiPreferences = preferences);
      if (ids.isNotEmpty) _showEmojiRecents(ids);
    } on Object {
      // Search stays usable with no Core connection or local state.
    }
  }

  void _showEmojiRecents(List<String> ids) {
    _rebuild(() {
      _recentEmojiIds = ids;
      final catalog = _emojiCatalog;
      if (_inSymbols ||
          _emojiComposing ||
          _search.text.trim().isNotEmpty ||
          _emojiLoading ||
          _emojiUnavailable ||
          catalog == null) {
        return;
      }
      final selected = _emojiSelection == 0 ? null : _selectedEmoji?.emoji;
      _emojiResults = [
        ...catalog.browse(locale: _emojiLocale, recentIds: ids),
        if (_inAllChars) ...SymbolCatalog.browse(),
      ];
      final index = _emojiResults.indexWhere((item) => item.emoji == selected);
      _emojiSelection = index < 0 ? 0 : index;
    });
  }

  bool get _canPasteEmoji =>
      _emojiPreferences.autoPaste && widget.platform.canPasteToPreviousWindow;

  void _setCharFilter(_CharFilter filter) {
    if (_charFilter == filter) return;
    _charFilter = filter;
    _queueEmojiSearch(_search.text);
    _searchFocus.requestFocus();
  }

  void _queueEmojiSearch(String query) {
    _emojiDelay?.cancel();
    final revision = ++_emojiRevision;
    _rebuild(() {
      _status = null;
      _emojiResults = const [];
      _emojiSelection = 0;
      _emojiUnavailable = false;
      _emojiLoading = !_emojiComposing;
    });
    if (_emojiComposing) return;
    _emojiDelay = Timer(
      query.isEmpty ? Duration.zero : const Duration(milliseconds: 65),
      () => _searchEmoji(query, revision),
    );
  }

  Future<void> _searchEmoji(String query, int revision) async {
    if (!mounted || !_inEmoji || revision != _emojiRevision) return;
    final empty = query.trim().isEmpty;
    final symbols = _charFilter == .emoji
        ? const <EmojiResult>[]
        : empty
        ? SymbolCatalog.browse()
        : SymbolCatalog.search(query);
    if (_inSymbols) {
      _rebuild(() {
        _emojiResults = symbols;
        _emojiLoading = false;
      });
      return;
    }
    final load = _emojiLoad ??= widget.loadEmojiCatalog();
    try {
      final catalog = await load;
      if (!mounted || !_inEmoji || revision != _emojiRevision) return;
      final results = empty
          ? catalog.browse(locale: _emojiLocale, recentIds: _recentEmojiIds)
          : catalog.search(query, locale: _emojiLocale);
      _rebuild(() {
        _emojiCatalog = catalog;
        _emojiResults = [...results, ...symbols];
        _emojiLoading = false;
      });
    } on Object {
      if (_emojiLoad == load) _emojiLoad = null;
      if (!mounted || !_inEmoji || revision != _emojiRevision) return;
      _rebuild(() {
        _emojiLoading = false;
        _emojiResults = symbols;
        _emojiUnavailable = symbols.isEmpty;
      });
    }
  }

  EmojiResult? get _selectedEmoji =>
      _emojiResults.isEmpty ? null : _emojiResults[_emojiSelection];

  bool _hasEmojiVariants(EmojiResult item) =>
      item.kind == .emoji && (_emojiCatalog?.hasVariants(item) ?? false);

  Future<void> _copyEmoji(EmojiResult item, {bool paste = false}) async {
    if (_emojiCopying) return;
    final revision = _emojiRevision;
    final session = _emojiSession;
    final remember = item.kind == .emoji && _emojiPreferences.recents;
    _emojiCopying = true;
    final kind = item.kind == .symbol ? 'symbol' : 'emoji';
    String message;
    try {
      final durable = await _writePlainText(item.emoji);
      message = 'Copied. Paste it in the target window.';
      if (remember) {
        try {
          final ids = await _emojiRecents.record(item.codePoints);
          if (mounted && session == _emojiSession) _showEmojiRecents(ids);
        } on Object {
          // Recording recents cannot invalidate a successful copy.
        }
      }
      if (paste &&
          durable &&
          mounted &&
          revision == _emojiRevision &&
          _canPasteEmoji) {
        try {
          if (await widget.platform.pasteToPreviousWindow()) {
            message = 'Pasted into the previous window.';
          }
        } on Object {
          // Retain the copy and manual paste status if the window disappeared.
        }
      }
    } on Object {
      message = 'Could not copy the $kind.';
    } finally {
      _emojiCopying = false;
    }
    if (!mounted || !_inEmoji || revision != _emojiRevision) return;
    _rebuild(() => _status = message);
    _searchFocus.requestFocus();
  }

  Future<void> _chooseEmojiVariant(EmojiResult item) async {
    final variants = _emojiCatalog?.variants(item, locale: _emojiLocale);
    if (variants == null || variants.isEmpty) return;
    final revision = _emojiRevision;
    final result = await showEmojiVariants(
      context,
      variants: variants,
      canPaste: _canPasteEmoji,
    );
    if (!mounted || !_inEmoji || revision != _emojiRevision) return;
    if (result != null) await _copyEmoji(result.item, paste: result.paste);
    if (mounted && _inEmoji && revision == _emojiRevision) {
      _searchFocus.requestFocus();
    }
  }

  KeyEventResult _onEmojiKey(
    KeyEvent event, {
    required bool ctrl,
    required bool shift,
    required bool alt,
  }) {
    final key = event.logicalKey;
    if (ctrl && !alt && key == LogicalKeyboardKey.keyF) {
      if (event is KeyDownEvent) {
        final filters = _CharFilter.values;
        _setCharFilter(
          filters[(_charFilter.index + (shift ? -1 : 1)) % filters.length],
        );
      }
      return .handled;
    }
    final item = _selectedEmoji;
    if (item == null || shift || alt) return .ignored;
    if (ctrl &&
        key == LogicalKeyboardKey.arrowRight &&
        _hasEmojiVariants(item)) {
      if (event is KeyDownEvent) unawaited(_chooseEmojiVariant(item));
      return .handled;
    }
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      if (ctrl && !_canPasteEmoji) return .ignored;
      if (event is KeyDownEvent) unawaited(_copyEmoji(item, paste: ctrl));
      return .handled;
    }
    if (ctrl) return .ignored;
    if (key == LogicalKeyboardKey.arrowDown ||
        key == LogicalKeyboardKey.arrowUp) {
      final step = key == LogicalKeyboardKey.arrowDown ? 1 : -1;
      _rebuild(() {
        _emojiSelection = (_emojiSelection + step).clamp(
          0,
          _emojiResults.length - 1,
        );
        _status = null;
      });
      return .handled;
    }
    return .ignored;
  }

  Widget _emojiBody() => Column(
    mainAxisSize: .min,
    crossAxisAlignment: .stretch,
    spacing: 6,
    children: [
      LauncherFilters(
        counts: {for (final filter in _CharFilter.values) filter.label: null},
        selected: _charFilter.label,
        shortcut: const LauncherKeyHint(['Ctrl', 'F']),
        onSelected: (label) => _setCharFilter(
          _CharFilter.values.firstWhere((filter) => filter.label == label),
        ),
      ),
      Flexible(
        child: EmojiResults(
          results: _emojiResults,
          selectedIndex: _emojiSelection,
          loading: _emojiLoading,
          unavailable: _emojiUnavailable,
          symbols: _inSymbols,
          onSelected: (index) => _rebuild(() => _emojiSelection = index),
          hasVariants: _hasEmojiVariants,
          onCopy: _copyEmoji,
          onVariants: _chooseEmojiVariant,
        ),
      ),
    ],
  );

  LauncherFooter _emojiFooter({required bool singleLine}) {
    final item = _selectedEmoji;
    final version = _inSymbols ? null : _emojiCatalog?.unicodeVersion;
    return LauncherFooter(
      singleLine: singleLine,
      leading: [
        if (item != null) const LauncherKeyHint(['↵'], label: 'Copy'),
        if (item != null && _canPasteEmoji)
          const LauncherKeyHint(['Ctrl', '↵'], label: 'Copy and paste'),
        if (item != null && _hasEmojiVariants(item))
          const LauncherKeyHint(['Ctrl', '→'], label: 'Variants'),
        LauncherKeyHint([
          'Esc',
        ], label: _search.text.isEmpty ? 'Back' : 'Clear'),
      ],
      trailing: [
        if (version != null) LauncherKeyHint(const [], label: 'Emoji $version'),
      ],
    );
  }
}
