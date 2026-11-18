import 'package:material_ui/material_ui.dart';

const launcherPreviewMono = TextStyle(
  fontFamily: 'JetBrains Mono',
  fontFamilyFallback: ['JetBrainsMono Nerd Font Mono', 'monospace'],
  fontSize: 12,
  height: 1.5,
);

class LauncherPreviewFrame extends StatelessWidget {
  const LauncherPreviewFrame({
    required this.child,
    this.compact = false,
    super.key,
  });

  final Widget child;
  final bool compact;

  @override
  Widget build(BuildContext context) => Container(
    padding: .all(compact ? 10 : 16),
    decoration: BoxDecoration(
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
      borderRadius: .circular(20),
    ),
    child: child,
  );
}

class LauncherPreviewContent extends StatelessWidget {
  const LauncherPreviewContent({required this.child, super.key});

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

class LauncherPreviewCode extends StatelessWidget {
  const LauncherPreviewCode(
    this.text, {
    this.firstLine = 1,
    this.highlightedLine,
    this.showRemainder = true,
    this.syntaxHighlight = true,
    super.key,
  });

  final String text;
  final int firstLine;
  final int? highlightedLine;
  final bool showRemainder;
  final bool syntaxHighlight;

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
          _buildLine(colors, firstLine + index, line),
        if (showRemainder && lines.length > 8)
          Text('+${lines.length - 8} more lines'),
      ],
    );
  }

  Widget _buildLine(ColorScheme colors, int lineNumber, String line) {
    final highlighted = highlightedLine == lineNumber;
    final codeStyle = highlighted
        ? launcherPreviewMono.copyWith(color: colors.onSecondaryContainer)
        : launcherPreviewMono;
    final row = Row(
      crossAxisAlignment: .start,
      spacing: 10,
      children: [
        Text(
          '$lineNumber',
          style: launcherPreviewMono.copyWith(
            color: highlighted
                ? colors.onSecondaryContainer
                : colors.onSurfaceVariant,
          ),
        ),
        Expanded(
          child: Text.rich(
            syntaxHighlight
                ? TextSpan(children: _spans(line, colors))
                : TextSpan(text: line),
            style: codeStyle,
            overflow: .ellipsis,
            maxLines: 1,
          ),
        ),
      ],
    );
    if (!highlighted) return row;
    return DecoratedBox(
      decoration: BoxDecoration(
        color: colors.secondaryContainer,
        borderRadius: .circular(4),
      ),
      child: row,
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

class LauncherPreviewImageFrame extends StatelessWidget {
  const LauncherPreviewImageFrame({
    required this.child,
    this.maxHeight = 220,
    super.key,
  });

  final Widget child;
  final double maxHeight;

  @override
  Widget build(BuildContext context) => ClipRRect(
    borderRadius: .circular(12),
    child: ConstrainedBox(
      constraints: BoxConstraints(maxHeight: maxHeight),
      child: CustomPaint(
        painter: _TransparencyGrid(Theme.of(context).colorScheme),
        child: child,
      ),
    ),
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

class LauncherPreviewMetadata extends StatelessWidget {
  const LauncherPreviewMetadata(this.metadata, {super.key});

  final Map<String, String> metadata;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Column(
      spacing: 6,
      children: [
        for (final entry in metadata.entries)
          Row(
            crossAxisAlignment: .start,
            mainAxisAlignment: .spaceBetween,
            spacing: 8,
            children: [
              Flexible(
                child: Text(entry.key, style: theme.textTheme.bodySmall),
              ),
              Flexible(
                flex: 2,
                child: Text(
                  entry.value,
                  textAlign: .end,
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: colors.onSurface,
                    fontWeight: .w500,
                  ),
                ),
              ),
            ],
          ),
      ],
    );
  }
}
