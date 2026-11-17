import 'dart:async';

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import 'apps_client.dart';

class AppChoice {
  const AppChoice(this.app, {this.frequent = false, this.window, this.action})
    : package = null,
      web = false;
  const AppChoice.install(this.package)
    : app = null,
      frequent = false,
      window = null,
      action = null,
      web = false;
  const AppChoice.web()
    : app = null,
      frequent = false,
      window = null,
      action = null,
      package = null,
      web = true;

  final LauncherAppEntry? app;
  final bool frequent, web;
  final AppWindow? window;
  final AppDesktopAction? action;
  final AppPackage? package;

  (String, String, String) get id => (
    app?.id ?? package?.id ?? '',
    frequent
        ? 'frequent'
        : window != null
        ? 'window'
        : action != null
        ? 'action'
        : package != null
        ? package!.source
        : web
        ? 'web'
        : 'app',
    window?.id ?? action?.id ?? app?.actionId ?? '',
  );

  bool get isParent =>
      app != null && window == null && action == null && app!.actionId == null;
}

class AppMenuAction {
  const AppMenuAction(
    this.action,
    this.label,
    this.icon,
    this.keys, {
    this.key,
    this.target,
    this.ctrl = false,
    this.shift = false,
    this.alt = false,
    this.danger = false,
  });

  final String action, label;
  final LauncherIconName icon;
  final List<String> keys;
  final LogicalKeyboardKey? key;
  final String? target;
  final bool ctrl, shift, alt, danger;
}

class AppsSearchController extends ChangeNotifier {
  AppsSearchController({
    required this.client,
    required this.onClose,
    required this.onCopy,
    required this.onInfo,
    required this.onWorkspace,
    required this.onSearchFocus,
  });

