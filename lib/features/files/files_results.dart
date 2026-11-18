import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import '../launcher/launcher_results.dart';
import 'file_icon_tile.dart';
import 'files_drag.dart';
import 'files_models.dart';

const _maxScrollHeight = 390.0;
const _previewWidth = 248.0;
const _previewGap = 8.0;
const _previewMinWidth = 550.0;
const _minPreviewHeight = 280.0;

/// Formats a byte count into a compact human-readable size string.
String filesSize(int bytes) {
  if (bytes < 0) return '0 B';
  if (bytes < 1024) return '$bytes B';
  const units = ['KB', 'MB', 'GB', 'TB'];
  var value = bytes / 1024;
  var unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex++;
  }
  return '${value.toStringAsFixed(1)} ${units[unitIndex]}';
}

/// Formats a Unix timestamp in seconds into a concise relative modification time.
String filesModified(int unixSeconds) {
  if (unixSeconds <= 0 || unixSeconds > 8640000000000) return '';
  final time = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  final elapsed = DateTime.now().difference(time);
  if (elapsed.isNegative || elapsed.inMinutes < 1) return 'just now';
  if (elapsed.inMinutes < 60) return '${elapsed.inMinutes} min ago';
  if (elapsed.inHours < 24) return '${elapsed.inHours} h ago';
  if (elapsed.inDays < 2) return 'yesterday';
  if (elapsed.inDays < 7) return '${elapsed.inDays} days ago';
  if (elapsed.inDays < 14) return 'last week';
  final month = time.month.toString().padLeft(2, '0');
  final day = time.day.toString().padLeft(2, '0');
  return '${time.year}-$month-$day';
}

typedef _ResultBlock = ({
  double height,
  List<String> keys,
  Widget Function() build,
});

/// Presentational results and optional preview pane for launcher file search.
class FilesResults extends StatefulWidget {
  const FilesResults({
    required this.groups,
    required this.selectedKey,
    required this.query,
    required this.onActivate,
    required this.onSelect,
    this.showIndexes = false,
    this.compact = false,
    this.scrollable = true,
    this.loading = false,
    this.diagnostic,
    this.createWritable = true,
    this.preview,
    this.onRetry,
    this.onDrag,
    super.key,
  });

  final List<FileGroup> groups;
  final String? selectedKey;
  final String query;
  final void Function(FileChoice) onActivate;
  final void Function(FileChoice) onSelect;
  final bool showIndexes;
  final bool compact;
  final bool scrollable;
  final bool loading;
  final String? diagnostic;
  final bool createWritable;
  final Widget? preview;
  final VoidCallback? onRetry;
  final void Function(FileEntry)? onDrag;

  @override
  State<FilesResults> createState() => _FilesResultsState();
}

class _FilesResultsState extends State<FilesResults> {
  final _scroll = ScrollController();
  final _selectedGlobalKey = GlobalKey();
  String? _lastSelectedKey;
  String? _hoverSelectedKey;
  Offset? _lastPointerPosition;
  List<_ResultBlock> _blocks = const [];

  @override
  void initState() {
    super.initState();
    _lastSelectedKey = widget.selectedKey;
  }

