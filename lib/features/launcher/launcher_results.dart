import 'package:material_ui/material_ui.dart';

import 'launcher_controls.dart';

class LauncherSectionHeader extends StatelessWidget {
  const LauncherSectionHeader(
    this.title, {
    this.trailing,
    this.height = 32,
    super.key,
  });

  final String title;
  final Widget? trailing;
  final double height;

  @override
  Widget build(BuildContext context) => Semantics(
    header: true,
    child: SizedBox(
      height: height,
      child: Padding(
        padding: const .symmetric(horizontal: 12),
        child: Row(
          spacing: 4,
          children: [
            Expanded(
              child: Text(
                title,
                maxLines: 1,
                overflow: .ellipsis,
                style: Theme.of(context).textTheme.labelSmall,
              ),
            ),
            ?trailing,
          ],
        ),
      ),
    ),
  );
}

class LauncherSymbolTile extends StatelessWidget {
  const LauncherSymbolTile(this.icon, {this.selected = false, super.key});

  final LauncherIconName icon;
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
      child: Center(child: LauncherIcon(icon)),
    );
  }
}

class LauncherResultRow extends StatelessWidget {
  const LauncherResultRow({
    required this.title,
    required this.subtitle,
    required this.leading,
    this.subtitleSpan,
    this.trailing,
    this.selected = false,
    this.monospaced = false,
    this.height = 56,
    this.onPressed,
    super.key,
  });

  final InlineSpan title;
  final String subtitle;
  final InlineSpan? subtitleSpan;
  final Widget leading;
  final Widget? trailing;
  final bool selected;
  final bool monospaced;
  final double height;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final shape = RoundedRectangleBorder(
      borderRadius: .circular(selected ? 12 : 20),
    );
    return Semantics(
      selected: selected,
      child: Material(
        shape: shape,
        color: selected ? colors.secondaryContainer : Colors.transparent,
        child: InkWell(
          customBorder: shape,
          onTap: onPressed,
          child: SizedBox(
            height: height,
            child: Padding(
              padding: const .only(left: 8, right: 12),
              child: Row(
                spacing: 12,
                children: [
                  SizedBox.square(dimension: 40, child: leading),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: .center,
                      crossAxisAlignment: .start,
                      children: [
                        Text.rich(
                          title,
                          maxLines: 1,
                          overflow: .ellipsis,
                          style: TextStyle(
                            fontSize: monospaced ? 14 : 15,
                            fontFamily: monospaced
                                ? 'JetBrainsMono Nerd Font Mono'
                                : null,
                            fontFamilyFallback: monospaced
                                ? const ['JetBrains Mono', 'monospace']
                                : null,
                            height: monospaced ? 20 / 14 : 20 / 15,
                            fontWeight: .w500,
                            color: selected
                                ? colors.onSecondaryContainer
                                : colors.onSurface,
                          ),
                        ),
                        Text.rich(
                          subtitleSpan ?? TextSpan(text: subtitle),
                          maxLines: 1,
                          overflow: .ellipsis,
                          style: theme.textTheme.bodySmall,
                        ),
                      ],
                    ),
                  ),
                  ?trailing,
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class LauncherPinnedTile extends StatelessWidget {
  const LauncherPinnedTile({
    required this.label,
    required this.icon,
    required this.shortcut,
    this.selected = false,
    this.onPressed,
    super.key,
  });

  final String label;
  final Widget icon;
  final String shortcut;
  final bool selected;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final shape = RoundedRectangleBorder(
      borderRadius: .circular(selected ? 12 : 20),
    );
    return Semantics(
      selected: selected,
      child: Material(
        color: selected
            ? colors.secondaryContainer
            : colors.surfaceContainerHighest,
        shape: shape,
        child: InkWell(
          customBorder: shape,
          onTap: onPressed,
          child: SizedBox(
            height: 88,
            child: Stack(
              children: [
                if (shortcut.isNotEmpty)
                  PositionedDirectional(
                    start: 8,
                    top: 8,
                    child: LauncherKeycap(shortcut),
                  ),
                Center(
                  child: Padding(
                    padding: const .only(top: 4),
                    child: Column(
                      mainAxisSize: .min,
                      spacing: 8,
                      children: [
                        SizedBox.square(dimension: 36, child: icon),
                        Text(
                          label,
                          maxLines: 1,
                          overflow: .ellipsis,
                          style: const TextStyle(fontSize: 12, height: 16 / 12),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// Emphasizes each whitespace-separated word of [query] found in [text].
InlineSpan launcherHighlight(String text, String query, Color color) {
  final words = query.split(RegExp(r'\s+')).where((word) => word.isNotEmpty);
  if (words.isEmpty) return TextSpan(text: text);
  final pattern = RegExp(
    words.map(RegExp.escape).join('|'),
    caseSensitive: false,
    unicode: true,
  );
  final matches = pattern.allMatches(text).toList();
  if (matches.isEmpty) return TextSpan(text: text);
  var start = 0;
  final parts = <TextSpan>[];
  for (final match in matches) {
    parts.add(TextSpan(text: text.substring(start, match.start)));
    parts.add(
      TextSpan(
        text: text.substring(match.start, match.end),
        style: TextStyle(color: color, fontWeight: .w600),
      ),
    );
    start = match.end;
  }
  return TextSpan(
    children: [
      ...parts,
      TextSpan(text: text.substring(start)),
    ],
  );
}
