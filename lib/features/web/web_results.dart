import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';

import '../apps/apps_icon.dart';
import '../launcher/launcher_controls.dart';
import '../launcher/launcher_results.dart';
import 'web_client.dart';

enum _ChoiceKind { direct, frequent, recent, bookmark, history, search }

class WebChoice {
  WebChoice._(this._kind, this.item, {String? directUrl})
    : url = directUrl ?? item!.url;

  final _ChoiceKind _kind;
  final WebResult? item;
  final String url;

  String get key => '${_kind.name}:${entryId ?? url}';
  String? get entryId => isDirect ? null : item?.entryId;
  String get title => isDirect ? _displayUrl(url) : item!.title;
  String get hostname => item?.hostname.isNotEmpty == true
      ? item!.hostname
      : Uri.tryParse(url)?.host ?? '';
  String? get engineId => item?.engineId;
  bool get isDirect => _kind == .direct;
  bool get isSearch => _kind == .search;
  List<String> get profileIds => [
    for (final source in item?.sources ?? const <WebSource>[]) source.profileId,
  ];
}

class WebResultLayout {
  WebResultLayout(
    this.result, {
    required this.query,
    this.engine,
    this.engines = const [],
    this.compact = false,
  }) {
    final directUrl = result?.input.directNavigationTarget;
    final empty = query.trim().isEmpty && engine == null;
    _groups = {
      for (final group in result?.groups ?? const <WebGroup>[])
        group.kind: group.items,
    };
    bookmarks = empty ? const [] : _items('bookmarks', .bookmark);
    history = empty ? const [] : _items('history', .history);
    direct = directUrl == null
        ? null
        : WebChoice._(
            .direct,
            history
                .where((choice) => choice.url == directUrl)
                .firstOrNull
                ?.item,
            directUrl: directUrl,
          );
    final indexed = [...bookmarks, ...history];
    final seen = <String>{?directUrl};
    local = compact
        ? indexed.where((choice) => seen.add(choice.url)).take(8).toList()
        : const [];
    frequent = !compact && empty
        ? [
            for (final item in (result?.frequentSites ?? const []).take(6))
              WebChoice._(.frequent, item),
          ]
        : const [];
    recent = !compact && empty
        ? [
            for (final item in result?.recent ?? const <WebResult>[])
              WebChoice._(.recent, item),
          ]
        : const [];
    final suggestions = compact || empty
        ? const <WebChoice>[]
        : _items('search', .search);
    primaryEngine = engine == null
        ? null
        : suggestions
              .where((choice) => choice.engineId == engine!.engineId)
              .firstOrNull;
    otherEngines = suggestions
        .where((choice) => choice != primaryEngine)
        .toList();
    choices = compact
        ? [?direct, ...local]
        : [
            ...frequent,
            ...recent,
            ?direct,
            ?primaryEngine,
            ...indexed,
            ...otherEngines,
          ];
  }

  final WebSearchResult? result;
  final String query;
  final WebEngine? engine;
  final List<WebEngine> engines;
  final bool compact;
  late final Map<String, List<WebResult>> _groups;

  late final WebChoice? direct;
  late final WebChoice? primaryEngine;
  late final List<WebChoice> local, frequent, recent, otherEngines, choices;
  late final List<WebChoice> bookmarks, history;

  List<WebChoice> _items(String group, _ChoiceKind kind) => [
    for (final item in _groups[group] ?? const <WebResult>[])
      WebChoice._(kind, item),
  ];

  String get _matchQuery => result?.input.query ?? query;
}

typedef _Block = (double height, List<String> keys, Widget Function() build);

class WebResults extends StatefulWidget {
  const WebResults({
    required this.layout,
    required this.selectedKey,
    required this.onOpen,
    required this.onSelect,
    required this.favicon,
    required this.tileColumns,
    this.onSettings,
    this.loading = false,
    this.error,
    this.onRetry,
    this.scrollable = true,
    super.key,
  });

  final WebResultLayout layout;
  final String? selectedKey;
  final ValueChanged<WebChoice> onOpen;
  final ValueChanged<WebChoice> onSelect;
  final Future<Uint8List?> Function(WebChoice) favicon;
  final int tileColumns;
  final VoidCallback? onSettings;
  final bool loading;
  final String? error;
  final VoidCallback? onRetry;
  final bool scrollable;

  @override
  State<WebResults> createState() => _WebResultsState();
}

class _WebResultsState extends State<WebResults> {
  final _scroll = ScrollController();
  final _selected = GlobalKey();
  (String?, int)? _lastSelection;

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  void _open(WebChoice choice) {
    widget.onSelect(choice);
    widget.onOpen(choice);
  }

