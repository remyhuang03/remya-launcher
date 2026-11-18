part of 'clipboard_launcher.dart';

const _fileShortcutDigits = [
  LogicalKeyboardKey.digit1,
  LogicalKeyboardKey.digit2,
  LogicalKeyboardKey.digit3,
  LogicalKeyboardKey.digit4,
  LogicalKeyboardKey.digit5,
  LogicalKeyboardKey.digit6,
  LogicalKeyboardKey.digit7,
  LogicalKeyboardKey.digit8,
  LogicalKeyboardKey.digit9,
];

extension _FilesKeyboard on _ClipboardLauncherState {
  KeyEventResult _onFilesKey(
    KeyEvent event, {
    required bool ctrl,
    required bool alt,
    required bool shift,
  }) {
    if (_filesShowIndexes != alt) _rebuild(() => _filesShowIndexes = alt);
    if (event is KeyUpEvent || _emojiComposing) return .ignored;
    final key = event.logicalKey;
    final down = event is KeyDownEvent;
    final enter =
        key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter;
    final up = key == LogicalKeyboardKey.arrowUp;
    final arrow = up || key == LogicalKeyboardKey.arrowDown;
    final choice = _files.selected;
    if (_filesMenuEntry case final menu?) {
      final items = _fileMenuItems(menu);
      if (down && key == LogicalKeyboardKey.escape) {
        if (_filesChoosingApp) {
          _rebuild(() {
            _filesChoosingApp = false;
            _filesMenuIndex = 0;
            _filesMenuQuery.clear();
          });
        } else {
          _closeFilesMenu();
        }
        return .handled;
      }
      if (arrow && !alt) {
        if (items.isNotEmpty) {
          _rebuild(
            () => _filesMenuIndex = (_filesMenuIndex + (up ? -1 : 1)).clamp(
              0,
              items.length - 1,
            ),
          );
        }
        return .handled;
      }
      if (alt && !ctrl && !shift && down && _filesChoosingApp) {
        final index = _fileShortcutDigits.indexOf(key);
        if (index >= 0 && index < items.length) {
          items[index].run();
          return .handled;
        }
      }
      if (enter && !alt && !ctrl && !shift) {
        if (down && items.isNotEmpty) {
          items[_filesMenuIndex.clamp(0, items.length - 1)].run();
        }
        return .handled;
      }
      return .ignored;
    }

    if (arrow && !alt && !shift) {
      _files.move(up ? -1 : 1, byGroup: ctrl);
      return .handled;
    }
    if (alt && !ctrl && !shift && down) {
      final index = _fileShortcutDigits.indexOf(key);
      final choices = _files.mode == .empty && _inFiles
          ? _files.groups.where((group) => group.tiles).firstOrNull?.choices ??
                _files.choices
          : _files.choices;
      if (index >= 0 && index < choices.length) {
        _files.select(choices[index]);
        _activateFileChoice(choices[index]);
        return .handled;
      }
    }
    if (alt) return .ignored;
    if (ctrl && !shift) {
      if ((key == LogicalKeyboardKey.keyW ||
              key == LogicalKeyboardKey.backspace) &&
          _inFiles &&
          _files.mode == .path &&
          _search.selection.isCollapsed) {
        _goFileParent();
        return .handled;
      }
      if (down && key == LogicalKeyboardKey.keyG && _inFiles) {
        _switchFilesContents();
        return .handled;
      }
      if (down && key == LogicalKeyboardKey.keyH && _inFiles) {
        unawaited(_files.toggleHidden());
        return .handled;
      }
      if (down && key == LogicalKeyboardKey.space && _inFiles) {
        unawaited(_files.togglePreview());
        return .handled;
      }
      if (key == LogicalKeyboardKey.keyZ && _files.undoId != null) {
        if (down) {
          unawaited(
            _performFileAction('undo', params: {'undoId': _files.undoId}),
          );
        }
        return .handled;
      }
      if (key == LogicalKeyboardKey.arrowRight &&
          _files.contents &&
          choice?.entry != null) {
        if (down) _files.expand(choice!);
        return .handled;
      }
    }

    if (!ctrl &&
        !shift &&
        down &&
        key == LogicalKeyboardKey.backspace &&
        _search.text.isEmpty &&
        _inFiles) {
      if (_files.filters.isNotEmpty) {
        _files.filters.removeLast();
        _changeFilesQuery('');
        return .handled;
      }
      if (_files.contents) {
        _switchFilesContents();
        return .handled;
      }
      if (_files.mode == .path) {
        _files.clearBrowseDirectory();
        _changeFilesQuery('');
        return .handled;
      }
    }

    if (!ctrl && !shift && down && key == LogicalKeyboardKey.tab && _inFiles) {
      if (_files.acceptFilterCandidate() case final query?) {
        _changeFilesQuery(query);
        return .handled;
      }
      if (_files.completion case final path?) {
        _changeFilesQuery(path);
        return .handled;
      }
      if (choice?.entry case final entry? when entry.isDirectory) {
        _files.browseOpaqueDirectory(entry);
        _changeFilesQuery('');
        return .handled;
      }
    }

    final entry = choice?.entry;
    if (enter) {
      if (down) {
        if (ctrl && !shift && choice?.isCreate == true) {
          unawaited(_createFile(directory: true));
        } else if (!ctrl && !shift && choice != null) {
          _activateFileChoice(choice);
        } else if (ctrl && entry != null) {
          unawaited(
            _performFileAction(shift ? 'run' : 'terminal', entry: entry),
          );
        }
      }
      return .handled;
    }
    if (entry == null || !down) return .ignored;
    final atEnd =
        _search.selection.isCollapsed && _cursorUtf16() == _search.text.length;
    if ((!shift && ctrl && key == LogicalKeyboardKey.keyK) ||
        (!shift && !ctrl && key == LogicalKeyboardKey.arrowRight && atEnd)) {
      unawaited(_openFilesMenu(entry));
      return .handled;
    }
    if (!shift && ctrl && key == LogicalKeyboardKey.keyO) {
      unawaited(_openFilesMenu(entry, chooseApp: true));
      return .handled;
    }
    if (key == LogicalKeyboardKey.keyC &&
        _searchFocus.hasFocus &&
        !_search.selection.isCollapsed) {
      return .ignored;
    }
    final action = switch ((ctrl, shift, key)) {
      (true, false, LogicalKeyboardKey.keyE) when !entry.isDirectory =>
        'editor',
      (true, false, LogicalKeyboardKey.keyR) => 'reveal',
      (true, false, LogicalKeyboardKey.keyC) => 'copyPath',
      (true, true, LogicalKeyboardKey.keyC) => 'copyFile',
      (false, false, LogicalKeyboardKey.f2) when !entry.isPlace => 'rename',
      (true, false, LogicalKeyboardKey.keyD) => entry.pinned ? 'unpin' : 'pin',
      (false, true, LogicalKeyboardKey.delete) when entry.group == 'recent' =>
        'forget',
      (true, false, LogicalKeyboardKey.delete) when !entry.isPlace => 'trash',
      _ => null,
    };
    if (action == null) return .ignored;
    unawaited(_performFileAction(action, entry: entry));
    return .handled;
  }

