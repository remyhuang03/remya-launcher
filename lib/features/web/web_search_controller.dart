import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import '../launcher/launcher_details.dart';
import '../launcher/launcher_panel.dart';
import 'web_client.dart';
import 'web_results.dart';

class WebSearchController extends ChangeNotifier {
  WebSearchController({
    required this.client,
    required this.onClose,
    required this.onSearchFocus,
    required this.confirmForget,
  });

  final WebClient? client;
  final Future<void> Function() onClose;
  final VoidCallback onSearchFocus;
  final Future<bool> Function(WebChoice choice, bool site) confirmForget;
  final menuFocus = FocusNode();
  final menuFilter = TextEditingController();

  List<WebEngine> engines = const [];
  WebEngine? engine;
  WebSearchResult? result;
  WebChoice? menuChoice;
  List<WebAction> _actions = const [];
  String query = '';
  String? selectedKey, error, status;
  bool _active = false, _compact = false, _disposed = false, _busy = false;
  bool loading = false;
  int menuIndex = 0;
  int _revision = 0, _menuRevision = 0;
  Timer? _timer;
  Future<void>? _engineLoad;
  Future<void> _iconQueue = Future.value();

  bool get available => client?.available ?? false;
  WebResultLayout get layout => WebResultLayout(
    result,
    query: query,
    engine: engine,
    engines: engines,
    compact: _compact,
  );
  WebChoice? get selected =>
      layout.choices.where((choice) => choice.key == selectedKey).firstOrNull ??
      layout.choices.firstOrNull;

  Future<void> loadEngines() => _engineLoad ??= _loadEngines();

  Future<void> _loadEngines() async {
    if (!available) return;
    try {
      final value = await client!.engines();
      if (_disposed) return;
      engines = value;
      notifyListeners();
    } on Object {
      _engineLoad = null;
    }
  }

  WebEngine? keyword(String text) {
    final match = RegExp(r'^([a-zA-Z]+) ').firstMatch(text);
    if (match == null) return null;
    return engines
        .where((item) => item.keyword == match[1]!.toLowerCase())
        .firstOrNull;
  }

  void update(
    LauncherScope scope,
    String text, {
    WebEngine? engine,
    bool immediate = false,
  }) {
    if (!available) return;
    final compact = scope == .all;
    final active = scope == .web || (compact && text.trim().isNotEmpty);
    if (!immediate &&
        active == _active &&
        compact == _compact &&
        text == query &&
        engine == this.engine) {
      return;
    }
    _timer?.cancel();
    closeMenu(refocus: false);
    final revision = ++_revision;
    _active = active;
    _compact = compact;
    query = text;
    this.engine = scope == .web ? engine : null;
    result = null;
    selectedKey = error = status = null;
    loading = active;
    notifyListeners();
    if (active) {
      unawaited(loadEngines());
      _timer = Timer(
        immediate ? Duration.zero : const Duration(milliseconds: 70),
        () => _search(revision),
      );
    }
  }

  Future<void> _search(int revision) async {
    try {
      final value = await client!.search(
        query,
        engineId: engine?.engineId,
        limit: _compact ? 8 : 24,
      );
      if (_disposed || revision != _revision) return;
      result = value;
      loading = false;
      error = null;
      notifyListeners();
      if (value.stale) {
        _timer = Timer(
          const Duration(milliseconds: 350),
          () => _search(revision),
        );
      }
    } on Object catch (failure) {
      if (_disposed || revision != _revision) return;
      loading = false;
      error = failure is WebException
          ? failure.message
          : 'Web search is unavailable.';
      notifyListeners();
    }
  }

  void retry() {
    if (!_active) return;
    _timer?.cancel();
    error = null;
    loading = true;
    notifyListeners();
    unawaited(_search(++_revision));
  }

  String? completionSuffix(String text) {
    final completion = result?.input.completion?.suggestedUrl;
    if (completion == null || text != query || text.isEmpty) return null;
    final display = text.contains('://')
        ? completion
        : completion.replaceFirst(RegExp(r'^https?://'), '');
    if (display.length <= text.length ||
        !display.toLowerCase().startsWith(text.toLowerCase())) {
      return null;
    }
    return display.substring(text.length);
  }

  void select(WebChoice choice) {
    if (selectedKey == choice.key) return;
    selectedKey = choice.key;
    notifyListeners();
  }

  bool move(int delta) {
    final choices = layout.choices;
    final current = selected;
    final index = choices.indexWhere((item) => item.key == current?.key);
    if (index < 0 || index + delta < 0 || index + delta >= choices.length) {
      return false;
    }
    select(choices[index + delta]);
    return true;
  }