  @override
  Widget build(BuildContext context) {
    final layout = widget.layout;
    if (widget.error case final error? when !layout.compact) {
      return _message(error, 'Retry', widget.onRetry);
    }
    if (!layout.compact && widget.loading && layout.result == null) {
      return const LinearProgressIndicator(
        semanticsLabel: 'Loading Web results',
      );
    }
    final blocks = <_Block>[];
    void header(String label, {int height = 26, Widget? trailing}) {
      blocks.add((
        height.toDouble(),
        const [],
        () => LauncherSectionHeader(
          label,
          height: height.toDouble(),
          trailing: trailing,
        ),
      ));
    }

    void row(WebChoice choice, {int? shortcut}) {
      final selected = choice.key == widget.selectedKey;
      blocks.add((
        58,
        [choice.key],
        () => _WebRow(
          key: !widget.scrollable && selected
              ? _selected
              : ValueKey(choice.key),
          choice: choice,
          query: layout._matchQuery,
          engine: layout.engines
              .where((engine) => engine.engineId == choice.engineId)
              .firstOrNull,
          primaryEngine: choice == layout.primaryEngine,
          defaultEngine:
              choice == layout.otherEngines.firstOrNull &&
              layout.engine == null,
          scopedEngine: layout.engine != null,
          shortcut: shortcut,
          selected: selected,
          favicon: widget.favicon,
          onPressed: () => _open(choice),
        ),
      ));
    }

    if (layout.result?.enabledProfileCount == 0 && widget.onSettings != null) {
      blocks.add((
        56,
        const [],
        () => _message(
          'Browser history is off.',
          'Web settings',
          widget.onSettings,
        ),
      ));
    }
    if (layout.compact) {
      if (layout.choices.isEmpty && blocks.isEmpty) {
        return const SizedBox.shrink();
      }
      if (layout.direct case final direct?) row(direct);
      if (layout.local.isNotEmpty) {
        header('WEB HISTORY');
        for (final choice in layout.local) {
          row(choice);
        }
      }
      return _buildList(blocks);
    }

    final frequent = layout.frequent;
    if (frequent.isNotEmpty) {
      header(
        'FREQUENT SITES',
        height: 32,
        trailing: Row(
          spacing: 4,
          children: [
            const _Meta('Alt +'),
            const LauncherKeycap('1'),
            if (frequent.length > 1) ...[
              const _Meta('…'),
              LauncherKeycap('${frequent.length}'),
            ],
          ],
        ),
      );
      final columns = widget.tileColumns.clamp(1, 6);
      for (var start = 0; start < frequent.length; start += columns) {
        final first = start;
        final tiles = frequent.skip(start).take(columns).toList();
        blocks.add((
          96,
          [for (final tile in tiles) tile.key],
          () => Padding(
            padding: const .fromLTRB(4, 8, 4, 0),
            child: Row(
              spacing: 8,
              children: [
                for (var index = 0; index < columns; index++)
                  Expanded(
                    child: index >= tiles.length
                        ? const SizedBox.shrink()
                        : _frequentTile(tiles[index], first + index + 1),
                  ),
              ],
            ),
          ),
        ));
      }
    }
    if (layout.recent.isNotEmpty) {
      header(
        'RECENT',
        height: 32,
        trailing: const LauncherKeyHint(['⇧', 'Del'], label: 'Hide'),
      );
      for (final choice in layout.recent) {
        row(choice);
      }
    }
    if (layout.direct case final direct?) row(direct);
    if (layout.primaryEngine case final primary?) {
      final engine = layout.engine!;
      header(
        'SEARCH ${engine.label.toUpperCase()}',
        trailing: _Meta('keyword ${engine.keyword}'),
      );
      row(primary);
    }
    final bookmarks = layout.bookmarks;
    if (bookmarks.isNotEmpty) {
      final labels = {
        for (final summary in layout.result!.sourceSummary)
          if (bookmarks.any(
            (choice) => choice.item!.sources.any(
              (source) => source.browserId == summary.browserId,
            ),
          ))
            summary.label,
      };
      header(
        'BOOKMARKS',
        trailing: labels.length == 1 ? _Meta(labels.single) : null,
      );
      for (final choice in bookmarks) {
        row(choice);
      }
    }
    final history = layout.history;
    if (history.isNotEmpty) {
      final host = layout.direct?.hostname;
      final onSite =
          host != null &&
          host.isNotEmpty &&
          history.every((choice) => choice.hostname == host);
      final engineHost = layout.engine?.hostname ?? '';
      header(
        engineHost.isNotEmpty
            ? 'HISTORY ON $engineHost'
            : onSite
            ? 'ON $host'
            : 'HISTORY',
        trailing: layout.engine != null
            ? null
            : onSite
            ? const LauncherKeyHint(['Tab'], label: 'Search this site')
            : const _Meta('frecency'),
      );
      for (final choice in history) {
        row(choice);
      }
    }
    final other = layout.otherEngines;
    if (other.isNotEmpty) {
      final shortcuts = other.length.clamp(1, 6);
      header(
        layout.engine == null ? 'SEARCH' : 'OTHER ENGINES',
        height: 32,
        trailing: layout.engine == null
            ? const LauncherKeyHint(['Ctrl', '↵'])
            : LauncherKeyHint(['Alt', shortcuts == 1 ? '1' : '1–$shortcuts']),
      );
      for (final (index, choice) in other.indexed) {
        row(
          choice,
          shortcut: layout.engine != null && index < shortcuts
              ? index + 1
              : null,
        );
      }
    }
    if (blocks.isEmpty) {
      return layout.result?.stale == true
          ? const LinearProgressIndicator(
              semanticsLabel: 'Indexing browser profiles',
            )
          : layout.engine != null
          ? _message(
              'Type a query to search ${layout.engine!.label}.',
              '',
              null,
            )
          : _message(
              'No recent pages or bookmarks.',
              'Web settings',
              widget.onSettings,
            );
    }
    return _buildList(blocks);
  }