  static const _digits = [
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

  final AppsClient? client;
  final Future<void> Function() onClose;
  final Future<void> Function(String) onCopy, onInfo;
  final Future<String?> Function() onWorkspace;
  final VoidCallback onSearchFocus;
  final menuFocus = FocusNode();
  final menuFilter = TextEditingController();
  final scroll = ScrollController();

  AppsSearch result = const AppsSearch();
  LauncherScope? _scope;
  String query = '';
  String? _resultQuery;
  LauncherScope? _resultScope;
  Timer? _debounce, _poll;
  int _revision = 0;
  bool _disposed = false;
  bool _requesting = false;
  bool busy = false;
  bool showIndices = false;
  String? error, actionError, status;
  (String, String, String)? _errorId;
  (String, String, String)? _selection;
  int selectionRevision = 0;
  String? expandedId;
  LauncherAppEntry? menuApp;
  int menuIndex = 0;

  bool get inApps => _scope == .apps;
  bool get available => client != null;
  bool get ready => _resultQuery == query && _resultScope == _scope;
  bool get loading => !ready && error == null;
  bool get empty => query.trim().isEmpty;
  bool get elsewhere =>
      inApps && query.trim().runes.length >= 3 && result.apps.isEmpty;
  List<LauncherAppEntry> get tiles =>
      !empty ? const [] : (inApps ? result.frequent : result.pinned);
  Iterable<LauncherAppEntry> get apps => inApps
      ? result.apps
      : result.hidden || empty
      ? const []
      : result.apps.take(8);

  Iterable<AppChoice> get choices sync* {
    for (final app in tiles) {
      yield AppChoice(app, frequent: true);
    }
    for (final app in apps) {
      yield AppChoice(app);
      if (expandedId != app.id || app.actionId != null) continue;
      for (final window in app.windows) {
        yield AppChoice(app, window: window);
      }
      for (final action in app.actions) {
        yield AppChoice(app, action: action);
      }
    }
    if (elsewhere) {
      for (final package in result.packages) {
        yield AppChoice.install(package);
      }
      yield const AppChoice.web();
    }
  }

  AppChoice? get selected =>
      choices.where((choice) => choice.id == _selection).firstOrNull;
  bool isSelected(AppChoice choice) => choice.id == _selection;
  String? errorFor(AppChoice choice) =>
      _errorId == choice.id ? actionError : null;

  void update(LauncherScope scope, String value, {bool immediate = false}) {
    if (client == null) return;
    final nextScope = scope == .apps || scope == .all ? scope : null;
    if (nextScope == _scope && value == query && !immediate) return;
    _debounce?.cancel();
    _poll?.cancel();
    _revision++;
    final scopeChanged = nextScope != _scope;
    final wasSearch = !empty;
    _scope = nextScope;
    query = value;
    _selection = null;
    if (scopeChanged || (wasSearch && empty) || (!wasSearch && !empty)) {
      result = const AppsSearch();
    }
    expandedId = null;
    menuApp = null;
    error = actionError = status = null;
    if (scroll.hasClients) scroll.jumpTo(0);
    if (nextScope != null) {
      _debounce = Timer(
        immediate ? Duration.zero : const Duration(milliseconds: 75),
        _refresh,
      );
      _poll = Timer.periodic(const Duration(seconds: 2), (_) {
        if (!_requesting && !busy && (_debounce?.isActive != true)) {
          unawaited(_refresh());
        }
      });
    }
    notifyListeners();
  }

  Future<void> _refresh() async {
    final revision = _revision;
    final scope = _scope;
    if (scope == null || _disposed) return;
    _requesting = true;
    final value = query;
    try {
      final data = await client!.search(value.trim(), packages: scope == .apps);
      if (_disposed || revision != _revision) return;
      result = data;
      _resultQuery = value;
      _resultScope = scope;
      error = null;
      if (expandedId != null &&
          !apps.any(
            (app) =>
                app.id == expandedId &&
                (app.windows.isNotEmpty || app.actions.isNotEmpty),
          )) {
        expandedId = null;
      }
      if (menuApp case final previous?) {
        menuApp = [...apps, ...tiles]
            .where((app) => app.id == previous.id && app.actionId == null)
            .firstOrNull;
        if (menuApp == null) onSearchFocus();
      }
      if (selected == null) {
        final nextSelection = choices.firstOrNull?.id;
        if (_selection != nextSelection) {
          _selection = nextSelection;
          selectionRevision++;
        }
      }
      notifyListeners();
    } on Object catch (failure) {
      if (_disposed || revision != _revision) return;
      error = failure is AppsException
          ? failure.message
          : 'Apps search failed.';
      notifyListeners();
    } finally {
      if (revision == _revision) _requesting = false;
    }
  }

  void select(AppChoice choice) {
    if (_selection != choice.id) {
      _selection = choice.id;
      selectionRevision++;
      status = null;
      notifyListeners();
    }
  }

  void _move(int delta, {bool vertical = true, int tileColumns = 6}) {
    final rows = choices.toList(growable: false);
    if (rows.isEmpty) return;
    final index = rows.indexWhere(isSelected);
    final count = tiles.length;
    var next = index + delta;
    if (count > 0 && index >= 0 && vertical) {
      if (index < count) {
        final otherRow = index + delta * tileColumns;
        next = otherRow < 0
            ? index
            : otherRow < count
            ? otherRow
            : count;
      } else if (index == count && delta < 0) {
        next = (count - 1) ~/ tileColumns * tileColumns;
      }
    }
    select(rows[next.clamp(0, rows.length - 1)]);
  }

  void _collapse() {
    final parent = apps.where((app) => app.id == expandedId).firstOrNull;
    expandedId = null;
    if (parent != null) select(AppChoice(parent));
    notifyListeners();
  }

  void openMenu(LauncherAppEntry app) {
    menuApp = app;
    menuFilter.clear();
    menuIndex = 0;
    notifyListeners();
    menuFocus.requestFocus();
  }

  void closeMenu() {
    if (menuApp == null) return;
    menuApp = null;
    onSearchFocus();
    notifyListeners();
  }

  void filterMenu() {
    menuIndex = 0;
    notifyListeners();
  }

  List<AppMenuAction> forApp(LauncherAppEntry app) {
    final actions = app.actions.where(
      (a) => !a.id.toLowerCase().replaceAll('_', '-').startsWith('new-window'),
    );
    return [
      const AppMenuAction('open', 'Open', .openNew, ['↵']),
      const AppMenuAction(
        'new',
        'New window',
        .openNew,
        ['Ctrl', '↵'],
        key: LogicalKeyboardKey.enter,
        ctrl: true,
      ),
      for (final (i, action) in actions.indexed)
        AppMenuAction(
          'desktopAction',
          action.name,
          action.id.contains('private') || action.id.contains('incognito')
              ? .lock
              : .openNew,
          i < _digits.length ? ['Alt', '${i + 1}'] : const [],
          key: i < _digits.length ? _digits[i] : null,
          target: action.id,
          alt: true,
        ),
      const AppMenuAction(
        'workspace',
        'Open on workspace…',
        .arrow,
        ['Ctrl', 'O'],
        key: LogicalKeyboardKey.keyO,
        ctrl: true,
      ),
      AppMenuAction(
        'pin',
        app.pinned ? 'Unpin from launcher' : 'Pin to launcher',
        .bookmark,
        const ['Ctrl', 'D'],
        key: LogicalKeyboardKey.keyD,
        ctrl: true,
      ),
      const AppMenuAction(
        'copyCommand',
        'Copy launch command',
        .copy,
        ['Ctrl', 'C'],
        key: LogicalKeyboardKey.keyC,
        ctrl: true,
      ),
      const AppMenuAction(
        'edit',
        'Edit desktop entry',
        .code,
        ['Ctrl', 'E'],
        key: LogicalKeyboardKey.keyE,
        ctrl: true,
      ),
      const AppMenuAction(
        'info',
        'Package info',
        .description,
        ['Ctrl', 'I'],
        key: LogicalKeyboardKey.keyI,
        ctrl: true,
      ),
      if (app.windows.isNotEmpty)
        AppMenuAction(
          'quit',
          'Quit · ${app.windows.length} ${app.windows.length == 1 ? 'window' : 'windows'}',
          .power,
          const ['Ctrl', 'Q'],
          key: LogicalKeyboardKey.keyQ,
          ctrl: true,
          danger: true,
        ),
      if (app.hidden)
        const AppMenuAction(
          'unhide',
          'Show in launcher',
          .apps,
          ['Ctrl', 'H'],
          key: LogicalKeyboardKey.keyH,
          ctrl: true,
        )
      else
        const AppMenuAction(
          'hide',
          'Hide from launcher',
          .visibilityOff,
          ['⇧', 'Del'],
          key: LogicalKeyboardKey.delete,
          shift: true,
          danger: true,
        ),
    ];
  }

  List<AppMenuAction> get menuActions {
    final app = menuApp;
    if (app == null) return const [];
    final filter = menuFilter.text.trim().toLowerCase();
    return forApp(app)
        .where((a) => a.label.toLowerCase().contains(filter))
        .toList(growable: false);
  }

  Future<void> runMenu(AppMenuAction action) async {
    final app = menuApp;
    if (app == null) return;
    final choice = selected;
    closeMenu();
    await run(
      choice?.app?.id == app.id ? choice! : AppChoice(app),
      action.action,
      action.target,
    );
  }

  Future<void> run(
    AppChoice choice, [
    String? operation,
    String? target,
  ]) async {
    if (busy || !ready || error != null) return;
    final app = choice.app;
    final package = choice.package;
    final action =
        operation ??
        (choice.web
            ? 'web'
            : package != null
            ? 'install'
            : choice.window != null
            ? 'window'
            : choice.action != null || app?.actionId != null
            ? 'desktopAction'
            : 'open');
    final revision = _revision;
    busy = true;
    actionError = status = null;
    notifyListeners();
    try {
      if (action == 'workspace') {
        target = await onWorkspace();
        if (target == null || _disposed || revision != _revision) return;
      }
      final text = await client!.perform(
        action,
        appId: app?.id,
        query: query.trim(),
        target:
            target ?? choice.window?.id ?? choice.action?.id ?? app?.actionId,
        packageId: package?.id,
        source: package?.source,
      );
      if (_disposed || revision != _revision) return;
      switch (action) {
        case 'open' ||
            'new' ||
            'desktopAction' ||
            'window' ||
            'workspace' ||
            'edit' ||
            'install' ||
            'web':
          await onClose();
        case 'copyCommand':
          if (text != null) await onCopy(text);
          status = 'Copied launch command.';
        case 'info':
          await onInfo(text ?? 'No package information available.');
        default:
          await _refresh();
      }
    } on Object catch (failure) {
      if (!_disposed && revision == _revision) {
        _errorId = choice.id;
        actionError = failure is AppsException
            ? failure.message
            : 'Could not complete the app action.';
      }
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  KeyEventResult handleKey(
    KeyEvent event,
    TextEditingController search,
    ValueChanged<String> onSearch, {
    required int tileColumns,
  }) {
    if (_scope == null || error != null) return .ignored;
    final keys = HardwareKeyboard.instance;
    final key = event.logicalKey;
    final ctrl = keys.isControlPressed;
    final shift = keys.isShiftPressed;
    final alt = keys.isAltPressed;
    final enter =
        key == LogicalKeyboardKey.enter ||
        key == LogicalKeyboardKey.numpadEnter;
    if (showIndices != alt) {
      showIndices = alt;
      notifyListeners();
    }
    if (event is KeyUpEvent) return .ignored;
    if (menuApp != null) {
      if (key == LogicalKeyboardKey.escape) {
        closeMenu();
        return .handled;
      }
      final actions = menuActions;
      if (key == LogicalKeyboardKey.arrowUp ||
          key == LogicalKeyboardKey.arrowDown) {
        if (actions.isNotEmpty) {
          menuIndex = (menuIndex + (key == LogicalKeyboardKey.arrowUp ? -1 : 1))
              .clamp(0, actions.length - 1);
          notifyListeners();
        }
        return .handled;
      }
      if (enter && !ctrl && !alt && !shift) {
        if (actions.isNotEmpty && event is KeyDownEvent) {
          unawaited(runMenu(actions[menuIndex.clamp(0, actions.length - 1)]));
        }
        return .handled;
      }
      final shortcut = forApp(menuApp!)
          .where(
            (action) =>
                (action.key == key ||
                    (enter && action.key == LogicalKeyboardKey.enter)) &&
                action.ctrl == ctrl &&
                action.shift == shift &&
                action.alt == alt,
          )
          .firstOrNull;
      if (shortcut != null) {
        if (event is KeyDownEvent) unawaited(runMenu(shortcut));
        return .handled;
      }
      return enter ? .handled : .ignored;
    }
    if (expandedId != null &&
        (key == LogicalKeyboardKey.escape ||
            (ctrl && key == LogicalKeyboardKey.arrowLeft))) {
      _collapse();
      return .handled;
    }
    if (!ready || choices.isEmpty) return .ignored;
    final selected = this.selected;
    final app = selected?.app;
    if ((!ctrl && !alt) &&
        (key == LogicalKeyboardKey.arrowUp ||
            key == LogicalKeyboardKey.arrowDown)) {
      _move(
        key == LogicalKeyboardKey.arrowUp ? -1 : 1,
        tileColumns: tileColumns,
      );
      return .handled;
    }
    if (alt && !ctrl && !shift && event is KeyDownEvent) {
      final index = _digits.indexOf(key);
      final quick = quickChoices;
      if (index >= 0 && index < quick.length) {
        unawaited(run(quick[index]));
        return .handled;
      }
    }
    if (selected == null) return .ignored;
    if (!ctrl &&
        !alt &&
        selected.frequent &&
        (search.text.isEmpty) &&
        (key == LogicalKeyboardKey.arrowLeft ||
            key == LogicalKeyboardKey.arrowRight)) {
      final index = tiles.indexWhere((tile) => tile.id == selected.app?.id);
      final delta = key == LogicalKeyboardKey.arrowLeft ? -1 : 1;
      final next = index + delta;
      if (next >= 0 && next < tiles.length) {
        select(AppChoice(tiles[next], frequent: true));
      }
      return .handled;
    }
    if (event is! KeyDownEvent || alt) return .ignored;
    if (enter && !shift) {
      unawaited(
        run(
          ctrl && elsewhere ? const AppChoice.web() : selected,
          ctrl && app != null && !elsewhere ? 'new' : null,
        ),
      );
      return .handled;
    }
    if (ctrl && key == LogicalKeyboardKey.keyI && selected.package != null) {
      unawaited(run(selected, 'info'));
      return .handled;
    }
    if (app == null) return .ignored;
    if (key == LogicalKeyboardKey.tab && !ctrl && !shift && !empty) {
      final best = apps.firstOrNull?.name ?? app.name;
      search.value = TextEditingValue(
        text: best,
        selection: TextSelection.collapsed(offset: best.length),
      );
      onSearch(best);
      return .handled;
    }
    final atEnd =
        search.selection.isCollapsed &&
        search.selection.extentOffset == search.text.length &&
        search.value.composing.isCollapsed;
    if (ctrl &&
        key == LogicalKeyboardKey.arrowRight &&
        atEnd &&
        selected.isParent) {
      if (!selected.frequent &&
          (app.windows.isNotEmpty || app.actions.isNotEmpty)) {
        expandedId = app.id;
        select(
          app.windows.isNotEmpty
              ? AppChoice(app, window: app.windows.first)
              : AppChoice(app, action: app.actions.first),
        );
        notifyListeners();
      }
      return .handled;
    }
    if ((ctrl && key == LogicalKeyboardKey.keyK) ||
        (!ctrl && key == LogicalKeyboardKey.arrowRight && atEnd)) {
      if (selected.isParent) {
        openMenu(app);
        return .handled;
      }
    }
    if (ctrl &&
        key == LogicalKeyboardKey.keyC &&
        !search.selection.isCollapsed) {
      return .ignored;
    }
    final shortcut = forApp(app)
        .where(
          (action) =>
              action.key == key &&
              action.ctrl == ctrl &&
              action.shift == shift &&
              !action.alt,
        )
        .firstOrNull;
    if (shortcut == null) return .ignored;
    unawaited(run(selected, shortcut.action, shortcut.target));
    return .handled;
  }

  List<AppChoice> get quickChoices {
    final rows = choices;
    if (empty && tiles.isNotEmpty) return rows.take(tiles.length).toList();
    return rows
        .where((r) => r.window == null && r.action == null && !r.web)
        .take(9)
        .toList();
  }

  @override
  void dispose() {
    _disposed = true;
    _debounce?.cancel();
    _poll?.cancel();
    menuFocus.dispose();
    menuFilter.dispose();
    scroll.dispose();
    unawaited(client?.close());
    super.dispose();
  }
}