  Future<Uint8List?> favicon(WebChoice choice) {
    if (choice.profileIds.isEmpty) return Future.value();
    final revision = _revision;
    // Keep only one icon request ahead of keyboard searches on the sidecar pipe.
    final next = _iconQueue.then((_) async {
      for (final id in choice.profileIds) {
        if (_disposed || revision != _revision) return null;
        try {
          final bytes = await client?.favicon(choice.url, profileId: id);
          if (bytes != null) return revision == _revision ? bytes : null;
        } on Object {
          // Another enabled source may have an icon for this URL.
        }
      }
      return null;
    });
    _iconQueue = next.then((_) {});
    return next;
  }

  WebChoice? get searchInstead =>
      layout.choices.where((c) => c.isSearch).firstOrNull;

  Future<void> open(WebChoice choice) =>
      run(choice, choice.isSearch ? 'search' : 'open.default');

  Future<void> openSettings() async {
    try {
      await Process.start(Platform.resolvedExecutable, [
        '--settings',
        'web',
      ], mode: .detached);
      await onClose();
    } on Object {
      if (!_disposed) {
        status = 'Could not open Web settings.';
        notifyListeners();
      }
    }
  }

  Future<void> openMenu(WebChoice choice) async {
    if (choice.isSearch) return;
    final revision = ++_menuRevision;
    menuChoice = choice;
    menuIndex = 0;
    menuFilter.clear();
    _actions = const [
      WebAction(action: 'open.default', label: 'Open', enabled: true),
      WebAction(action: 'copy.url', label: 'Copy URL', enabled: true),
    ];
    notifyListeners();
    menuFocus.requestFocus();
    try {
      final id = choice.entryId;
      final actions = id != null
          ? await client!.actions(id)
          : await _directActions();
      if (_disposed || revision != _menuRevision) return;
      if (actions.isNotEmpty) _actions = actions;
      notifyListeners();
    } on Object {
      // Opening and copying are still available when action discovery fails.
    }
  }

  Future<List<WebAction>> _directActions() async {
    final capabilities = await client!.capabilities();
    return [
      const WebAction(action: 'open.default', label: 'Open'),
      WebAction(
        action: 'open.private',
        label: 'Open in private window',
        enabled: capabilities.defaultPrivate,
      ),
      for (final browser in capabilities.browsers)
        if (browser.canOpen)
          WebAction(
            action: 'open.browser',
            label: 'Open in ${browser.label}',
            browserId: browser.browserId,
          ),
      const WebAction(action: 'copy.url', label: 'Copy URL'),
      const WebAction(action: 'search.site', label: 'Search this site'),
    ];
  }

  void closeMenu({bool refocus = true}) {
    if (menuChoice == null) return;
    _menuRevision++;
    menuChoice = null;
    _actions = const [];
    if (refocus) onSearchFocus();
    notifyListeners();
  }

  Iterable<WebAction> get _allActions sync* {
    for (final action in _actions) {
      yield action;
      if (action.action == 'copy.url') {
        yield const WebAction(
          action: 'copy.markdown',
          label: 'Copy as Markdown link',
          enabled: true,
        );
      }
    }
  }

  List<WebAction> get menuActions {
    final filter = menuFilter.text.toLowerCase();
    return _allActions
        .where((action) => action.label.toLowerCase().contains(filter))
        .toList(growable: false);
  }

  void filterMenu() {
    menuIndex = 0;
    notifyListeners();
  }

  void moveMenu(int delta) {
    final actions = menuActions;
    if (actions.isEmpty) return;
    final start = menuIndex.clamp(0, actions.length - 1);
    for (var i = start + delta; i >= 0 && i < actions.length; i += delta) {
      if (!actions[i].enabled) continue;
      menuIndex = i;
      notifyListeners();
      return;
    }
  }

