part of 'clipboard_launcher.dart';

extension _LauncherEmoji on _ClipboardLauncherState {
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
    final load = _emojiLoad ??= widget.loadEmojiCatalog();
    try {
      final catalog = await load;
      if (!mounted || !_inEmoji || revision != _emojiRevision) return;
      final results = query.trim().isEmpty
          ? catalog.browse(locale: _emojiLocale)
          : catalog.search(query, locale: _emojiLocale);
      _rebuild(() {
        _emojiCatalog = catalog;
        _emojiResults = results;
        _emojiLoading = false;
      });
    } on Object {
      if (_emojiLoad == load) _emojiLoad = null;
      if (!mounted || !_inEmoji || revision != _emojiRevision) return;
      _rebuild(() {
        _emojiLoading = false;
        _emojiUnavailable = true;
      });
    }
  }

  EmojiResult? get _selectedEmoji =>
      _emojiResults.isEmpty ? null : _emojiResults[_emojiSelection];

  bool _hasEmojiVariants(EmojiResult item) =>
      _emojiCatalog?.hasVariants(item) ?? false;

  Future<void> _copyEmoji(EmojiResult item) async {
    if (_emojiCopying) return;
    final revision = _emojiRevision;
    _emojiCopying = true;
    String message;
    try {
      await _writePlainText(item.emoji);
      message = 'Copied. Paste it in the target window.';
    } on Object {
      message = 'Could not copy the emoji.';
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
    final result = await showEmojiVariants(context, variants: variants);
    if (!mounted || !_inEmoji || revision != _emojiRevision) return;
    if (result != null) await _copyEmoji(result);
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
    final item = _selectedEmoji;
    if (item == null || shift || alt) return .ignored;
    final key = event.logicalKey;
    if (ctrl &&
        key == LogicalKeyboardKey.arrowRight &&
        _hasEmojiVariants(item)) {
      if (event is KeyDownEvent) unawaited(_chooseEmojiVariant(item));
      return .handled;
    }
    if (ctrl) return .ignored;
    if (key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter) {
      if (event is KeyDownEvent) unawaited(_copyEmoji(item));
      return .handled;
    }
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

  Widget _emojiBody() => EmojiResults(
    results: _emojiResults,
    selectedIndex: _emojiSelection,
    loading: _emojiLoading,
    unavailable: _emojiUnavailable,
    onSelected: (index) => _rebuild(() => _emojiSelection = index),
    hasVariants: _hasEmojiVariants,
    onCopy: _copyEmoji,
    onVariants: _chooseEmojiVariant,
  );

  LauncherFooter _emojiFooter({required bool singleLine}) {
    final item = _selectedEmoji;
    final version = _emojiCatalog?.unicodeVersion;
    return LauncherFooter(
      singleLine: singleLine,
      leading: [
        if (item != null) const LauncherKeyHint(['↵'], label: 'Copy'),
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
