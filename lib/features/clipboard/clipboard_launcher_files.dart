part of 'clipboard_launcher.dart';

extension _FilesLauncher on _ClipboardLauncherState {
  void _updateFiles(String query, {bool immediate = false}) => _files.update(
    active: _inFiles || _scope == .all,
    dedicated: _inFiles,
    query: query,
    immediate: immediate,
  );

  void _changeFilesQuery(String text) {
    _search.value = TextEditingValue(
      text: text,
      selection: TextSelection.collapsed(offset: text.length),
    );
    _updateFiles(text, immediate: true);
    _searchFocus.requestFocus();
  }

  void _filesSearchChanged(String text) {
    if (_emojiComposing) return;
    if (!_files.contents && text.startsWith('rg ') && _files.contentsEnabled) {
      _files.toggleContents();
      _changeFilesQuery(text.substring(3));
      return;
    }
    final (query, filters) = filesCommitFilters(text);
    if (filters.isEmpty) {
      _files.update(active: true, dedicated: true, query: text);
      return;
    }
    _files.filters.addAll(filters);
    _changeFilesQuery(query);
  }

  void _switchFilesContents() {
    if (!_files.contentsEnabled && !_files.contents) {
      _files.report('Contents search is disabled in Files settings.');
      return;
    }
    final wasPath = _files.mode == .path;
    final query = _files.result?.createName ?? (wasPath ? '' : _search.text);
    _files.toggleContents();
    _changeFilesQuery(query);
  }

  void _closeFilesMenu({bool restoreFocus = true}) {
    if (_filesMenuEntry == null) return;
    _rebuild(() {
      _filesMenuEntry = null;
      _filesMenuData = const {};
      _filesMenuLoading = false;
      _filesChoosingApp = false;
      _filesSetDefault = false;
      _filesMenuIndex = 0;
      _filesMenuQuery.clear();
    });
    if (restoreFocus) _searchFocus.requestFocus();
  }

  Future<void> _openFilesMenu(FileEntry entry, {bool chooseApp = false}) async {
    _apps.closeMenu();
    _web.closeMenu();
    _rebuild(() {
      _filesMenuEntry = entry;
      _filesMenuIndex = 0;
      _filesMenuLoading = true;
      _filesChoosingApp = chooseApp;
      _filesMenuData = const {};
      _filesMenuQuery.clear();
    });
    _filesMenuFocus.requestFocus();
    try {
      final data = await _files.request('action', {
        'action': 'applications',
        'entryId': entry.id,
        'version': ?entry.version,
      });
      if (!mounted || _filesMenuEntry != entry) return;
      _rebuild(() => _filesMenuData = data);
    } on Object catch (error) {
      if (mounted && _filesMenuEntry == entry) {
        _files.report(
          error is FilesException
              ? error.message
              : 'Could not load file actions.',
        );
      }
    } finally {
      if (mounted && _filesMenuEntry == entry) {
        _rebuild(() => _filesMenuLoading = false);
      }
    }
  }