  @override
  void didUpdateWidget(covariant FilesResults oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.selectedKey != _lastSelectedKey) {
      final targetKey = widget.selectedKey;
      _lastSelectedKey = targetKey;
      if (targetKey != null && targetKey == _hoverSelectedKey) {
        _hoverSelectedKey = null;
        return;
      }
      _hoverSelectedKey = null;
      if (targetKey != null) {
        _scheduleEnsureVisible(targetKey);
      }
    }
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  void _scheduleEnsureVisible(String targetKey) {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || widget.selectedKey != targetKey) return;
      if (_selectedGlobalKey.currentContext case final ctx?) {
        Scrollable.ensureVisible(ctx, alignmentPolicy: .keepVisibleAtStart);
        Scrollable.ensureVisible(ctx, alignmentPolicy: .keepVisibleAtEnd);
        return;
      }
      if (!widget.scrollable || !_scroll.hasClients) return;
      final blockIndex = _blocks.indexWhere(
        (block) => block.keys.contains(targetKey),
      );
      if (blockIndex < 0) return;
      final top = _blocks
          .take(blockIndex)
          .fold<double>(0, (sum, block) => sum + block.height);
      final bottom = top + _blocks[blockIndex].height;
      final position = _scroll.position;
      if (top < position.pixels ||
          bottom > position.pixels + position.viewportDimension) {
        final next = top < position.pixels
            ? top
            : bottom - position.viewportDimension;
        _scroll.jumpTo(next.clamp(0.0, position.maxScrollExtent));
      }
    });
  }

  void _activate(FileChoice choice) {
    widget.onSelect(choice);
    widget.onActivate(choice);
  }

  void _hoverSelect(FileChoice choice, Offset position) {
    if (_lastPointerPosition == position) return;
    _lastPointerPosition = position;
    if (!_isInteractive(choice) || choice.key == widget.selectedKey) return;
    _hoverSelectedKey = choice.key;
    widget.onSelect(choice);
  }

  bool _isInteractive(FileChoice choice) => switch (choice.kind) {
    .createFile || .createDirectory => widget.createWritable,
    _ => true,
  };

  VoidCallback? _dragCallback(FileChoice choice, {bool enabled = true}) {
    final onDrag = widget.onDrag;
    final entry = choice.entry;
    if (!enabled ||
        onDrag == null ||
        choice.kind != .entry ||
        entry == null ||
        entry.line != null ||
        entry.completion == null) {
      return null;
    }
    return () => onDrag(entry);
  }

  @override
  Widget build(BuildContext context) {
    final diagnostic = widget.diagnostic?.trim();
    final hasDiagnostic = diagnostic != null && diagnostic.isNotEmpty;
    final hasChoices = widget.groups.any((group) => group.choices.isNotEmpty);

    return LayoutBuilder(
      builder: (context, constraints) {
        final availableWidth = constraints.maxWidth.isFinite
            ? constraints.maxWidth
            : 720.0;

        if (!widget.scrollable) {
          if (!hasChoices) {
            if (hasDiagnostic) {
              return _DiagnosticBanner(
                message: diagnostic,
                onRetry: widget.onRetry,
              );
            }
            if (widget.loading) {
              return const LinearProgressIndicator(
                semanticsLabel: 'Searching files',
              );
            }
            return const SizedBox.shrink();
          }
          final blocks = _buildBlocks(
            context,
            listWidth: availableWidth,
            diagnostic: hasDiagnostic ? diagnostic : null,
          );
          _blocks = blocks;
          return Column(
            mainAxisSize: .min,
            crossAxisAlignment: .stretch,
            children: [
              if (widget.loading)
                const LinearProgressIndicator(
                  minHeight: 2,
                  semanticsLabel: 'Searching files',
                ),
              for (final block in blocks)
                SizedBox(height: block.height, child: block.build()),
            ],
          );
        }

        final maxHeight = constraints.hasBoundedHeight
            ? math.max(0.0, math.min(_maxScrollHeight, constraints.maxHeight))
            : _maxScrollHeight;
        if (maxHeight <= 0) return const SizedBox.shrink();

        if (!hasChoices) {
          if (hasDiagnostic) {
            return _ConstrainedMessage(
              maxHeight: maxHeight,
              child: _DiagnosticBanner(
                message: diagnostic,
                onRetry: widget.onRetry,
              ),
            );
          }
          if (widget.loading) {
            return ConstrainedBox(
              constraints: BoxConstraints(maxHeight: maxHeight),
              child: const LinearProgressIndicator(
                semanticsLabel: 'Searching files',
              ),
            );
          }
          return _ConstrainedMessage(
            maxHeight: maxHeight,
            child: Padding(
              padding: const .all(12),
              child: Text(
                widget.query.trim().isEmpty
                    ? 'No files.'
                    : 'No matching files.',
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
          );
        }

        final showPreview =
            widget.preview != null && availableWidth >= _previewMinWidth;
        final listWidth = showPreview
            ? math.max(0.0, availableWidth - _previewWidth - _previewGap)
            : availableWidth;

        final blocks = _buildBlocks(
          context,
          listWidth: listWidth,
          diagnostic: hasDiagnostic ? diagnostic : null,
        );
        _blocks = blocks;

        final showProgress = widget.loading && maxHeight >= 12;
        final progressHeight = showProgress ? 2.0 : 0.0;
        final contentHeight =
            blocks.fold<double>(0, (sum, block) => sum + block.height) +
            progressHeight;
        final minPreview = showPreview
            ? math.min(_minPreviewHeight, maxHeight)
            : 0.0;
        final targetHeight = math.min(
          maxHeight,
          math.max(contentHeight, minPreview),
        );

        final listView = ListView.builder(
          controller: _scroll,
          padding: .zero,
          itemCount: blocks.length,
          itemExtentBuilder: (index, _) => blocks[index].height,
          itemBuilder: (_, index) => blocks[index].build(),
        );

        return SizedBox(
          height: targetHeight,
          child: Column(
            crossAxisAlignment: .stretch,
            children: [
              if (showProgress)
                const LinearProgressIndicator(
                  minHeight: 2,
                  semanticsLabel: 'Searching files',
                ),
              Expanded(
                child: showPreview
                    ? Row(
                        crossAxisAlignment: .stretch,
                        spacing: _previewGap,
                        children: [
                          Expanded(child: listView),
                          SizedBox(width: _previewWidth, child: widget.preview),
                        ],
                      )
                    : listView,
              ),
            ],
          ),
        );
      },
    );
  }

  List<_ResultBlock> _buildBlocks(
    BuildContext context, {
    required double listWidth,
    String? diagnostic,
  }) {
    final blocks = <_ResultBlock>[];
    final highlightQuery = _extractHighlightQuery(widget.query);
    final narrow = widget.compact || listWidth < 340;
    final veryNarrow = widget.compact || listWidth < 290;
    var quickIndex = 0;
    var assignedSelectedGlobalKey = false;

    Key? keyFor(FileChoice choice, bool selected) {
      if (selected && !assignedSelectedGlobalKey) {
        assignedSelectedGlobalKey = true;
        return _selectedGlobalKey;
      }
      return ValueKey(choice.key);
    }

    if (diagnostic != null) {
      blocks.add((
        height: 48,
        keys: const [],
        build: () => _DiagnosticBanner(
          message: diagnostic,
          onRetry: widget.onRetry,
          compact: true,
        ),
      ));
    }

    for (final group in widget.groups) {
      if (group.choices.isEmpty) continue;
      if (group.tiles) {
        final tiles = group.choices.take(6).toList(growable: false);
        final startIndex = quickIndex;
        quickIndex += tiles.length;
        final sixColWidth = (listWidth - 48) / 6;
        final columns = sixColWidth < 80 ? 3 : 6;
        blocks.add((
          height: 28,
          keys: const [],
          build: () => LauncherSectionHeader(
            group.title,
            height: 28,
            trailing: _sectionTrailing(
              context,
              listWidth: listWidth,
              hint: group.hint,
              tileCount: widget.showIndexes ? tiles.length : 0,
            ),
          ),
        ));
        for (var start = 0; start < tiles.length; start += columns) {
          final offset = start;
          final end = math.min(start + columns, tiles.length);
          final rowChoices = tiles.sublist(offset, end);
          blocks.add((
            height: 96,
            keys: [for (final choice in rowChoices) choice.key],
            build: () => Padding(
              padding: const .fromLTRB(4, 4, 4, 4),
              child: Row(
                spacing: 8,
                children: [
                  for (var i = 0; i < columns; i++)
                    Expanded(
                      child: i < rowChoices.length
                          ? _buildTile(
                              rowChoices[i],
                              index: startIndex + offset + i + 1,
                              selected: rowChoices[i].key == widget.selectedKey,
                              keyFor: keyFor,
                            )
                          : const SizedBox.shrink(),
                    ),
                ],
              ),
            ),
          ));
        }
        continue;
      }

      blocks.add((
        height: 28,
        keys: const [],
        build: () => LauncherSectionHeader(
          group.title,
          height: 28,
          trailing: _sectionTrailing(
            context,
            listWidth: listWidth,
            hint: group.hint,
          ),
        ),
      ));

      for (final choice in group.choices) {
        final index = ++quickIndex;
        final selected = choice.key == widget.selectedKey;
        final isHit = choice.kind == .entry && choice.entry?.line != null;
        final height = isHit ? 32.0 : 58.0;
        final indexLabel =
            widget.showIndexes &&
                !widget.groups.any((group) => group.tiles) &&
                index <= 9
            ? '$index'
            : null;
        blocks.add((
          height: height,
          keys: [choice.key],
          build: () => _buildChoiceRow(
            choice,
            selected: selected,
            key: keyFor(choice, selected),
            highlightQuery: highlightQuery,
            listWidth: listWidth,
            narrow: narrow,
            veryNarrow: veryNarrow,
            indexLabel: indexLabel,
          ),
        ));
      }
    }

    return blocks;
  }

  Widget? _sectionTrailing(
    BuildContext context, {
    required double listWidth,
    String? hint,
    int tileCount = 0,
  }) {
    if (hint != null && hint.isNotEmpty) {
      return ConstrainedBox(
        constraints: BoxConstraints(maxWidth: listWidth * 0.45),
        child: _MetaText(hint, small: true),
      );
    }
    if (tileCount > 0) {
      return Row(
        mainAxisSize: .min,
        spacing: 4,
        children: [
          const _MetaText('Alt +', small: true),
          const LauncherKeycap('1'),
          if (tileCount > 1) ...[
            const _MetaText('…', small: true),
            LauncherKeycap('${tileCount.clamp(1, 9)}'),
          ],
        ],
      );
    }
    return null;
  }

  Widget _buildTile(
    FileChoice choice, {
    required int index,
    required bool selected,
    required Key? Function(FileChoice, bool) keyFor,
  }) {
    final entry = choice.entry;
    final label = entry?.name ?? choice.label;
    final shortcut = '$index';
    final semanticLabel = entry != null
        ? 'Place ${entry.name}, ${entry.displayPath ?? entry.path}'
        : 'Place $label';
    final drag = _dragCallback(choice);
    Widget tile = LauncherPinnedTile(
      key: keyFor(choice, selected),
      label: label,
      icon: _PlaceTileIcon(choice: choice, selected: selected),
      shortcut: shortcut,
      selected: selected,
      onPressed: () => _activate(choice),
    );
    if (drag != null) {
      tile = FilesDragSource(onDrag: drag, child: tile);
    }
    return Semantics(
      container: true,
      button: true,
      selected: selected,
      enabled: true,
      label: semanticLabel,
      child: MouseRegion(
        onHover: (event) => _hoverSelect(choice, event.position),
        child: tile,
      ),
    );
  }

  Widget _buildChoiceRow(
    FileChoice choice, {
    required bool selected,
    required Key? key,
    required String highlightQuery,
    required double listWidth,
    required bool narrow,
    required bool veryNarrow,
    required String? indexLabel,
  }) {
    if (choice.kind == .entry && choice.entry?.line != null) {
      final entry = choice.entry!;
      return Padding(
        key: key,
        padding: const .only(bottom: 2),
        child: MouseRegion(
          onHover: (event) => _hoverSelect(choice, event.position),
          child: _ContentHitRow(
            entry: entry,
            highlightQuery: highlightQuery,
            selected: selected,
            listWidth: listWidth,
            indexLabel: indexLabel,
            onPressed: () => _activate(choice),
          ),
        ),
      );
    }

    final enabled = _isInteractive(choice);
    final drag = _dragCallback(choice, enabled: enabled);
    Widget row = _FileChoiceRow(
      choice: choice,
      highlightQuery: highlightQuery,
      selected: selected,
      enabled: enabled,
      narrow: narrow,
      veryNarrow: veryNarrow,
      indexLabel: indexLabel,
      onPressed: enabled ? () => _activate(choice) : null,
    );
    if (drag != null) {
      row = FilesDragSource(onDrag: drag, child: row);
    }
    return Padding(
      key: key,
      padding: const .only(bottom: 2),
      child: MouseRegion(
        onHover: enabled
            ? (event) => _hoverSelect(choice, event.position)
            : null,
        child: row,
      ),
    );
  }
}

class _FileChoiceRow extends StatelessWidget {
  const _FileChoiceRow({
    required this.choice,
    required this.highlightQuery,
    required this.selected,
    required this.enabled,
    required this.narrow,
    required this.veryNarrow,
    required this.indexLabel,
    required this.onPressed,
  });

  final FileChoice choice;
  final String highlightQuery;
  final bool selected;
  final bool enabled;
  final bool narrow;
  final bool veryNarrow;
  final String? indexLabel;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final entry = choice.entry;
    final isCreate =
        choice.kind == .createFile || choice.kind == .createDirectory;
    final effectiveSelected = selected && enabled;

    final InlineSpan title;
    final String subtitle;
    final Widget leading;
    final String? actionLabel;
    final String? meta;
    final String semanticLabel;

    switch (choice.kind) {
      case .entry:
        final item = entry!;
        title = launcherHighlight(item.title, highlightQuery, colors.primary);
        subtitle = item.location;
        leading = FileIconTile(
          basename: item.name,
          isDirectory: item.isDirectory,
          selected: effectiveSelected,
        );
        actionLabel = 'Open';
        final sizeText = item.size == null ? null : filesSize(item.size!);
        final modifiedText = item.modified == null
            ? null
            : filesModified(item.modified!);
        final parts = [
          if (sizeText != null && sizeText.isNotEmpty) sizeText,
          if (modifiedText != null && modifiedText.isNotEmpty) modifiedText,
        ];
        meta = parts.isEmpty ? null : parts.join(' · ');
        semanticLabel =
            '${item.isDirectory ? 'Folder' : 'File'} ${item.title}, ${item.location}';
      case .createFile:
      case .createDirectory:
        final isDir = choice.kind == .createDirectory;
        title = TextSpan(text: choice.label);
        subtitle = choice.detail.isNotEmpty
            ? choice.detail
            : (enabled ? '' : 'Read-only location');
        leading = _CreateSymbolTile(
          isDirectory: isDir,
          selected: effectiveSelected,
          enabled: enabled,
        );
        actionLabel = 'Create';
        meta = enabled ? 'Create' : 'Read-only';
        semanticLabel =
            '${choice.label}${subtitle.isEmpty ? '' : ', $subtitle'}${enabled ? '' : ', disabled'}';
      case .contents:
        title = TextSpan(text: choice.label);
        subtitle = choice.detail;
        leading = _MaterialSymbolTile(
          Icons.manage_search_rounded,
          selected: effectiveSelected,
        );
        actionLabel = 'Search';
        meta = null;
        semanticLabel =
            '${choice.label}${subtitle.isEmpty ? '' : ', $subtitle'}';
      case .filter:
        title = TextSpan(text: choice.label);
        subtitle = choice.detail;
        leading = _MaterialSymbolTile(
          Icons.tune_rounded,
          selected: effectiveSelected,
        );
        actionLabel = 'Apply';
        meta = null;
        semanticLabel =
            '${choice.label}${subtitle.isEmpty ? '' : ', $subtitle'}';
      case .more:
        title = TextSpan(text: choice.label);
        subtitle = choice.detail;
        leading = _MaterialSymbolTile(
          Icons.more_horiz_rounded,
          selected: effectiveSelected,
        );
        actionLabel = 'Show';
        meta = null;
        semanticLabel =
            '${choice.label}${subtitle.isEmpty ? '' : ', $subtitle'}';
    }

    final trailingChildren = <Widget>[
      if (indexLabel != null) LauncherKeycap(indexLabel!),
      if (effectiveSelected) ...[
        if (!narrow) _MetaText(actionLabel, color: colors.onSecondaryContainer),
        const LauncherKeycap('↵'),
      ] else if (!veryNarrow && meta != null && meta.isNotEmpty)
        _MetaText(meta),
    ];

    Widget row = LauncherResultRow(
      title: title,
      subtitle: subtitle,
      leading: leading,
      selected: effectiveSelected,
      trailing: trailingChildren.isEmpty
          ? null
          : Row(mainAxisSize: .min, spacing: 8, children: trailingChildren),
      onPressed: onPressed,
    );

    if (isCreate && !effectiveSelected) {
      row = DecoratedBox(
        decoration: BoxDecoration(
          borderRadius: .circular(20),
          border: Border.all(
            color: enabled
                ? colors.outlineVariant.withValues(alpha: 0.8)
                : colors.outlineVariant.withValues(alpha: 0.4),
          ),
        ),
        child: row,
      );
    }

    if (!enabled) {
      row = Opacity(opacity: 0.48, child: row);
    }

    return Semantics(
      container: true,
      button: true,
      selected: effectiveSelected,
      enabled: enabled,
      label: semanticLabel,
      child: row,
    );
  }
}

