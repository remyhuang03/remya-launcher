import 'dart:async';
import 'dart:io';

import 'package:material_ui/material_ui.dart';

import '../files/file_icon_tile.dart';
import '../launcher/launcher_controls.dart';
import '../launcher/launcher_preview.dart';
import 'clipboard_client.dart';
import 'clipboard_color.dart';
import 'clipboard_preview.dart';
import 'clipboard_svg.dart';

String clipboardRelativeTime(DateTime timestamp) {
  final age = DateTime.now().difference(timestamp);
  if (age.inMinutes < 1) return 'Just now';
  if (age.inHours < 1) return '${age.inMinutes} min';
  if (age.inDays < 1) return '${age.inHours} h';
  return '${age.inDays} d';
}

class ClipboardLivePreview extends StatelessWidget {
  const ClipboardLivePreview({
    required this.entry,
    required this.detail,
    required this.hints,
    required this.revealed,
    required this.compact,
    this.onColor,
    super.key,
  });

  final ClipboardEntry entry;
  final ClipboardDetail? detail;
  final List<LauncherKeyHint> hints;
  final String? revealed;
  final bool compact;
  final ValueChanged<String>? onColor;

  @override
  Widget build(BuildContext context) {
    final locale = MaterialLocalizations.of(context);
    final copied = entry.copiedAt.toLocal();
    final detail = this.detail;
    final dimensions = detail?.width != null && detail?.height != null
        ? '${detail!.width} × ${detail.height}'
        : null;
    final summary = entry.kind == .image && dimensions != null
        ? '$dimensions · ${entry.mimeTypes.firstOrNull?.split('/').last.toUpperCase() ?? 'Image'}'
        : entry.summary;
    return ClipboardPreview(
      type: entry.kind.label,
      compact: compact,
      summary: summary,
      content: _content(context),
      metadata: {
        'Copied':
            '${locale.formatShortDate(copied)} · ${locale.formatTimeOfDay(TimeOfDay.fromDateTime(copied))}',
        if (entry.mimeTypes.isNotEmpty && entry.kind != .hidden)
          'MIME': entry.mimeTypes.join(', '),
        if (entry.timesCopied > 1) 'Times copied': '${entry.timesCopied}',
        if (detail?.size case final size?)
          'Size': size < 1024
              ? '$size B'
              : size < 1024 * 1024
              ? '${(size / 1024).toStringAsFixed(1)} KB'
              : '${(size / (1024 * 1024)).toStringAsFixed(1)} MB',
        if (entry.kind == .hidden) 'Stored on disk': 'No',
      },
      hints: hints,
    );
  }

  Widget _content(BuildContext context) {
    final detail = this.detail;
    if (entry.kind == .hidden) {
      return LauncherPreviewContent(
        child: _HiddenPreview(
          key: ValueKey(entry.id),
          expiry: entry.expiresAt,
          revealed: revealed,
        ),
      );
    }
    if (detail == null) {
      return const LinearProgressIndicator(semanticsLabel: 'Loading preview');
    }
    final text = detail.text ?? '';
    final color = clipboardColor(entry.color);
    return switch (entry.kind) {
      .text => LauncherPreviewContent(child: _TextLines(text)),
      .command => LauncherPreviewContent(child: _TextLines(text, mono: true)),
      .code => LauncherPreviewContent(child: LauncherPreviewCode(text)),
      .link => LauncherPreviewContent(
        child: Text(
          text,
          style: launcherPreviewMono.copyWith(
            color: Theme.of(context).colorScheme.primary,
          ),
        ),
      ),
      .image => _ImagePreview(detail, thumbnailOnly: entry.thumbnailOnly),
      .color when color != null => _ColorPreview(
        color,
        formats: clipboardColorFormats(entry.color!, color),
        onFormat: onColor,
      ),
      .color => LauncherPreviewContent(child: _TextLines(text, mono: true)),
      .files => _filesPreview(detail),
      .hidden => const SizedBox.shrink(),
    };
  }

  Widget _filesPreview(ClipboardDetail detail) {
    final fallback = LauncherPreviewContent(
      child: Column(
        crossAxisAlignment: .stretch,
        spacing: 8,
        children: [
          for (final path in detail.files.take(24)) _FileLine(path),
          if (detail.files.length > 24)
            Text('+${detail.files.length - 24} more files'),
        ],
      ),
    );
    if (detail.files.length == 1 && isSvgFilePath(detail.files.first)) {
      return ClipboardSvgFile(
        path: detail.files.first,
        fit: .contain,
        semanticLabel: 'Clipboard image',
        fallback: fallback,
        wrapper: (context, child) => LauncherPreviewImageFrame(child: child),
      );
    }
    return fallback;
  }
}

class _TextLines extends StatelessWidget {
  const _TextLines(this.text, {this.mono = false});

  final String text;
  final bool mono;

  @override
  Widget build(BuildContext context) {
    final lines = text.split('\n');
    return Column(
      crossAxisAlignment: .stretch,
      spacing: 6,
      children: [
        Text(
          lines.take(8).join('\n'),
          style: mono
              ? launcherPreviewMono
              : const TextStyle(fontSize: 13, height: 1.5),
        ),
        if (lines.length > 8) Text('+${lines.length - 8} more lines'),
      ],
    );
  }
}