  Future<void> run(WebChoice choice, String action, {String? browserId}) async {
    if (_busy || _disposed) return;
    _busy = true;
    closeMenu();
    status = null;
    notifyListeners();
    try {
      if (action == 'copy.url' || action == 'copy.markdown') {
        final title = choice.title
            .replaceAll(RegExp(r'[\r\n]'), ' ')
            .replaceAll('\\', '\\\\')
            .replaceAll('[', r'\[')
            .replaceAll(']', r'\]');
        final text = action == 'copy.url'
            ? choice.url
            : '[$title](<${choice.url.replaceAll('<', '%3C').replaceAll('>', '%3E')}>)';
        await Clipboard.setData(ClipboardData(text: text));
        status = action == 'copy.url' ? 'Copied URL.' : 'Copied Markdown link.';
        return;
      }

      final localHide =
          action == 'forget.localPage' || action == 'forget.localSite';
      if (localHide &&
          !await confirmForget(choice, action == 'forget.localSite')) {
        return;
      }
      if (_disposed) return;
      await client!.perform(
        action,
        entryId: choice.entryId,
        url: choice.entryId == null ? choice.url : null,
        engineId: choice.engineId,
        browserId: browserId,
      );
      if (_disposed) return;
      if (localHide || action == 'pin.local') {
        status = localHide ? 'Hidden from Launcher.' : 'Updated pinned sites.';
        retry();
      } else {
        await onClose();
      }
    } on Object catch (failure) {
      status = failure is WebException
          ? failure.message
          : 'Could not complete the Web action.';
    } finally {
      _busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _timer?.cancel();
    menuFilter.dispose();
    menuFocus.dispose();
    unawaited(client?.close());
    super.dispose();
  }
}

List<String> webActionKeys(String action) => switch (action) {
  'open.default' => const ['↵'],
  'open.private' => const ['Ctrl', '⇧', '↵'],
  'open.browser' => const ['Ctrl', 'O'],
  'copy.url' => const ['Ctrl', 'C'],
  'copy.markdown' => const ['Ctrl', 'M'],
  'search.site' => const ['Tab'],
  'bookmark.browser' => const ['Ctrl', 'B'],
  'pin.local' => const ['Ctrl', 'D'],
  'forget.localPage' => const ['⇧', 'Del'],
  'forget.localSite' => const ['Ctrl', '⇧', 'Del'],
  _ => const [],
};

class WebActionsMenu extends StatelessWidget {
  const WebActionsMenu(
    this.controller, {
    required this.onKey,
    required this.onAction,
    super.key,
  });

  final WebSearchController controller;
  final FocusOnKeyEventCallback onKey;
  final ValueChanged<WebAction> onAction;

  @override
  Widget build(BuildContext context) {
    final choice = controller.menuChoice!;
    final actions = controller.menuActions;
    return Focus(
      onKeyEvent: onKey,
      child: LauncherActionsMenu(
        title: choice.title,
        filter: TextField(
          focusNode: controller.menuFocus,
          controller: controller.menuFilter,
          onChanged: (_) => controller.filterMenu(),
          decoration: InputDecoration(
            hintText: choice.title,
            border: .none,
            isCollapsed: true,
          ),
          style: Theme.of(context).textTheme.bodySmall,
        ),
        actions: [
          for (final action in actions)
            LauncherAction(
              action.enabled ? action.label : '${action.label} (unavailable)',
              switch (action.action) {
                'open.private' => .lock,
                'open.browser' => .swap,
                'copy.url' => .copy,
                'copy.markdown' => .text,
                'search.site' => .search,
                'bookmark.browser' => .bookmark,
                'pin.local' => .star,
                'forget.localPage' || 'forget.localSite' => .trash,
                _ => .openNew,
              },
              webActionKeys(action.action),
              destructive: action.action.startsWith('forget.'),
              onPressed: action.enabled ? () => onAction(action) : null,
            ),
        ],
        selectedIndex: controller.menuIndex,
      ),
    );
  }
}

LauncherFooter webFooter(WebSearchController web, {required bool singleLine}) =>
    LauncherFooter(
      singleLine: singleLine,
      leading: web.menuChoice != null
          ? const [
              LauncherKeyHint(['↑', '↓'], label: 'Choose'),
              LauncherKeyHint(['↵'], label: 'Run action'),
              LauncherKeyHint(['Esc'], label: 'Back to results'),
            ]
          : [
              if (web.result?.input.completion != null)
                const LauncherKeyHint(['Tab'], label: 'Accept'),
              if (web.query.isEmpty && web.engine == null)
                const LauncherKeyHint(['↑', '↓'], label: 'Navigate'),
              if (web.selected != null)
                LauncherKeyHint(const [
                  '↵',
                ], label: web.selected!.isSearch ? 'Search' : 'Open'),
              if (web.engine != null)
                const LauncherKeyHint(['Tab'], label: 'Next engine')
              else if (web.searchInstead != null)
                const LauncherKeyHint(['Ctrl', '↵'], label: 'Search instead'),
              if (web.selected case final choice? when !choice.isSearch)
                const LauncherKeyHint(['Ctrl', 'C'], label: 'Copy URL'),
            ],
      trailing: [
        if (web.menuChoice == null && web.selected?.entryId != null)
          const LauncherKeyHint(['Ctrl', 'K'], label: 'Actions')
        else if (web.menuChoice == null &&
            web.query.isEmpty &&
            web.engine == null)
          const LauncherKeyHint(['?'], label: 'Engine keywords'),
      ],
    );