class _ContentHitRow extends StatelessWidget {
  const _ContentHitRow({
    required this.entry,
    required this.highlightQuery,
    required this.selected,
    required this.listWidth,
    required this.indexLabel,
    required this.onPressed,
  });

  final FileEntry entry;
  final String highlightQuery;
  final bool selected;
  final double listWidth;
  final String? indexLabel;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final highContrast = MediaQuery.maybeHighContrastOf(context) ?? false;
    final indent = math.min(76.0, math.max(24.0, listWidth * 0.22));
    final lineNumber = entry.line!;
    final snippet = (entry.snippet ?? entry.name).replaceAll('\t', '  ').trim();
    final shape = RoundedRectangleBorder(
      borderRadius: .circular(selected ? 10 : 14),
      side: selected && highContrast
          ? BorderSide(color: colors.onSecondaryContainer)
          : .none,
    );
    const monoStyle = TextStyle(
      fontFamily: 'JetBrains Mono',
      fontFamilyFallback: ['JetBrainsMono Nerd Font Mono', 'monospace'],
      fontSize: 12.5,
      height: 1.3,
    );

    return Semantics(
      container: true,
      button: true,
      selected: selected,
      enabled: true,
      label: 'Line $lineNumber in ${entry.name}: $snippet',
      child: Padding(
        padding: .only(left: indent),
        child: Material(
          shape: shape,
          color: selected ? colors.secondaryContainer : Colors.transparent,
          child: InkWell(
            customBorder: shape,
            onTap: onPressed,
            child: ExcludeSemantics(
              child: SizedBox(
                height: 30,
                child: Padding(
                  padding: const .symmetric(horizontal: 10),
                  child: Row(
                    spacing: 8,
                    children: [
                      ConstrainedBox(
                        constraints: const BoxConstraints(minWidth: 28),
                        child: Text(
                          '$lineNumber',
                          textAlign: .end,
                          style: monoStyle.copyWith(
                            color: selected
                                ? colors.onSecondaryContainer
                                : colors.onSurfaceVariant,
                            fontWeight: selected ? .w600 : .w400,
                          ),
                        ),
                      ),
                      Expanded(
                        child: Text.rich(
                          launcherHighlight(
                            snippet,
                            highlightQuery,
                            colors.primary,
                          ),
                          maxLines: 1,
                          overflow: .ellipsis,
                          style: monoStyle.copyWith(
                            color: selected
                                ? colors.onSecondaryContainer
                                : colors.onSurface,
                            fontWeight: selected ? .w500 : .w400,
                          ),
                        ),
                      ),
                      if (indexLabel != null) LauncherKeycap(indexLabel!),
                      if (selected) const LauncherKeycap('↵'),
                    ],
                  ),
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _PlaceTileIcon extends StatelessWidget {
  const _PlaceTileIcon({required this.choice, required this.selected});

  final FileChoice choice;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final color = selected
        ? colors.onSecondaryContainer
        : colors.onSurfaceVariant;
    final key = (choice.entry?.role ?? choice.entry?.name ?? choice.label)
        .trim()
        .toLowerCase();

    final Widget icon = switch (key) {
      'home' => Icon(Icons.home_rounded, size: 24, color: color),
      'downloads' ||
      'download' => LauncherIcon(.download, size: 24, color: color),
      'documents' ||
      'document' => LauncherIcon(.description, size: 24, color: color),
      'trash' => LauncherIcon(.trash, size: 24, color: color),
      'desktop' => Icon(Icons.desktop_windows_rounded, size: 24, color: color),
      'pictures' || 'photos' => LauncherIcon(.image, size: 24, color: color),
      'music' => Icon(Icons.music_note_rounded, size: 24, color: color),
      'videos' => Icon(Icons.video_library_rounded, size: 24, color: color),
      _ => LauncherIcon(.folder, size: 24, color: color),
    };
    return Center(child: icon);
  }
}

class _MaterialSymbolTile extends StatelessWidget {
  const _MaterialSymbolTile(this.icon, {required this.selected});

  final IconData icon;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: selected
            ? colors.surfaceContainer
            : colors.surfaceContainerHighest,
        borderRadius: .circular(12),
      ),
      child: Center(
        child: Icon(
          icon,
          size: 20,
          color: selected
              ? colors.onSecondaryContainer
              : colors.onSurfaceVariant,
        ),
      ),
    );
  }
}

class _CreateSymbolTile extends StatelessWidget {
  const _CreateSymbolTile({
    required this.isDirectory,
    required this.selected,
    required this.enabled,
  });

  final bool isDirectory;
  final bool selected;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final background = !enabled
        ? colors.surfaceContainerHighest
        : selected
        ? colors.primary
        : colors.primaryContainer;
    final foreground = !enabled
        ? colors.onSurfaceVariant
        : selected
        ? colors.onPrimary
        : colors.onPrimaryContainer;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: background,
        borderRadius: .circular(12),
        border: enabled
            ? null
            : Border.all(color: colors.outlineVariant.withValues(alpha: 0.7)),
      ),
      child: Center(
        child: Icon(
          isDirectory
              ? Icons.create_new_folder_outlined
              : Icons.note_add_outlined,
          size: 20,
          color: foreground,
        ),
      ),
    );
  }
}

class _DiagnosticBanner extends StatelessWidget {
  const _DiagnosticBanner({
    required this.message,
    this.onRetry,
    this.compact = false,
  });

