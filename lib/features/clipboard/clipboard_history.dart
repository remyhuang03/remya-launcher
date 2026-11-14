import 'dart:math' as math;
import 'dart:typed_data';

import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import '../launcher/launcher_results.dart';
import 'clipboard_client.dart';
import 'clipboard_color.dart';
import 'clipboard_live_preview.dart';
import 'clipboard_svg.dart';

class ClipboardHistory extends StatelessWidget {
  const ClipboardHistory({
    required this.entries,
    required this.selectedId,
    required this.client,
    required this.scroll,
    required this.filters,
    required this.paused,
    required this.compact,
    required this.preview,
    required this.onSelected,
    this.onPaste,
    super.key,
  });

  final List<ClipboardEntry> entries;
  final String? selectedId;
  final ClipboardClient client;
  final ScrollController scroll;
  final Widget filters;
  final bool paused;
  final bool compact;
  final Widget preview;
  final ValueChanged<ClipboardEntry> onSelected;
  final ValueChanged<ClipboardEntry>? onPaste;

  static void showEntry(
    ScrollController controller,
    List<ClipboardEntry> entries,
    String? id,
  ) {
    if (id == null || !controller.hasClients) return;
    final position = controller.position;
    var offset = 0.0;
    for (final row in _historyRows(entries)) {
      if (row.entry?.id == id) {
        if (offset < position.pixels) {
          controller.jumpTo(offset.clamp(0, position.maxScrollExtent));
        } else if (offset + row.height >
            position.pixels + position.viewportDimension) {
          final bottom = offset + row.height - position.viewportDimension;
          controller.jumpTo(bottom.clamp(0, position.maxScrollExtent));
        }
        return;
      }
      offset += row.height;
    }
  }

  @override
  Widget build(BuildContext context) {
    final rows = _historyRows(entries);
    final list = Column(
      crossAxisAlignment: .stretch,
      spacing: 4,
      children: [
        filters,
        if (paused)
          Padding(
            padding: const .symmetric(horizontal: 12, vertical: 2),
            child: Text(
              'Clipboard history is paused.',
              style: Theme.of(context).textTheme.bodySmall,
            ),
          ),
        Expanded(
          child: ListView.builder(
            controller: scroll,
            itemCount: rows.length,
            itemExtentBuilder: (index, dimensions) => rows[index].height,
            itemBuilder: (context, index) => _row(context, rows[index]),
          ),
        ),
      ],
    );
    return LayoutBuilder(
      builder: (context, constraints) {
        final wide = constraints.maxWidth >= 650;
        final rowsHeight = rows.fold(
          36.0,
          (height, row) => height + row.height,
        );
        return SizedBox(
          height: wide ? math.min(math.max(300.0, rowsHeight), 595.0) : 595,
          child: wide
              ? Row(
                  crossAxisAlignment: .stretch,
                  spacing: 8,
                  children: [
                    Expanded(child: list),
                    SizedBox(width: 320, child: preview),
                  ],
                )
              : Column(
                  crossAxisAlignment: .stretch,
                  spacing: 8,
                  children: [
                    Expanded(flex: 3, child: list),
                    Expanded(flex: 2, child: preview),
                  ],
                ),
        );
      },
    );
  }

  Widget _row(BuildContext context, _HistoryRow row) {
    final entry = row.entry;
    if (entry == null) {
      return LauncherSectionHeader(
        row.heading!,
        height: row.height,
        trailing: row.heading == 'PINNED'
            ? const LauncherKeyHint(['Ctrl', 'D'], label: 'pin / unpin')
            : null,
      );
    }
    final selected = entry.id == selectedId;
    return Padding(
      key: ValueKey(entry.id),
      padding: const .only(bottom: 2),
      child: LauncherResultRow(
        title: TextSpan(
          text: entry.kind == .hidden ? '••••••••••••' : entry.title,
        ),
        subtitle: entry.summary.isEmpty
            ? entry.kind.label
            : '${entry.kind.label} · ${entry.summary}',
        leading: _EntryLeading(
          entry: entry,
          client: client,
          selected: selected,
        ),
        selected: selected,
        monospaced: const {
          ClipboardKind.command,
          ClipboardKind.code,
          ClipboardKind.color,
        }.contains(entry.kind),
        trailing: selected && entry.canPaste
            ? compact
                  ? IconButton(
                      tooltip: 'Paste',
                      onPressed: onPaste == null ? null : () => onPaste!(entry),
                      visualDensity: .compact,
                      icon: const Icon(Icons.keyboard_return_rounded, size: 18),
                    )
                  : TextButton(
                      onPressed: onPaste == null ? null : () => onPaste!(entry),
                      child: const Text('Paste  ↵'),
                    )
            : Text(
                entry.pinned ? 'Pinned' : clipboardRelativeTime(entry.copiedAt),
                style: Theme.of(context).textTheme.bodySmall,
              ),
        onPressed: () => onSelected(entry),
      ),
    );
  }
}