  List<FileMenuItem> _fileMenuItems(FileEntry entry) {
    void perform(String action, [Map<String, Object?> params = const {}]) {
      if (action != 'openWith') _closeFilesMenu();
      unawaited(_performFileAction(action, entry: entry, params: params));
    }

    final query = _filesMenuQuery.text.trim().toLowerCase();
    final apps = (_filesMenuData['applications'] as List<dynamic>? ?? const [])
        .cast<Map<String, dynamic>>();
    final items = _filesChoosingApp
        ? [
            for (final app in apps)
              FileMenuItem(
                app['name'] as String,
                Icons.open_in_new_rounded,
                () => perform('openWith', {
                  'appId': app['id'] as String,
                  'setDefault': _filesSetDefault,
                }),
              ),
          ]
        : [
            FileMenuItem(
              entry.line == null ? 'Open' : 'Open at line ${entry.line}',
              Icons.open_in_new_rounded,
              () => perform(entry.line == null ? 'open' : 'editor'),
              keys: const ['↵'],
            ),
            FileMenuItem(
              'Open with…',
              Icons.swap_horiz_rounded,
              () => _rebuild(() {
                _filesChoosingApp = true;
                _filesMenuIndex = 0;
                _filesMenuQuery.clear();
                _filesMenuFocus.requestFocus();
              }),
              keys: const ['Ctrl', 'O'],
            ),
            if (_filesMenuData['editable'] == true || entry.line != null)
              FileMenuItem(
                'Open in editor',
                Icons.edit_rounded,
                () => perform('editor'),
                keys: const ['Ctrl', 'E'],
              ),
            FileMenuItem(
              'Reveal in file manager',
              Icons.folder_open_rounded,
              () => perform('reveal'),
              keys: const ['Ctrl', 'R'],
            ),
            if (_filesMenuData['terminalAvailable'] == true)
              FileMenuItem(
                'Open terminal here',
                Icons.terminal_rounded,
                () => perform('terminal'),
                keys: const ['Ctrl', '↵'],
              ),
            if (_filesMenuData['runAvailable'] == true)
              FileMenuItem(
                'Run in terminal…',
                Icons.play_arrow_rounded,
                () => perform('run'),
                keys: const ['Ctrl', '⇧', '↵'],
              ),
            if (entry.completion != null)
              FileMenuItem(
                'Copy path',
                Icons.content_copy_rounded,
                () => perform('copyPath'),
                keys: const ['Ctrl', 'C'],
              ),
            if (_filesMenuData['copyFileAvailable'] == true)
              FileMenuItem(
                'Copy file',
                Icons.content_paste_rounded,
                () => perform('copyFile'),
                keys: const ['Ctrl', '⇧', 'C'],
              ),
            if (!entry.isPlace)
              FileMenuItem(
                'Rename…',
                Icons.edit_rounded,
                () => perform('rename'),
                keys: const ['F2'],
              ),
            FileMenuItem(
              entry.pinned ? 'Unpin from Launcher' : 'Pin to Launcher',
              entry.pinned ? Icons.star_border_rounded : Icons.star_rounded,
              () => perform(entry.pinned ? 'unpin' : 'pin'),
              keys: const ['Ctrl', 'D'],
            ),
            if (entry.group == 'recent')
              FileMenuItem(
                'Forget from recent',
                Icons.history_rounded,
                () => perform('forget'),
                keys: const ['⇧', 'Del'],
                danger: true,
              ),
            if (!entry.isPlace)
              FileMenuItem(
                'Move to trash…',
                Icons.delete_outline_rounded,
                () => perform('trash'),
                keys: const ['Ctrl', 'Del'],
                danger: true,
              ),
          ];
    return items
        .where((item) => item.label.toLowerCase().contains(query))
        .toList();
  }

  Widget _filesMenu(FileEntry entry) => FilesActionsMenu(
    title: _filesChoosingApp ? 'Open ${entry.name} with…' : entry.name,
    items: _fileMenuItems(entry),
    selected: _filesMenuIndex,
    search: _filesMenuQuery,
    focus: _filesMenuFocus,
    onChanged: (_) => _rebuild(() => _filesMenuIndex = 0),
    loading: _filesMenuLoading,
    indexes: _filesShowIndexes && _filesChoosingApp,
    setDefault: _filesChoosingApp ? _filesSetDefault : null,
    onSetDefault: (value) => _rebuild(() => _filesSetDefault = value),
  );

  void _activateFileChoice(FileChoice choice) {
    switch (choice.kind) {
      case .entry:
        final entry = choice.entry!;
        unawaited(
          _performFileAction(
            entry.line == null ? 'open' : 'editor',
            entry: entry,
          ),
        );
      case .createFile:
        unawaited(_createFile(directory: false));
      case .createDirectory:
        unawaited(_createFile(directory: true));
      case .contents:
        _switchFilesContents();
      case .filter:
        final query = _files.acceptFilter(choice);
        if (query != null) _changeFilesQuery(query);
      case .more:
        _files.expand(choice);
    }
  }

  Future<void> _createFile({required bool directory}) async {
    final result = _files.result;
    if (result?.createWritable != true ||
        result?.directoryId == null ||
        result?.directoryVersion == null ||
        result?.createRelative == null) {
      _files.report('This path cannot be created.');
      return;
    }
    await _performFileAction(
      directory ? 'createDirectory' : 'createFile',
      params: {
        'entryId': result!.directoryId,
        'version': result.directoryVersion,
        'name': result.createRelative,
      },
    );
  }

