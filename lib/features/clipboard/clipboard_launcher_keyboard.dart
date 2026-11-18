part of 'clipboard_launcher.dart';

extension _LauncherKeyboard on _ClipboardLauncherState {
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
    if (_web.menuChoice != null) return .ignored;
    if (_emojiComposing &&
        (_inEmoji || (_scope == .all && _search.text.startsWith(':')))) {
      return .ignored;
    }
    if (ctrl && !alt && event is KeyDownEvent) {
      if (key == LogicalKeyboardKey.digit3 && _files.available) {
        _setScope(.files, initialQuery: _search.text);
        return .handled;
      }
      if (key == LogicalKeyboardKey.keyG &&
          _scope == .all &&
          _files.available) {
        _setScope(.files, initialQuery: _search.text);
        _switchFilesContents();
        return .handled;
      }
      if (key == LogicalKeyboardKey.digit4 && _web.available) {
        _setScope(.web);
        return .handled;
      }
      if (key == LogicalKeyboardKey.digit2 && _apps.available) {
        _setScope(.apps);
        return .handled;
      }
      if (key == LogicalKeyboardKey.digit7 && _apps.available) {
        _setScope(.windows);
        return .handled;
      }
      if (key == LogicalKeyboardKey.tab) {
        final scopes = _activeScopes;
        final index = scopes.indexOf(_scope);
        _setScope(scopes[(index + (shift ? -1 : 1)) % scopes.length]);
        return .handled;
      }
    }
    if (_emojiComposing && _inFiles) return .ignored;
    if (_inFiles || _filesMenuEntry != null) {
      final handled = _onFilesKey(event, ctrl: ctrl, alt: alt, shift: shift);
      if (handled == .handled || _filesMenuEntry != null) return handled;
    } else if (_scope == .all) {
      final handled = _onAllFilesKey(event, ctrl: ctrl, alt: alt, shift: shift);
      if (handled == .handled) return handled;
    }
    if (_inWeb) {
      final handled = _webShortcuts.handle(event, compact: false);
      if (handled == .handled) return handled;
    }
    if (_scope == .all &&
        event is! KeyUpEvent &&
        _apps.menuApp == null &&
        _web.selected != null) {
      final towardApps = _web.layout.direct != null
          ? LogicalKeyboardKey.arrowDown
          : LogicalKeyboardKey.arrowUp;
      final towardWeb = towardApps == LogicalKeyboardKey.arrowDown
          ? LogicalKeyboardKey.arrowUp
          : LogicalKeyboardKey.arrowDown;
      if (!ctrl && !alt && !shift && _apps.selected != null) {
        final apps = _apps.choices;
        final edgeApp = towardApps == LogicalKeyboardKey.arrowDown
            ? apps.first
            : apps.last;
        if (_webActiveInAll && key == towardApps) {
          if (!_web.move(towardApps == LogicalKeyboardKey.arrowDown ? 1 : -1)) {
            _apps.select(edgeApp);
            _rebuild(() => _webSelectedInAll = false);
          }
          return .handled;
        }
        if (!_webActiveInAll &&
            key == towardWeb &&
            _apps.selected?.id == edgeApp.id) {
          final choices = _web.layout.choices;
          _web.select(
            towardWeb == LogicalKeyboardKey.arrowUp
                ? choices.last
                : choices.first,
          );
          _rebuild(() => _webSelectedInAll = true);
          return .handled;
        }
      }
      if (_webActiveInAll) {
        final handled = _webShortcuts.handle(event, compact: true);
        if (handled == .handled) return handled;
      }
    }
    if (_inApps || (_scope == .all && !_webActiveInAll && !_filesActiveInAll)) {
      final handled = _apps.handleKey(
        event,
        _search,
        _searchChanged,
        tileColumns: _tileColumns,
      );
      if (handled == .handled) return handled;
    }
    if (_inWindows && _windows.handleKey(event) == .handled) return .handled;
    if (event is KeyUpEvent) return .ignored;

    if (key == LogicalKeyboardKey.escape && event is KeyDownEvent) {
      if (_calculator.actionsOpen) {
        _closeCalculatorActions();
        return .handled;
      }
      if (_inCalculator && _calculator.shouldShowError) {
        _calculator.dismissError();
        return .handled;
      }
      if (_inFiles &&
          _search.text.isEmpty &&
          (_files.filters.isNotEmpty ||
              _files.contents ||
              _files.mode == .path)) {
        _files.filters.clear();
        if (_files.contents) _files.toggleContents();
        _files.clearBrowseDirectory();
        _changeFilesQuery('');
      } else if (_inWeb && _web.engine != null) {
        _setScope(.web, initialQuery: '');
      } else if (_search.text.isNotEmpty) {
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
        (!_inEmoji || _searchFocus.hasPrimaryFocus) &&
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
    if (_inEmoji && _searchFocus.hasPrimaryFocus) {
      return _onEmojiKey(event, ctrl: ctrl, shift: shift, alt: alt);
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
}