  void _goFileParent() {
    final text = _search.text;
    if (text.isEmpty) {
      _files.clearBrowseDirectory();
      _changeFilesQuery('');
      return;
    }
    final trimmed = text.endsWith('/')
        ? text.substring(0, text.length - 1)
        : text;
    final separator = trimmed.lastIndexOf('/');
    if (separator < 0) {
      if (text == '/') return;
      final directory = _files.result?.directory;
      _changeFilesQuery(
        directory == null || directory == text ? '' : '$directory/',
      );
      return;
    }
    _changeFilesQuery(text.substring(0, separator + 1));
  }

  KeyEventResult _onAllFilesKey(
    KeyEvent event, {
    required bool ctrl,
    required bool alt,
    required bool shift,
  }) {
    if (_files.choices.isEmpty ||
        _apps.menuApp != null ||
        _web.menuChoice != null) {
      return .ignored;
    }
    if (!ctrl && !alt && !shift && event is! KeyUpEvent) {
      final key = event.logicalKey;
      final webBelowApps =
          _web.layout.direct == null && _web.layout.choices.isNotEmpty;
      final appsLast = _apps.choices.lastOrNull;
      final webLast = _web.layout.choices.lastOrNull;
      if (_filesActiveInAll &&
          key == LogicalKeyboardKey.arrowUp &&
          _files.selected?.key == _files.choices.first.key) {
        if (webLast != null && (webBelowApps || appsLast == null)) {
          _web.select(webLast);
          _rebuild(() {
            _webSelectedInAll = true;
            _filesSelectedInAll = false;
          });
          return .handled;
        }
        if (appsLast != null) {
          _apps.select(appsLast);
          _rebuild(() {
            _webSelectedInAll = false;
            _filesSelectedInAll = false;
          });
          return .handled;
        }
      } else if (!_filesActiveInAll && key == LogicalKeyboardKey.arrowDown) {
        final atBottom = _webActiveInAll
            ? _web.selected?.key == webLast?.key &&
                  (webBelowApps || appsLast == null)
            : !webBelowApps &&
                  _apps.selected?.id == appsLast?.id &&
                  appsLast != null;
        if (atBottom) {
          _files.select(_files.choices.first);
          _rebuild(() => _filesSelectedInAll = true);
          return .handled;
        }
      }
    }
    if (!_filesActiveInAll) return .ignored;
    return _onFilesKey(event, ctrl: ctrl, alt: alt, shift: shift);
  }
}