class _ImagePreview extends StatelessWidget {
  const _ImagePreview(this.detail, {required this.thumbnailOnly});

  final ClipboardDetail detail;
  final bool thumbnailOnly;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: .stretch,
    spacing: 8,
    children: [
      if (detail.thumbnail case final thumbnail?)
        LauncherPreviewImageFrame(
          child: Image.memory(
            thumbnail,
            fit: .contain,
            semanticLabel: 'Clipboard image',
            errorBuilder: (context, error, stack) =>
                const Text('Image preview unavailable'),
          ),
        )
      else
        const Text('Image preview unavailable'),
      if (thumbnailOnly)
        const Text('Thumbnail only. Original image unavailable.'),
    ],
  );
}

class _ColorPreview extends StatelessWidget {
  const _ColorPreview(this.color, {required this.formats, this.onFormat});

  final Color color;
  final List<String> formats;
  final ValueChanged<String>? onFormat;

  static const _names = ['HEX', 'RGB', 'HSL', 'OKLCH'];

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: .stretch,
    spacing: 12,
    children: [
      Container(
        height: 72,
        decoration: BoxDecoration(color: color, borderRadius: .circular(12)),
      ),
      LauncherPreviewContent(
        child: Column(
          crossAxisAlignment: .stretch,
          children: [
            for (final (index, value) in formats.indexed)
              InkWell(
                onTap: onFormat == null ? null : () => onFormat!(value),
                child: Padding(
                  padding: const .symmetric(vertical: 4),
                  child: Row(
                    spacing: 6,
                    children: [
                      SizedBox(
                        width: 40,
                        child: Text(
                          _names[index],
                          style: Theme.of(context).textTheme.labelSmall,
                        ),
                      ),
                      Expanded(child: Text(value, style: launcherPreviewMono)),
                      LauncherKeycap('${index + 1}'),
                    ],
                  ),
                ),
              ),
          ],
        ),
      ),
    ],
  );
}

class _FileLine extends StatefulWidget {
  const _FileLine(this.path);

  final String path;

  @override
  State<_FileLine> createState() => _FileLineState();
}

class _FileLineState extends State<_FileLine> {
  late final _type = FileSystemEntity.type(widget.path);

  @override
  Widget build(BuildContext context) {
    final path = widget.path;
    final segments = path.split('/');
    final title = segments.last.isEmpty ? path : segments.last;
    final parent = segments.length > 1
        ? segments.take(segments.length - 1).join('/')
        : '';
    return FutureBuilder<FileSystemEntityType>(
      future: _type,
      builder: (context, snapshot) {
        final missing = snapshot.data == FileSystemEntityType.notFound;
        return Opacity(
          opacity: missing ? .55 : 1,
          child: Row(
            spacing: 8,
            children: [
              FileIconTile(
                basename: title,
                isDirectory: snapshot.data == FileSystemEntityType.directory,
                size: 28,
                iconSize: 18,
              ),
              Expanded(
                child: Column(
                  crossAxisAlignment: .start,
                  children: [
                    Text(title, maxLines: 1, overflow: .ellipsis),
                    if (parent.isNotEmpty || missing)
                      Text(
                        missing ? '$parent · No longer exists' : parent,
                        maxLines: 2,
                        overflow: .ellipsis,
                        style: Theme.of(context).textTheme.bodySmall,
                      ),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _HiddenPreview extends StatefulWidget {
  const _HiddenPreview({
    required this.expiry,
    required this.revealed,
    super.key,
  });

  final DateTime? expiry;
  final String? revealed;

  @override
  State<_HiddenPreview> createState() => _HiddenPreviewState();
}

class _HiddenPreviewState extends State<_HiddenPreview> {
  Timer? _clock;

  @override
  void initState() {
    super.initState();
    if (widget.expiry != null) {
      _clock = Timer.periodic(const Duration(seconds: 1), (_) {
        if (_remaining() == 0) _clock?.cancel();
        setState(() {});
      });
    }
  }

  int _remaining() =>
      ((widget.expiry?.difference(DateTime.now()).inMilliseconds ?? 0) / 1000)
          .ceil()
          .clamp(0, 45);

  @override
  void dispose() {
    _clock?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final remaining = _remaining();
    return Column(
      spacing: 8,
      children: [
        const LauncherIcon(.lock, size: 28),
        Text(
          remaining > 0 ? widget.revealed ?? '••••••••••••' : 'Expired',
          maxLines: 8,
          overflow: .ellipsis,
          textAlign: .center,
          style: const TextStyle(fontSize: 16),
        ),
        if (remaining > 0)
          Text(
            'Hold Ctrl + Space to reveal · Clears in 0:${remaining.toString().padLeft(2, '0')}',
          ),
        if (widget.expiry != null)
          LinearProgressIndicator(
            value: remaining / 45,
            borderRadius: .circular(3),
            semanticsLabel: 'Time remaining',
          ),
      ],
    );
  }
}