  Future<void> _performFileAction(
    String action, {
    FileEntry? entry,
    Map<String, Object?> params = const {},
  }) async {
    if (_files.busy) return;
    _files.busy = true;
    _files.report(null);
    try {
      var arguments = <String, Object?>{
        'entryId': ?entry?.id,
        'version': ?entry?.version,
        'line': ?entry?.line,
        if (const {'open', 'openWith', 'editor'}.contains(action))
          'operationId': '$_fileSession-${++_fileOperation}',
        ...params,
      };
      if (action == 'copyPath') {
        if (entry?.completion == null) {
          _files.report('This path cannot be represented as clipboard text.');
          return;
        }
        final data = await _files.request('action', {
          'action': action,
          ...arguments,
        });
        await Clipboard.setData(ClipboardData(text: data['path'] as String));
        _files.report('Path copied');
        return;
      }
      if (action == 'rename') {
        final name = await _askFileName(entry!);
        if (name == null || name == entry.name || !mounted) return;
        arguments = {...arguments, 'name': name};
      }
      if (action == 'trash' || action == 'run') {
        final confirm = await _confirmFileAction(action, entry!);
        if (!confirm || !mounted) return;
        arguments = {...arguments, 'confirmed': true};
      }
      final history = const {'pin', 'unpin', 'forget'}.contains(action);
      final data = await _files.request(
        history ? 'history.$action' : 'action',
        history ? arguments : {'action': action, ...arguments},
      );
      if (!mounted) return;
      if (data['dispatched'] == true) {
        _closeFilesMenu();
        if (data['warning'] case final String warning) {
          _files.report(warning);
        } else {
          await _dismiss();
        }
        return;
      }
      switch (action) {
        case 'copyFile':
          _files.report('File copied');
          break;
        case 'pin':
        case 'unpin':
        case 'forget':
          _files.retry();
          _files.report(switch (action) {
            'pin' => 'Pinned to Launcher',
            'unpin' => 'Unpinned',
            _ => 'Removed from recent files',
          });
          break;
        case 'rename':
        case 'createDirectory':
        case 'createFile':
        case 'trash':
          final path = data['path'] as String?;
          if (action == 'createDirectory' && path != null) {
            _changeFilesQuery(path.endsWith('/') ? path : '$path/');
          } else {
            _files.retry();
          }
          _files.rememberUndo(data['undoId'] as String?, switch (action) {
            'rename' => 'Renamed',
            'trash' => 'Moved to trash',
            'createDirectory' => 'Folder created',
            _ => 'File created',
          });
          break;
        case 'undo':
          _files.retry();
          _files.rememberUndo(null, 'Undone');
          break;
      }
    } on Object catch (error) {
      if (!mounted) return;
      final message = error is FilesException
          ? error.message
          : 'Could not complete the file action.';
      _files.report(message);
      if (action == 'open' &&
          entry != null &&
          (error is FilesException && error.code == 'NO_ASSOCIATED_APP' ||
              message.toLowerCase().contains('associated application'))) {
        unawaited(_openFilesMenu(entry, chooseApp: true));
      }
    } finally {
      _files.busy = false;
      if (mounted && _inFiles && _filesMenuEntry == null) {
        _searchFocus.requestFocus();
      }
    }
  }

  Future<String?> _askFileName(FileEntry entry) => showDialog<String>(
    context: context,
    builder: (_) => _RenameFileDialog(entry),
  );

  Future<bool> _confirmFileAction(String action, FileEntry entry) async {
    final trash = action == 'trash';
    return await showDialog<bool>(
          context: context,
          builder: (context) => AlertDialog(
            title: Text(trash ? 'Move to trash?' : 'Run in terminal?'),
            content: SelectableText(entry.path),
            actions: [
              TextButton(
                autofocus: true,
                onPressed: () => Navigator.pop(context, false),
                child: const Text('Cancel'),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(context, true),
                child: Text(trash ? 'Move to trash' : 'Run'),
              ),
            ],
          ),
        ) ==
        true;
  }

  Widget? _filesQueryTokens(double maxWidth) {
    if (!_files.contents && _files.filters.isEmpty) return null;
    return ConstrainedBox(
      constraints: BoxConstraints(maxWidth: maxWidth.clamp(0, 300)),
      child: SingleChildScrollView(
        scrollDirection: .horizontal,
        child: Row(
          mainAxisSize: .min,
          spacing: 8,
          children: [
            if (_files.contents)
              LauncherQueryToken(
                'Contents',
                icon: .text,
                removeLabel: 'Exit contents search',
                onRemove: _switchFilesContents,
              ),
            for (final filter in _files.filters)
              _FilesFilterToken(
                filter,
                onRemove: () {
                  _files.filters.remove(filter);
                  _changeFilesQuery(_search.text);
                },
              ),
          ],
        ),
      ),
    );
  }

