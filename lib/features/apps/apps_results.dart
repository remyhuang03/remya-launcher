import 'dart:async';
import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import '../launcher/launcher_details.dart';
import '../launcher/launcher_panel.dart';
import '../launcher/launcher_results.dart';
import 'apps_client.dart';
import 'apps_icon.dart';
import 'apps_search_controller.dart';

typedef _ResultBlock = (double, Widget Function(), bool);

class AppsResults extends StatefulWidget {
  const AppsResults(
    this.controller, {
    required this.tileColumns,
    this.selectionVisible = true,
    super.key,
  });

  final AppsSearchController controller;
  final int tileColumns;
  final bool selectionVisible;

  @override
  State<AppsResults> createState() => _AppsResultsState();
}

class _AppsResultsState extends State<AppsResults> {
  final _inlineSelection = GlobalKey();
  (int, bool)? _lastScroll;

  @override
  Widget build(BuildContext context) {
    final c = widget.controller;
    if (!c.available) return const SizedBox.shrink();
    if (c.error case final error?) {
      return c.inApps
          ? Padding(
              padding: const .all(12),
              child: Row(
                spacing: 8,
                children: [
                  Expanded(child: Text(error)),
                  TextButton(
                    onPressed: () => c.update(.apps, c.query, immediate: true),
                    child: const Text('Retry'),
                  ),
                ],
              ),
            )
          : const SizedBox.shrink();
    }
    if (c.loading && c.result.total == 0) {
      return c.inApps
          ? const LinearProgressIndicator(semanticsLabel: 'Loading apps')
          : const SizedBox.shrink();
    }

    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final blocks = <_ResultBlock>[];
    void add(double height, Widget Function() build, {bool selected = false}) =>
        blocks.add((height, build, selected));
    void header(String name, {String? meta, bool child = false}) => add(
      28,
      () => _Indented(
        enabled: child,
        child: LauncherSectionHeader(
          name,
          height: 28,
          trailing: meta == null ? null : _Meta(meta, small: true),
        ),
      ),
    );
    void row(AppChoice choice, {bool child = false}) => add(
      58,
      () => _Indented(
        enabled: child,
        child: Padding(
          padding: const .only(bottom: 2),
          child: _AppResultRow(
            c,
            choice,
            selectionVisible: widget.selectionVisible,
          ),
        ),
      ),
      selected: widget.selectionVisible && c.isSelected(choice),
    );

    final tiles = c.tiles;
    if (tiles.isNotEmpty) {
      add(
        28,
        () => LauncherSectionHeader(
          c.inApps ? 'FREQUENT' : 'PINNED',
          height: 28,
          trailing: Row(
            spacing: 4,
            children: [
              const _Meta('Alt +', small: true),
              const LauncherKeycap('1'),
              if (tiles.length > 1) ...[
                const _Meta('…', small: true),
                LauncherKeycap('${tiles.length.clamp(1, 9)}'),
              ],
            ],
          ),
        ),
      );
      final columns = widget.tileColumns;
      for (var start = 0; start < tiles.length; start += columns) {
        final offset = start;
        final end = math.min(start + columns, tiles.length);
        add(
          96,
          () => Padding(
            padding: const .fromLTRB(4, 4, 4, 4),
            child: Row(
              spacing: 8,
              children: [
                for (var i = offset; i < offset + columns; i++)
                  Expanded(
                    child: i < end
                        ? _PinnedApp(c, tiles[i], i + 1)
                        : const SizedBox.shrink(),
                  ),
              ],
            ),
          ),
          selected: tiles
              .sublist(offset, end)
              .any((app) => c.isSelected(AppChoice(app, frequent: true))),
        );
      }
    }
    if (c.apps.isNotEmpty) {
      header(
        c.result.hidden
            ? 'HIDDEN'
            : c.empty
            ? 'ALL APPS'
            : 'APPS',
        meta: !c.inApps
            ? null
            : c.result.hidden
            ? 'Ctrl H to show'
            : c.expandedId != null
            ? '${c.result.matched} of ${c.result.total}'
            : c.empty
            ? '${c.result.total} · by name'
            : 'name › kind › keywords',
      );
      for (final app in c.apps) {
        row(AppChoice(app));
        if (c.expandedId != app.id || app.actionId != null) continue;
        if (app.windows.isNotEmpty) {
          header('OPEN WINDOWS', meta: 'Hyprland', child: true);
          for (final window in app.windows) {
            row(AppChoice(app, window: window), child: true);
          }
        }
        if (app.actions.isNotEmpty) {
          header('DESKTOP ACTIONS', meta: app.id, child: true);
          for (final action in app.actions) {
            row(AppChoice(app, action: action), child: true);
          }
        }
      }
    }
    if (c.elsewhere && c.ready) {
      add(
        46,
        () => Container(
          padding: const .symmetric(horizontal: 16),
          decoration: BoxDecoration(
            color: colors.surfaceContainerHighest,
            borderRadius: .circular(20),
          ),
          child: Row(
            spacing: 12,
            children: [
              const LauncherIcon(.apps, size: 18),
              Expanded(
                child: Text('No installed app matches “${c.query.trim()}”'),
              ),
              if (widget.tileColumns == 6)
                const _Meta('Checked names, kinds, keywords and hidden apps'),
            ],
          ),
        ),
      );
      if (c.result.packages.isNotEmpty) {
        header('INSTALL', meta: 'local package databases · offline');
        for (final package in c.result.packages) {
          row(AppChoice.install(package));
        }
      }
      add(
        28,
        () => const LauncherSectionHeader(
          'ELSEWHERE',
          height: 28,
          trailing: LauncherKeyHint(['Ctrl', '↵']),
        ),
      );
      row(const AppChoice.web());
    }
    if (blocks.isEmpty) {
      if (!c.inApps) return const SizedBox.shrink();
      add(
        44,
        () => Padding(
          padding: const .all(12),
          child: Text(
            c.empty ? 'No visible applications.' : 'No matching apps.',
          ),
        ),
      );
    }

    final scroll = (c.selectionRevision, widget.selectionVisible);
    if (scroll != _lastScroll) {
      _lastScroll = scroll;
      var start = 0.0;
      for (final (height, _, selected) in blocks) {
        if (selected) {
          final target = start;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (!mounted || _lastScroll != scroll) return;
            if (!c.inApps) {
              if (_inlineSelection.currentContext case final context?) {
                Scrollable.ensureVisible(
                  context,
                  alignmentPolicy: .keepVisibleAtStart,
                );
                Scrollable.ensureVisible(
                  context,
                  alignmentPolicy: .keepVisibleAtEnd,
                );
              }
              return;
            }
            if (!c.scroll.hasClients) return;
            final position = c.scroll.position;
            final end = target + height;
            final visibleTop = position.pixels;
            final visibleBottom = position.pixels + position.viewportDimension;
            if (target < visibleTop || end > visibleBottom) {
              final next = target < visibleTop
                  ? target
                  : end - position.viewportDimension;
              c.scroll.jumpTo(next.clamp(0.0, position.maxScrollExtent));
            }
          });
          break;
        }
        start += height;
      }
    }

    if (!c.inApps) {
      return Column(
        crossAxisAlignment: .stretch,
        children: [
          for (final (height, build, selected) in blocks)
            SizedBox(
              key: selected ? _inlineSelection : null,
              height: height,
              child: build(),
            ),
        ],
      );
    }
    final height = blocks.fold(0.0, (sum, block) => sum + block.$1);
    return SizedBox(
      height: math.max(height, c.menuApp == null ? 0 : 408),
      child: ListView.builder(
        controller: c.scroll,
        padding: .zero,
        itemCount: blocks.length,
        itemExtentBuilder: (index, _) => blocks[index].$1,
        itemBuilder: (_, index) => blocks[index].$2(),
      ),
    );
  }
}