  Widget _frequentTile(WebChoice choice, int index) {
    final selected = choice.key == widget.selectedKey;
    return LauncherPinnedTile(
      key: !widget.scrollable && selected ? _selected : ValueKey(choice.key),
      label: choice.title,
      icon: _WebFavicon(choice: choice, favicon: widget.favicon, size: 36),
      shortcut: '$index',
      selected: selected,
      onPressed: () => _open(choice),
    );
  }

  Widget _buildList(List<_Block> blocks) {
    final selectedKey = widget.selectedKey;
    final index = blocks.indexWhere((block) => block.$2.contains(selectedKey));
    final selection = (selectedKey, index);
    if (selection != _lastSelection) {
      final upward = index < (_lastSelection?.$2 ?? -1);
      _lastSelection = selection;
      if (index >= 0) {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (!mounted || _lastSelection != selection) return;
          if (!widget.scrollable) {
            if (_selected.currentContext case final context?) {
              Scrollable.ensureVisible(
                context,
                alignmentPolicy: upward
                    ? .keepVisibleAtStart
                    : .keepVisibleAtEnd,
              );
            }
            return;
          }
          if (!_scroll.hasClients) return;
          final top = blocks
              .take(index)
              .fold(0.0, (sum, block) => sum + block.$1);
          final position = _scroll.position;
          final bottom = top + blocks[index].$1;
          if (top < position.pixels ||
              bottom > position.pixels + position.viewportDimension) {
            final target = top < position.pixels
                ? top
                : bottom - position.viewportDimension;
            _scroll.jumpTo(target.clamp(0.0, position.maxScrollExtent));
          }
        });
      }
    }
    if (!widget.scrollable) {
      return Column(
        crossAxisAlignment: .stretch,
        children: [
          for (final (height, _, build) in blocks)
            SizedBox(height: height, child: build()),
        ],
      );
    }
    return SizedBox(
      height: blocks.fold<double>(0, (sum, block) => sum + block.$1),
      child: ListView.builder(
        controller: _scroll,
        padding: .zero,
        itemCount: blocks.length,
        itemExtentBuilder: (index, _) => blocks[index].$1,
        itemBuilder: (_, index) => blocks[index].$3(),
      ),
    );
  }

  Widget _message(String message, String action, VoidCallback? onPressed) =>
      Padding(
        padding: const .all(12),
        child: Row(
          spacing: 8,
          children: [
            Expanded(child: Text(message)),
            if (onPressed != null)
              TextButton(onPressed: onPressed, child: Text(action)),
          ],
        ),
      );
}

class _WebRow extends StatelessWidget {
  const _WebRow({
    required this.choice,
    required this.query,
    required this.engine,
    required this.primaryEngine,
    required this.defaultEngine,
    required this.scopedEngine,
    required this.shortcut,
    required this.selected,
    required this.favicon,
    required this.onPressed,
    super.key,
  });