  final String message;
  final VoidCallback? onRetry;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Padding(
      padding: .symmetric(horizontal: 12, vertical: compact ? 4 : 12),
      child: Row(
        spacing: 8,
        children: [
          Icon(
            Icons.info_outline_rounded,
            size: 18,
            color: colors.onSurfaceVariant,
          ),
          Expanded(
            child: Text(
              message,
              maxLines: compact ? 2 : null,
              overflow: compact ? .ellipsis : null,
              style: theme.textTheme.bodySmall,
            ),
          ),
          if (onRetry != null)
            TextButton(
              onPressed: onRetry,
              style: TextButton.styleFrom(
                tapTargetSize: .shrinkWrap,
                visualDensity: .compact,
              ),
              child: const Text('Retry'),
            ),
        ],
      ),
    );
  }
}

class _ConstrainedMessage extends StatelessWidget {
  const _ConstrainedMessage({required this.maxHeight, required this.child});

  final double maxHeight;
  final Widget child;

  @override
  Widget build(BuildContext context) => ConstrainedBox(
    constraints: BoxConstraints(maxHeight: maxHeight),
    child: SingleChildScrollView(child: child),
  );
}

class _MetaText extends StatelessWidget {
  const _MetaText(this.text, {this.small = false, this.color});

  final String text;
  final bool small;
  final Color? color;

