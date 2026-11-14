import 'dart:async';
import 'dart:io';

import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import 'clipboard_client.dart';
import 'clipboard_color.dart';
import 'clipboard_preview.dart';
import 'clipboard_svg.dart';

const clipboardMono = TextStyle(
  fontFamily: 'JetBrains Mono',
  fontFamilyFallback: ['JetBrainsMono Nerd Font Mono', 'monospace'],
  fontSize: 12,
  height: 1.5,
);

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
      return _PreviewContent(
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
      .text => _PreviewContent(child: _TextLines(text)),
      .command => _PreviewContent(child: _TextLines(text, mono: true)),
      .code => _PreviewContent(child: _CodeLines(text)),
      .link => _PreviewContent(
        child: Text(
          text,
          style: clipboardMono.copyWith(
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
      .color => _PreviewContent(child: _TextLines(text, mono: true)),
      .files => _filesPreview(detail),
      .hidden => const SizedBox.shrink(),
    };
  }

  Widget _filesPreview(ClipboardDetail detail) {
    final fallback = _PreviewContent(
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
        wrapper: (context, child) => _ImageFrame(child: child),
      );
    }
    return fallback;
  }
}

class _PreviewContent extends StatelessWidget {
  const _PreviewContent({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => Container(
    padding: const .all(12),
    decoration: BoxDecoration(
      color: Theme.of(context).colorScheme.surfaceContainer,
      borderRadius: .circular(12),
    ),
    child: child,
  );
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
              ? clipboardMono
              : const TextStyle(fontSize: 13, height: 1.5),
        ),
        if (lines.length > 8) Text('+${lines.length - 8} more lines'),
      ],
    );
  }
}

class _CodeLines extends StatelessWidget {
  const _CodeLines(this.text);

  final String text;

  static final _tokens = RegExp(
    r'''("(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*')|(#.*|//.*)|(\b(?:async|await|class|const|def|else|fn|for|from|function|if|import|let|pub|return|var|void|while)\b)|(\b\d+(?:\.\d+)?\b)''',
  );

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final lines = text.split('\n');
    return Column(
      crossAxisAlignment: .stretch,
      spacing: 2,
      children: [
        for (final (index, line) in lines.take(8).indexed)
          Row(
            crossAxisAlignment: .start,
            spacing: 10,
            children: [
              Text(
                '${index + 1}',
                style: clipboardMono.copyWith(color: colors.onSurfaceVariant),
              ),
              Expanded(
                child: Text.rich(
                  TextSpan(children: _spans(line, colors)),
                  style: clipboardMono,
                  overflow: .ellipsis,
                  maxLines: 1,
                ),
              ),
            ],
          ),
        if (lines.length > 8) Text('+${lines.length - 8} more lines'),
      ],
    );
  }

  List<TextSpan> _spans(String line, ColorScheme colors) {
    var start = 0;
    final spans = <TextSpan>[];
    for (final token in _tokens.allMatches(line)) {
      if (start < token.start) {
        spans.add(TextSpan(text: line.substring(start, token.start)));
      }
      final color = token[1] != null
          ? colors.tertiary
          : token[2] != null
          ? colors.onSurfaceVariant
          : colors.primary;
      spans.add(
        TextSpan(
          text: token[0],
          style: TextStyle(color: color),
        ),
      );
      start = token.end;
    }
    if (start < line.length) spans.add(TextSpan(text: line.substring(start)));
    return spans;
  }
}

class _ImageFrame extends StatelessWidget {
  const _ImageFrame({required this.child});

  final Widget child;

  @override
  Widget build(BuildContext context) => ClipRRect(
    borderRadius: .circular(12),
    child: ConstrainedBox(
      constraints: const BoxConstraints(maxHeight: 220),
      child: CustomPaint(
        painter: _TransparencyGrid(Theme.of(context).colorScheme),
        child: child,
      ),
    ),
  );
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
        _ImageFrame(
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

class _TransparencyGrid extends CustomPainter {
  const _TransparencyGrid(this.colors);

  final ColorScheme colors;

  @override
  void paint(Canvas canvas, Size size) {
    const tile = 8.0;
    canvas.drawRect(
      Offset.zero & size,
      Paint()..color = colors.surfaceContainer,
    );
    final paint = Paint()..color = colors.surfaceContainerHigh;
    for (var y = 0.0; y < size.height; y += tile) {
      for (var x = y % (tile * 2); x < size.width; x += tile * 2) {
        canvas.drawRect(Rect.fromLTWH(x, y, tile, tile), paint);
      }
    }
  }

  @override
  bool shouldRepaint(_TransparencyGrid oldDelegate) =>
      colors != oldDelegate.colors;
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
      _PreviewContent(
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
                      Expanded(child: Text(value, style: clipboardMono)),
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
              LauncherIcon(
                snapshot.data == FileSystemEntityType.directory
                    ? .folder
                    : .description,
                size: 18,
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