class _PinnedApp extends StatelessWidget {
  const _PinnedApp(this.controller, this.app, this.index);

  final AppsSearchController controller;
  final LauncherAppEntry app;
  final int index;

  @override
  Widget build(BuildContext context) {
    final choice = AppChoice(app, frequent: true);
    return LauncherPinnedTile(
      label: app.name,
      icon: AppsIcon.app(app, size: 36),
      shortcut: index <= 9 ? '$index' : '',
      selected: controller.isSelected(choice),
      onPressed: () {
        controller.select(choice);
        unawaited(controller.run(choice));
      },
    );
  }
}

class _Indented extends StatelessWidget {
  const _Indented({required this.enabled, required this.child});

  final bool enabled;
  final Widget child;

  @override
  Widget build(BuildContext context) => !enabled
      ? child
      : Padding(
          padding: const .only(left: 27),
          child: DecoratedBox(
            decoration: BoxDecoration(
              border: Border(
                left: BorderSide(
                  width: 2,
                  color: Theme.of(context).colorScheme.outlineVariant,
                ),
              ),
            ),
            child: Padding(padding: const .only(left: 14), child: child),
          ),
        );
}

class _AppResultRow extends StatelessWidget {
  const _AppResultRow(
    this.controller,
    this.choice, {
    required this.selectionVisible,
  });

