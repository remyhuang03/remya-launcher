import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import '../launcher/launcher_preview.dart';

class ClipboardPreview extends StatelessWidget {
  const ClipboardPreview({
    required this.type,
    required this.summary,
    required this.content,
    required this.metadata,
    required this.hints,
    this.compact = false,
    super.key,
  });

  final String type;
  final String summary;
  final Widget content;
  final Map<String, String> metadata;
  final List<LauncherKeyHint> hints;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return LauncherPreviewFrame(
      compact: compact,
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        spacing: compact ? 8 : 12,
        children: [
          Row(
            spacing: 6,
            children: [
              Container(
                padding: const .symmetric(horizontal: 8, vertical: 2),
                decoration: BoxDecoration(
                  color: colors.secondaryContainer,
                  borderRadius: .circular(20),
                ),
                child: Text(
                  type,
                  style: TextStyle(
                    fontSize: 11,
                    height: 14 / 11,
                    fontWeight: .w500,
                    color: colors.onSecondaryContainer,
                  ),
                ),
              ),
              Expanded(child: Text(summary, style: theme.textTheme.bodySmall)),
            ],
          ),
          content,
          LauncherPreviewMetadata(metadata),
          Divider(height: 1, thickness: 1, color: colors.outlineVariant),
          Column(crossAxisAlignment: .start, spacing: 8, children: hints),
        ],
      ),
    );
  }
}

class ClipboardImageThumbnail extends StatelessWidget {
  const ClipboardImageThumbnail({this.square = false, super.key});

  final bool square;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final width = square ? 40.0 : 288.0;
    final height = square ? 40.0 : 162.0;
    return Semantics(
      label: 'Screenshot thumbnail',
      image: true,
      child: AspectRatio(
        aspectRatio: width / height,
        child: FittedBox(
          fit: .contain,
          child: SizedBox(
            width: width,
            height: height,
            child: ClipRRect(
              borderRadius: .circular(square ? 8 : 12),
              child: ColoredBox(
                color: colors.tertiaryContainer,
                child: Stack(
                  children: square
                      ? [
                          _tile(0, 0, 40, 6, colors.onTertiaryContainer, 0),
                          _tile(4, 10, 20, 26, colors.tertiary, 2),
                          _tile(27, 10, 9, 12, colors.onTertiaryContainer, 2),
                          _tile(27, 25, 9, 11, colors.onTertiaryContainer, 2),
                        ]
                      : [
                          _tile(0, 0, 288, 18, colors.onTertiaryContainer, 0),
                          _tile(14, 30, 150, 118, colors.tertiary, 6),
                          _tile(176, 30, 98, 56, colors.onTertiaryContainer, 6),
                          _tile(176, 94, 98, 54, colors.onTertiaryContainer, 6),
                          _tile(24, 44, 90, 8, colors.tertiaryContainer, 6),
                          _tile(24, 60, 120, 6, colors.tertiaryContainer, 6),
                          _tile(24, 72, 100, 6, colors.tertiaryContainer, 6),
                        ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }

  Widget _tile(
    double left,
    double top,
    double width,
    double height,
    Color color,
    double radius,
  ) {
    return Positioned(
      left: left,
      top: top,
      width: width,
      height: height,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: color,
          borderRadius: .circular(radius),
        ),
      ),
    );
  }
}