  final WebChoice choice;
  final String query;
  final WebEngine? engine;
  final bool primaryEngine;
  final bool defaultEngine;
  final bool scopedEngine;
  final int? shortcut;
  final bool selected;
  final Future<Uint8List?> Function(WebChoice) favicon;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final url = _displayUrl(choice.url);
    final item = choice.item;
    final keyword = engine?.keyword;
    final visited = _timeAgo(item?.lastVisitedMs);
    final subtitle = switch (choice._kind) {
      .bookmark => [
        url,
        'Bookmarks',
        if (item!.folderPath.isNotEmpty) item.folderPath.join(' › '),
      ].join(' · '),
      .direct => [
        Uri.tryParse(choice.url)?.scheme == 'http' ? 'http://' : 'https://',
        if (item != null && item.visitedCount > 0)
          'visited ${item.visitedCount} ${item.visitedCount == 1 ? 'time' : 'times'}',
        ?visited,
      ].join(' · '),
      .search when primaryEngine => url,
      .search when defaultEngine => 'Default engine',
      .search when keyword != null =>
        scopedEngine
            ? 'Keyword $keyword'
            : 'Keyword $keyword · or type “$keyword $query”',
      _ => url,
    };
    final meta = switch (choice._kind) {
      .recent => visited,
      .history when (item?.visitedCount ?? 0) > 1 =>
        '${item!.visitedCount} visits',
      .history => visited,
      .search when !defaultEngine && !primaryEngine => keyword,
      _ => null,
    };
    final accent = Theme.of(context).colorScheme.primary;
    return Padding(
      padding: const .only(bottom: 2),
      child: LauncherResultRow(
        title: choice.isSearch
            ? TextSpan(text: choice.title)
            : launcherHighlight(choice.title, query, accent),
        subtitle: subtitle,
        subtitleSpan: choice._kind == .history
            ? launcherHighlight(subtitle, query, accent)
            : null,
        leading: switch (choice._kind) {
          .direct => const LauncherSymbolTile(.openNew),
          .search => const LauncherSymbolTile(.search),
          _ => Center(
            child: _WebFavicon(choice: choice, favicon: favicon),
          ),
        },
        trailing: selected
            ? Row(
                mainAxisSize: .min,
                spacing: 6,
                children: [
                  _Meta(choice.isSearch ? 'Search' : 'Open'),
                  const LauncherKeycap('↵'),
                ],
              )
            : shortcut != null
            ? LauncherKeycap('$shortcut')
            : meta == null
            ? null
            : _Meta(meta),
        selected: selected,
        onPressed: onPressed,
      ),
    );
  }
}

class _WebFavicon extends StatefulWidget {
  const _WebFavicon({
    required this.choice,
    required this.favicon,
    this.size = 32,
  });

  final WebChoice choice;
  final Future<Uint8List?> Function(WebChoice) favicon;
  final double size;

  @override
  State<_WebFavicon> createState() => _WebFaviconState();
}

class _WebFaviconState extends State<_WebFavicon> {
  Future<Uint8List?>? _bytes;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(_WebFavicon oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.choice.item != oldWidget.choice.item) _load();
  }

  void _load() {
    final choice = widget.choice;
    _bytes = choice.profileIds.isEmpty ? null : widget.favicon(choice);
  }

  @override
  Widget build(BuildContext context) {
    final choice = widget.choice;
    final fallbackText = choice.item?.favicon.fallbackText;
    final fallback = AppsIcon(
      id: choice.hostname,
      name: fallbackText == null || fallbackText.isEmpty
          ? choice.title
          : fallbackText,
      size: widget.size,
    );
    return FutureBuilder<Uint8List?>(
      future: _bytes,
      builder: (_, snapshot) {
        final bytes = snapshot.connectionState == .done ? snapshot.data : null;
        return bytes == null
            ? fallback
            : Image.memory(
                bytes,
                width: widget.size,
                height: widget.size,
                fit: .contain,
                excludeFromSemantics: true,
                errorBuilder: (_, _, _) => fallback,
              );
      },
    );
  }
}

class _Meta extends StatelessWidget {
  const _Meta(this.text);
  final String text;

  @override
  Widget build(BuildContext context) => Text(
    text,
    maxLines: 1,
    overflow: .ellipsis,
    style: Theme.of(context).textTheme.bodySmall,
  );
}

String _displayUrl(String url) => url.replaceFirst(RegExp(r'^https?://'), '');

String? _timeAgo(int? millis) {
  if (millis == null || millis <= 0 || millis > 8640000000000000) return null;
  final time = DateTime.fromMillisecondsSinceEpoch(millis);
  final elapsed = DateTime.now().difference(time);
  return switch (elapsed.inMinutes) {
    < 1 => 'Just now',
    < 60 => '${elapsed.inMinutes} min ago',
    < 1440 => '${elapsed.inHours} h ago',
    < 2880 => 'Yesterday',
    < 10080 => '${elapsed.inDays} days ago',
    < 20160 => 'Last week',
    _ =>
      '${time.year}-${time.month.toString().padLeft(2, '0')}-${time.day.toString().padLeft(2, '0')}',
  };
}