  final AppsSearchController controller;
  final AppChoice choice;
  final bool selectionVisible;

  @override
  Widget build(BuildContext context) {
    final c = controller;
    final colors = Theme.of(context).colorScheme;
    final selected = selectionVisible && c.isSelected(choice);
    final app = choice.app;
    final package = choice.package;
    final window = choice.window;
    final action = choice.action;
    final error = c.errorFor(choice);
    final query = c.query.trim();
    var title = '';
    var description = '';
    var meta = '';
    var primary = 'Open';
    var match = query;
    var icon = LauncherIconName.apps;
    InlineSpan? subtitle;
    if (window != null) {
      title = window.title.isEmpty ? app!.name : window.title;
      description = 'Workspace ${window.workspace}';
      meta = 'ws ${window.workspace}';
      primary = 'Focus';
      icon = .window;
      match = '';
    } else if (action != null) {
      title = action.name;
      description = 'Action ${action.id}';
      primary = 'Run';
      icon = action.id.contains('private') || action.id.contains('incognito')
          ? .lock
          : .openNew;
      match = '';
    } else if (app != null) {
      final running = app.windows.isNotEmpty;
      final directAction = app.actions
          .where((a) => a.id == app.actionId)
          .firstOrNull;
      title = directAction == null
          ? app.name
          : '${app.name} › ${directAction.name}';
      description = app.description;
      if (app.matchKind == 'kind') {
        final kind = app.matchText ?? app.description;
        subtitle = TextSpan(
          children: [
            launcherHighlight(kind, query, colors.primary),
            const TextSpan(text: ' · matched kind'),
          ],
        );
      } else if (app.matchKind == 'keyword' || app.matchKind == 'comment') {
        final matchText = app.matchText ?? '';
        subtitle = TextSpan(
          text:
              '${description.isEmpty || description == matchText ? '' : '$description · '}matched ${app.matchKind} “',
          children: [
            launcherHighlight(matchText, query, colors.primary),
            const TextSpan(text: '”'),
          ],
        );
      } else {
        final detail = directAction != null
            ? 'Action ${directAction.id}'
            : running && selected
            ? 'running on workspace ${app.windows.first.workspace}'
            : running && c.expandedId == app.id
            ? '${app.windows.length} open windows'
            : app.source == 'Flatpak'
            ? app.id.replaceFirst(RegExp(r'\.desktop$'), '')
            : app.terminal
            ? 'opens in terminal'
            : '';
        description = [
          description,
          detail,
        ].where((s) => s.isNotEmpty).join(' · ');
      }
      primary = directAction != null ? 'Run' : 'Open';
      meta = c.expandedId == app.id && directAction == null
          ? 'Expanded'
          : running
          ? 'Running · ${app.windows.length}'
          : app.source ?? (app.terminal ? 'Terminal app' : '');
    } else if (package != null) {
      title = package.name;
      description = package.description;
      meta = package.source;
      primary = 'Install';
      icon = .download;
    } else {
      title = 'Search the web for “$query”';
      description = 'DuckDuckGo';
      icon = .search;
    }

    final index = c.showIndices
        ? c.quickChoices.indexWhere((r) => r.id == choice.id)
        : -1;
    final trailing = <Widget>[
      if (index >= 0) LauncherKeycap('${index + 1}'),
      if (selected) _Meta(primary),
      if (selected) const LauncherKeycap('↵'),
      if (!selected && meta.isNotEmpty) _Meta(meta),
    ];
    return Opacity(
      opacity: app?.hidden == true && !selected ? .68 : 1,
      child: LauncherResultRow(
        title: launcherHighlight(title, match, colors.primary),
        subtitle: description,
        subtitleSpan: error == null
            ? subtitle
            : TextSpan(
                text: error,
                style: TextStyle(color: colors.error),
              ),
        leading: choice.isParent || app?.actionId != null
            ? Align(alignment: .centerLeft, child: AppsIcon.app(app!))
            : LauncherSymbolTile(icon, selected: selected),
        trailing: trailing.isEmpty
            ? null
            : Row(mainAxisSize: .min, spacing: 8, children: trailing),
        selected: selected,
        onPressed: c.busy
            ? null
            : () {
                c.select(choice);
                unawaited(c.run(choice));
              },
      ),
    );
  }
}