  @override
  Widget build(BuildContext context) => Text(
    text,
    maxLines: 1,
    overflow: .ellipsis,
    style: Theme.of(context).textTheme.bodySmall
        ?.copyWith(fontSize: small ? 11 : 12, color: color),
  );
}

String _extractHighlightQuery(String raw) {
  var text = raw.trim();
  if (text.isEmpty) return '';
  if (text.startsWith('rg ')) {
    text = text.substring(3).trimLeft();
  }
  final tokens = <String>[];
  final buffer = StringBuffer();
  var quoted = false;
  for (final rune in text.runes) {
    final ch = String.fromCharCode(rune);
    if (ch == '"') {
      quoted = !quoted;
      continue;
    }
    if (!quoted && ch.trim().isEmpty) {
      if (buffer.isNotEmpty) {
        tokens.add(buffer.toString());
        buffer.clear();
      }
      continue;
    }
    buffer.write(ch);
  }
  if (buffer.isNotEmpty) tokens.add(buffer.toString());

  const filterKeys = {'in', 'hidden', 'type', 'size', 'mod', 'ext', 'git'};
  final terms = <String>[];
  for (final token in tokens) {
    final colon = token.indexOf(':');
    if (colon > 0 && filterKeys.contains(token.substring(0, colon))) {
      continue;
    }
    final cleaned = token.split('/').where((part) => part.isNotEmpty);
    terms.addAll(cleaned);
  }
  return terms.join(' ');
}
