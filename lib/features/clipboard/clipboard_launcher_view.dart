part of 'clipboard_launcher.dart';

extension _LauncherView on _ClipboardLauncherState {
  Widget _buildLauncher(BuildContext context) {
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
          _tileColumns = constraints.maxWidth < 640 ? 3 : 6;
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
                      _apps.closeMenu();
                      _web.closeMenu();
                      _closeFilesMenu();
                    },
                    excludeFromSemantics: true,
                    child: ListenableBuilder(
                      listenable: Listenable.merge([
                        _calculator,
                        _apps,
                        _windows,
                        _web,
                        _files,
                      ]),
                      builder: (context, _) => LauncherPanel(
                        width: _inClipboard
                            ? 960
                            : _inFiles && _files.showPreview
                            ? 880
                            : 720,
                        scope: _scope,
                        enabledScopes: _activeScopes.toSet(),
                        onScopeSelected: _setScope,
                        scrollBody:
                            !_inClipboard &&
                            !_inFiles &&
                            !_inApps &&
                            !_inWindows &&
                            !_inEmoji,
                        actionsBodyHeight: _filesMenuEntry != null
                            ? 390
                            : _web.menuChoice != null
                            ? 420
                            : _apps.menuApp == null
                            ? 286
                            : 408,
                        actionsWidth:
                            _web.menuChoice != null || _filesMenuEntry != null
                            ? 344
                            : 300,
                        actionsEnd:
                            _inFiles &&
                                _files.showPreview &&
                                constraints.maxWidth >= 610 &&
                                height >= 510
                            ? 269
                            : 21,
                        onOverlay: true,
                        compact: compact,
                        search: LauncherSearchField(
                          scope: _scope,
                          controller: _search,
                          focusNode: _searchFocus,
                          autofocus: true,
                          compact: compact,
                          queryToken: _inFiles
                              ? _filesQueryTokens(constraints.maxWidth * .36)
                              : _inWeb && _web.engine != null
                              ? WebEngineToken(
                                  _web.engine!.label,
                                  onRemove: () => _setScope(
                                    .web,
                                    initialQuery: _search.text,
                                  ),
                                )
                              : null,
                          completionSuffix:
                              (_inFiles || _inWeb || _scope == .all) &&
                                  _searchFocus.hasFocus &&
                                  _search.selection.isCollapsed &&
                                  _search.selection.extentOffset ==
                                      _search.text.length &&
                                  _search.value.composing.isCollapsed
                              ? (_inFiles
                                    ? _files.completionSuffix
                                    : _web.completionSuffix(_search.text))
                              : null,
                          placeholder: _inFiles
                              ? (_files.contents
                                    ? 'Search inside files…'
                                    : 'Search files by name, or start with / or ~…')
                              : _inWeb
                              ? 'Search history & bookmarks, or type a URL…'
                              : _inClipboard
                              ? 'Filter clipboard…'
                              : _inCalculator
                              ? 'Expression, conversion, or search functions & units'
                              : _inApps
                              ? (_apps.ready
                                    ? 'Filter ${_apps.result.total} apps by name, kind or keyword…'
                                    : 'Filter apps by name, kind or keyword…')
                              : _inWindows
                              ? 'Filter windows by title or class…'
                              : _inEmoji
                              ? (_inSymbols
                                    ? 'Search symbols by name or code point…'
                                    : emojiSearchPlaceholder(
                                        Localizations.localeOf(context),
                                      ))
                              : _inActions
                              ? 'Filter actions…'
                              : 'Search launcher…',
                          onChanged: _searchChanged,
                          trailing: _inFiles && !compact
                              ? _filesSearchHint()
                              : _inWeb
                              ? _webSearchHint(context)
                              : _inApps && _apps.ready
                              ? AppsSearchHint(_apps)
                              : _inWindows && _windows.ready
                              ? Text(
                                  '${_windows.windows.length} windows',
                                  style: Theme.of(context).textTheme.bodySmall,
                                )
                              : _inClipboard && _error == null
                              ? Text(
                                  '${_visibleEntries.length} items',
                                  style: Theme.of(context).textTheme.bodySmall,
                                )
                              : _inCalculator && _calculator.searchHint != null
                              ? Text(
                                  _calculator.searchHint!,
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
                            : _inFiles
                            ? _filesBody(compact: compact)
                            : _inCalculator
                            ? _calculatorBody(compact: compact)
                            : _inApps
                            ? AppsResults(_apps, tileColumns: _tileColumns)
                            : _inWindows
                            ? WindowsResults(_windows)
                            : _inWeb
                            ? _webBody()
                            : _inEmoji
                            ? _emojiBody()
                            : _shellBody(),
                        actions: _filesMenuEntry != null
                            ? _filesMenu(_filesMenuEntry!)
                            : _web.menuChoice != null
                            ? WebActionsMenu(
                                _web,
                                onKey: _webShortcuts.handleMenu,
                                onAction: _webShortcuts.runMenu,
                              )
                            : _calculator.actionsOpen
                            ? _calculatorActionsOverlay()
                            : _apps.menuApp != null
                            ? AppsActionsMenu(_apps)
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

  Widget _shellBody() {
    if (_inActions) {
      if (_error != null) return _unavailable();
      if (_loading) return const LinearProgressIndicator();
      if (!_showsClear) return const _Message('No matching actions.');
      return LauncherResultRow(
        title: const TextSpan(text: 'Clear clipboard history'),
        subtitle: 'Remy\'a Launcher',
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
          AppsResults(
            _apps,
            tileColumns: _tileColumns,
            selectionVisible: !_filesActiveInAll,
          ),
          CalculatorInlineResult(
            controller: _calculator,
            onOpenInCalc: () =>
                _setScope(.calculator, initialQuery: _search.text),
            onCopy: () => unawaited(_copyCalculatorResult()),
          ),
          if (_files.groups.isNotEmpty) _filesBody(compact: false, all: true),
          clipboardRow,
        ],
      );
    }
    if (_scope == .all && _search.text.trim().isEmpty) {
      return Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: [
          AppsResults(
            _apps,
            tileColumns: _tileColumns,
            selectionVisible: !_filesActiveInAll,
          ),
          if (_files.groups.isNotEmpty) _filesBody(compact: false, all: true),
          LauncherResultRow(
            title: const TextSpan(text: 'Calculator'),
            subtitle: 'Math, unit conversions, programmer bases, floats, time, currency',
            leading: const LauncherSymbolTile(.calculate),
            trailing: const LauncherKeyHint(['Ctrl', '5']),
            onPressed: () => _setScope(.calculator),
          ),
          clipboardRow,
          if (_apps.available)
            LauncherResultRow(
              title: const TextSpan(text: 'Windows'),
              subtitle: 'Focus an open window or move it here',
              leading: const LauncherSymbolTile(.window),
              trailing: const LauncherKeyHint(['Ctrl', '7']),
              onPressed: () => _setScope(.windows),
            ),
        ],
      );
    }
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        if (_web.layout.direct != null) _webBody(),
        AppsResults(
          _apps,
          tileColumns: _tileColumns,
          selectionVisible: !_webActiveInAll && !_filesActiveInAll,
        ),
        if (_web.layout.direct == null &&
            (_web.layout.choices.isNotEmpty ||
                (_apps.choices.isEmpty &&
                    _web.result?.enabledProfileCount == 0)))
          _webBody(),
        if (_files.groups.isNotEmpty) _filesBody(compact: false, all: true),
        clipboardRow,
      ],
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
        _rebuild(() => _select(entry));
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
    if ((_status ??
            ((_inFiles || _filesActiveInAll) ? _files.status : null) ??
            ((_inWeb || _webActiveInAll) ? _web.status : null))
        case final status?) {
      return LauncherFooter(
        leading: [LauncherKeyHint(const [], label: status)],
      );
    }
    if (_inFiles || _filesActiveInAll || _filesMenuEntry != null) {
      return _filesFooter(singleLine);
    }
    if (_inWeb || _webActiveInAll) {
      return webFooter(_web, singleLine: singleLine);
    }
    if (_inWindows) return windowsFooter(_windows, singleLine: singleLine);
    if (_inApps || (_scope == .all && _apps.selected != null)) {
      return appsFooter(_apps, singleLine: singleLine);
    }
    if (_inCalculator) return _calculatorFooter(singleLine: singleLine);
    if (_inEmoji) return _emojiFooter(singleLine: singleLine);
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