class _HistoryRow {
  const _HistoryRow.entry(this.entry) : heading = null, height = 58;
  const _HistoryRow.heading(this.heading)
    : entry = null,
      height = heading == 'PINNED' ? 32 : 26;

  final ClipboardEntry? entry;
  final String? heading;
  final double height;
}

List<_HistoryRow> _historyRows(List<ClipboardEntry> entries) {
  final now = DateTime.now();
  final today = DateTime(now.year, now.month, now.day);
  final yesterday = DateTime(now.year, now.month, now.day - 1);
  final groups = <String, List<ClipboardEntry>>{
    'PINNED': [],
    'TODAY': [],
    'YESTERDAY': [],
    'EARLIER': [],
  };
  for (final entry in entries) {
    final name = entry.pinned
        ? 'PINNED'
        : !entry.copiedAt.isBefore(today)
        ? 'TODAY'
        : !entry.copiedAt.isBefore(yesterday)
        ? 'YESTERDAY'
        : 'EARLIER';
    groups[name]!.add(entry);
  }
  return [
    for (final group in groups.entries)
      if (group.value.isNotEmpty) ...[
        _HistoryRow.heading(group.key),
        for (final entry in group.value) _HistoryRow.entry(entry),
      ],
  ];
}

class _EntryLeading extends StatefulWidget {
  const _EntryLeading({
    required this.entry,
    required this.client,
    required this.selected,
  });

  final ClipboardEntry entry;
  final ClipboardClient client;
  final bool selected;

  @override
  State<_EntryLeading> createState() => _EntryLeadingState();
}

class _EntryLeadingState extends State<_EntryLeading> {
  Future<Uint8List?>? _thumbnail;
  Future<String?>? _svgPath;

  @override
  void initState() {
    super.initState();
    _loadThumbnail();
  }

  @override
  void didUpdateWidget(covariant _EntryLeading oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.entry.id != widget.entry.id) _loadThumbnail();
  }

  void _loadThumbnail() {
    _thumbnail = widget.entry.kind == .image
        ? widget.client.thumbnail(widget.entry.id)
        : null;
    _svgPath = widget.entry.kind == .files && isSvgFilePath(widget.entry.title)
        ? _loadSvgPath()
        : null;
  }

  Future<String?> _loadSvgPath() async {
    try {
      final detail = await widget.client.detail(widget.entry.id);
      if (detail.files.length == 1 && isSvgFilePath(detail.files.first)) {
        return detail.files.first;
      }
    } catch (_) {
      // Fall back to the file tile when detail lookup fails.
    }
    return null;
  }

  @override
  Widget build(BuildContext context) {
    final entry = widget.entry;
    final color = clipboardColor(entry.color);
    if (color != null) {
      return DecoratedBox(
        decoration: BoxDecoration(color: color, shape: .circle),
      );
    }
    final tile = LauncherSymbolTile(switch (entry.kind) {
      .text => .text,
      .command => .terminal,
      .code => .code,
      .image => .image,
      .link => .public,
      .color => .palette,
      .files => .folder,
      .hidden => .lock,
    }, selected: widget.selected);
    if (entry.kind == .files && _svgPath != null) {
      return FutureBuilder<String?>(
        future: _svgPath,
        builder: (context, snapshot) => switch (snapshot.data) {
          final path? => ClipboardSvgFile(
            path: path,
            fit: .cover,
            excludeFromSemantics: true,
            fallback: tile,
            wrapper: (context, child) =>
                ClipRRect(borderRadius: .circular(8), child: child),
          ),
          null => tile,
        },
      );
    }
    if (entry.kind != .image) return tile;
    return FutureBuilder<Uint8List?>(
      future: _thumbnail,
      builder: (context, snapshot) => switch (snapshot.data) {
        final bytes? => ClipRRect(
          borderRadius: .circular(8),
          child: Image.memory(
            bytes,
            fit: .cover,
            cacheWidth: 80,
            excludeFromSemantics: true,
            errorBuilder: (context, error, stack) => tile,
          ),
        ),
        null => tile,
      },
    );
  }
}