class _Meta extends StatelessWidget {
  const _Meta(this.text, {this.small = false});
  final String text;
  final bool small;

  @override
  Widget build(BuildContext context) => Text(
    text,
    maxLines: 1,
    overflow: .ellipsis,
    style: Theme.of(context).textTheme.bodySmall
        ?.copyWith(fontSize: small ? 11 : 12),
  );
}

class AppsActionsMenu extends StatelessWidget {
  const AppsActionsMenu(this.controller, {super.key});

  final AppsSearchController controller;

  @override
  Widget build(BuildContext context) {
    final app = controller.menuApp!;
    final textStyle = Theme.of(context).textTheme.bodyMedium
        ?.copyWith(fontSize: 13, fontWeight: .w600);
    return LauncherActionsMenu(
      title: app.name,
      filter: TextField(
        controller: controller.menuFilter,
        focusNode: controller.menuFocus,
        onChanged: (_) => controller.filterMenu(),
        style: textStyle,
        decoration: InputDecoration(
          hintText: app.name,
          hintStyle: textStyle,
          isCollapsed: true,
          border: .none,
        ),
        scrollPadding: .zero,
      ),
      actions: [
        for (final action in controller.menuActions)
          LauncherAction(
            action.label,
            action.icon,
            action.keys,
            destructive: action.danger,
            onPressed: () => unawaited(controller.runMenu(action)),
          ),
      ],
      selectedIndex: controller.menuIndex,
    );
  }
}

class AppsSearchHint extends StatelessWidget {
  const AppsSearchHint(this.controller, {super.key});

  final AppsSearchController controller;

  @override
  Widget build(BuildContext context) {
    final controller = this.controller;
    if (controller.expandedId != null) {
      return const LauncherKeyHint(['Ctrl', '←'], label: 'collapse');
    }
    return _Meta(
      controller.empty
          ? '${controller.result.total} apps'
          : '${controller.result.matched} of ${controller.result.total}',
    );
  }
}

LauncherFooter appsFooter(AppsSearchController c, {required bool singleLine}) {
  final selected = c.selected;
  if (c.status case final status?) {
    return LauncherFooter(leading: [LauncherKeyHint(const [], label: status)]);
  }
  if (c.menuApp != null) {
    return LauncherFooter(
      singleLine: singleLine,
      leading: const [
        LauncherKeyHint(['↑', '↓'], label: 'Choose'),
        LauncherKeyHint(['↵'], label: 'Run action'),
        LauncherKeyHint(['Esc'], label: 'Back to results'),
      ],
    );
  }
  if (c.expandedId != null) {
    return LauncherFooter(
      singleLine: singleLine,
      leading: const [
        LauncherKeyHint(['↑', '↓'], label: 'Choose'),
        LauncherKeyHint(['↵'], label: 'Run'),
        LauncherKeyHint(['Ctrl', '←'], label: 'Collapse'),
      ],
      trailing: const [
        LauncherKeyHint(['Esc'], label: 'Back'),
      ],
    );
  }
  if (c.elsewhere) {
    return LauncherFooter(
      singleLine: singleLine,
      leading: [
        if (selected?.package != null)
          const LauncherKeyHint(['↵'], label: 'Install in terminal'),
        const LauncherKeyHint(['Ctrl', '↵'], label: 'Search web'),
        if (selected?.package != null)
          const LauncherKeyHint(['Ctrl', 'I'], label: 'Package info'),
      ],
      trailing: const [
        LauncherKeyHint(['Esc'], label: 'Clear'),
      ],
    );
  }
  final app = selected?.app;
  return LauncherFooter(
    singleLine: singleLine,
    leading: [
      if (c.empty) const LauncherKeyHint(['↑', '↓'], label: 'Navigate'),
      if (app != null) ...[
        LauncherKeyHint(const [
          '↵',
        ], label: app.actionId != null ? 'Run' : 'Open'),
        const LauncherKeyHint(['Ctrl', '↵'], label: 'New window'),
        if (!c.empty && (app.windows.isNotEmpty || app.actions.isNotEmpty))
          const LauncherKeyHint(['Ctrl', '→'], label: 'Expand app'),
        if (app.hidden)
          const LauncherKeyHint(['Ctrl', 'H'], label: 'Show in launcher'),
      ] else
        LauncherKeyHint(const ['Esc'], label: c.empty ? 'Back' : 'Clear'),
    ],
    trailing: [
      if (selected?.isParent == true)
        const LauncherKeyHint(['Ctrl', 'K'], label: 'Actions'),
    ],
  );
}