  Widget? _filesSearchHint() {
    if (_files.completionSuffix != null) {
      return const LauncherKeyHint(['Tab'], label: 'accept');
    }
    if (_search.text.isEmpty && _files.filters.isNotEmpty) {
      return const LauncherKeyHint(['⌫'], label: 'remove filter');
    }
    if (_files.hint case final hint?) {
      return ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 170),
        child: Text(
          hint,
          maxLines: 1,
          overflow: .ellipsis,
          style: Theme.of(context).textTheme.bodySmall,
        ),
      );
    }
    return null;
  }

  Widget _filesBody({required bool compact, bool all = false}) => FilesResults(
    groups: _files.groups,
    selectedKey: !all || _filesActiveInAll ? _files.selected?.key : null,
    query: _files.result?.matchQuery ?? _search.text,
    compact: compact,
    scrollable: !all,
    showIndexes: _filesShowIndexes && (!all || _filesActiveInAll),
    loading: !all && _files.loading,
    diagnostic: all ? null : _files.diagnostic,
    onRetry: _files.failed ? _files.retry : null,
    createWritable: _files.result?.createWritable ?? false,
    onSelect: (choice) {
      if (all) _rebuild(() => _filesSelectedInAll = true);
      _files.select(choice);
      _searchFocus.requestFocus();
    },
    onActivate: _activateFileChoice,
    onDrag: (entry) => unawaited(_beginFileDrag(entry)),
    preview: all || !_files.showPreview
        ? null
        : FilesPreview(
            choice: _files.selected,
            result: _files.result,
            data: _files.preview,
            loading: _files.previewLoading,
          ),
  );

  Future<void> _beginFileDrag(FileEntry entry) async {
    try {
      const channel = MethodChannel('top.remya.launcher/window');
      await channel.invokeMethod<bool>('beginFileDrag', entry.path);
    } on PlatformException catch (error) {
      _files.report(error.message ?? 'Could not start dragging the file.');
    } on MissingPluginException {
      _files.report('File dragging is unavailable.');
    }
  }

  Future<void> _openFilesHistorySettings() async {
    try {
      await Process.start(Platform.resolvedExecutable, const [
        '--settings',
        FilesPreferences.historyId,
      ], mode: .detached);
    } on ProcessException {
      _files.report('Could not open Files settings.');
    }
  }

  LauncherFooter _filesFooter(bool singleLine) {
    final choice = _files.selected;
    final creating = choice?.isCreate == true;
    final actions = _filesMenuEntry != null;
    return LauncherFooter(
      singleLine: singleLine,
      leading: [
        if (actions) ...const [
          LauncherKeyHint(['↑', '↓'], label: 'Choose'),
          LauncherKeyHint(['↵'], label: 'Run action'),
          LauncherKeyHint(['Esc'], label: 'Back'),
        ] else ...[
          if (choice != null)
            LauncherKeyHint(
              ['↵'],
              label: creating
                  ? 'Create'
                  : choice.entry?.line != null
                  ? 'Open at line'
                  : 'Open',
            ),
          if (_files.completion != null || choice?.entry?.isDirectory == true)
            LauncherKeyHint([
              'Tab',
            ], label: _files.mode == .path ? 'Complete' : 'Browse into'),
          if (creating)
            const LauncherKeyHint(['Ctrl', '↵'], label: 'Create folder')
          else if (_files.contents)
            const LauncherKeyHint(['Ctrl', 'G'], label: 'Search names')
          else if (choice?.entry != null)
            const LauncherKeyHint(['Ctrl', 'K'], label: 'Actions'),
        ],
      ],
      trailing: [
        if (_inFiles &&
            !actions &&
            _files.mode == .empty &&
            !_files.historyEnabled &&
            !singleLine)
          TextButton(
            style: TextButton.styleFrom(
              visualDensity: .compact,
              tapTargetSize: .shrinkWrap,
            ),
            onPressed: () => unawaited(_openFilesHistorySettings()),
            child: const Text('Recent files are off'),
          ),
        if (_inFiles && !actions)
          LauncherKeyHint([
            'Ctrl',
            'Space',
          ], label: _files.showPreview ? 'Hide preview' : 'Show preview'),
      ],
    );
  }
}

class _FilesFilterToken extends StatelessWidget {
  const _FilesFilterToken(this.filter, {required this.onRemove});

  final String filter;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return ActionChip(
      visualDensity: .compact,
      backgroundColor: colors.secondaryContainer,
      side: .none,
      avatar: Icon(
        Icons.filter_list_rounded,
        size: 16,
        color: colors.onSecondaryContainer,
      ),
      label: Text(filter, maxLines: 1, overflow: .ellipsis),
      tooltip: 'Remove $filter',
      onPressed: onRemove,
    );
  }
}

class _RenameFileDialog extends StatefulWidget {
  const _RenameFileDialog(this.entry);

  final FileEntry entry;

  @override
  State<_RenameFileDialog> createState() => _RenameFileDialogState();
}

class _RenameFileDialogState extends State<_RenameFileDialog> {
  late final _text = TextEditingController.fromValue(
    TextEditingValue(
      text: widget.entry.name,
      selection: TextSelection(baseOffset: 0, extentOffset: _basenameLength),
    ),
  );

  int get _basenameLength {
    final name = widget.entry.name;
    final dot = name.lastIndexOf('.');
    return !widget.entry.isDirectory && dot > 0 ? dot : name.length;
  }

  @override
  void dispose() {
    _text.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.entry.isDirectory ? 'Rename folder' : 'Rename file'),
    content: TextField(
      controller: _text,
      autofocus: true,
      decoration: const InputDecoration(labelText: 'Name'),
      onSubmitted: (value) => Navigator.pop(context, value),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        onPressed: () => Navigator.pop(context, _text.text),
        child: const Text('Rename'),
      ),
    ],
  );
}
