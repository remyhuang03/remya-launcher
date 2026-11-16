part of 'clipboard_launcher.dart';

extension _CalculatorLauncher on _ClipboardLauncherState {
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
    if (ctrl &&
        !shift &&
        !alt &&
        key == LogicalKeyboardKey.keyR &&
        (_calculator.selectedCandidate?.kind == 'currency' ||
            _calculator.hasRateDiagnostic)) {
      unawaited(_calculator.refreshRates());
      return .handled;
    }
    if (ctrl &&
        !shift &&
        !alt &&
        key == LogicalKeyboardKey.keyI &&
        _calculator.candidates.length > 1) {
      _calculator.cycleCandidate(1);
      return .handled;
    }
    if (ctrl &&
        !alt &&
        (shift && key == LogicalKeyboardKey.keyC ||
            !shift && key == LogicalKeyboardKey.enter)) {
      final expr = _calculator.selectedCandidate?.normalized ?? _search.text;
      if (expr.isNotEmpty) unawaited(_copyPlainText(expr));
      return .handled;
    }
    if (alt &&
        !ctrl &&
        !shift &&
        key == LogicalKeyboardKey.enter &&
        _calculator.isCopyableResult) {
      unawaited(_pasteCalculatorResult());
      return .handled;
    }
    if (ctrl &&
        !alt &&
        !shift &&
        key == LogicalKeyboardKey.keyS &&
        _calculator.isCopyableResult) {
      unawaited(_promptSaveVariable(_search.text));
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
      final candidate = _calculator.selectedCandidate;
      if (index >= 0 &&
          _calculator.completions.isNotEmpty &&
          index < _calculator.completions.length) {
        if (_calculator.completions[index].edit case final edit?) {
          _applyCalculatorEdit(edit);
          return .handled;
        }
      }
      if (index >= 0 &&
          _calculator.candidates.length > 1 &&
          index < _calculator.candidates.length) {
        _calculator.selectCandidate(_calculator.candidates[index].id);
        return .handled;
      }
      final formats = candidate?.numberedFormats ?? const [];
      final format = index >= 0 && index < formats.length
          ? formats[index]
          : null;
      if (format != null) {
        _calculator.selectFormat(format.id);
        unawaited(
          _copyCalculatorResult(
            formatIdOverride: format.id,
            copyFloatBits: true,
          ),
        );
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
      if (_calculator.shouldShowError && _calculator.suggestions.isNotEmpty) {
        _calculator.moveSuggestion(delta);
        return .handled;
      }
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
      if (_calculator.shouldShowError && _calculator.suggestions.isNotEmpty) {
        final suggestion =
            _calculator.suggestions[_calculator.selectedSuggestionIndex];
        if (suggestion.replacement case final edit?) {
          _applyCalculatorEdit(edit);
        } else {
          _fillCalculatorQuery(suggestion.label);
        }
        return .handled;
      }
      if (_calculator.selectedCompletion?.edit case final edit?) {
        _applyCalculatorEdit(edit);
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
        if (entry.definition case final definition?) {
          unawaited(_copyPlainText(definition));
        } else {
          _fillCalculatorQuery(entry.insertText);
        }
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
        key == LogicalKeyboardKey.keyD &&
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

  Future<void> _pasteCalculatorResult() async {
    final result = await _calculator.commitSelected();
    if (result == null || result.isEmpty || !mounted) return;
    await _perform(() => _clipboardActions.pasteText(result));
  }

  Future<void> _copyCalculatorResult({
    String? formatIdOverride,
    bool copyFloatBits = false,
  }) async {
    final text = await _calculator.commitSelected(
      formatIdOverride: formatIdOverride,
      copyFloatBits: copyFloatBits,
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
        onPasteResult: _pasteCalculatorResult,
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
    _rebuild(() {
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
    _rebuild(() => _status = 'Calculator history cleared.');
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
            ? (_calculator.selectedCandidate?.primaryDisplay(
                    _calculator.selectedFormatId,
                  ) ??
                  'Calculator actions')
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

  LauncherFooter _calculatorFooter({required bool singleLine}) {
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
              'D',
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
    if (_calculator.shouldShowError && _calculator.suggestions.isNotEmpty) {
      return LauncherFooter(
        singleLine: singleLine,
        leading: const [
          LauncherKeyHint(['↵'], label: 'Use suggestion'),
          LauncherKeyHint(['↑', '↓'], label: 'Choose'),
        ],
        trailing: const [
          LauncherKeyHint(['Esc'], label: 'Keep editing'),
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
        if (_calculator.selectedCandidate?.kind == 'programmer') ...const [
          LauncherKeyHint(['Alt', '1–4'], label: 'Copy format'),
          LauncherKeyHint(['Ctrl', 'B'], label: 'Bit width'),
        ],
        if (_calculator.selectedCandidate?.kind == 'float_format')
          const LauncherKeyHint(['Alt', '1–3'], label: 'Copy bits'),
        if (_calculator.selectedCandidate?.kind == 'currency')
          const LauncherKeyHint(['Ctrl', 'R'], label: 'Refresh rates'),
        if (_calculator.candidates.length > 1)
          const LauncherKeyHint(['Ctrl', 'I'], label: 'Next reading'),
        if (_calculator.selectedCatalogEntry != null)
          const LauncherKeyHint(['Tab'], label: 'Insert symbol'),
      ],
      trailing: [
        if (_calculator.canOpenActions)
          const LauncherKeyHint(['Ctrl', 'K'], label: 'Actions'),
      ],
    );
  }
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
