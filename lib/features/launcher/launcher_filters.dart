import 'package:material_ui/material_ui.dart';

import 'launcher_controls.dart';

class LauncherFilters extends StatelessWidget {
  const LauncherFilters({
    required this.counts,
    required this.selected,
    this.shortcut,
    this.onSelected,
    super.key,
  });

  final Map<String, int?> counts;
  final LauncherKeyHint? shortcut;
  final String selected;
  final ValueChanged<String>? onSelected;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const .only(left: 8, right: 8, top: 4),
      child: Row(
        spacing: 8,
        children: [
          Expanded(
            child: SingleChildScrollView(
              scrollDirection: .horizontal,
              child: Row(
                spacing: 2,
                children: [
                  for (final entry in counts.entries)
                    _FilterChip(
                      label: entry.key,
                      count: entry.value,
                      selected: entry.key == selected,
                      onPressed: onSelected == null
                          ? null
                          : () => onSelected!(entry.key),
                    ),
                ],
              ),
            ),
          ),
          ?shortcut,
        ],
      ),
    );
  }
}

class _FilterChip extends StatelessWidget {
  const _FilterChip({
    required this.label,
    required this.count,
    required this.selected,
    this.onPressed,
  });

  final String label;
  final int? count;
  final bool selected;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final shape = RoundedRectangleBorder(borderRadius: .circular(14));
    return Semantics(
      selected: selected,
      child: Material(
        color: selected ? colors.surfaceContainerHighest : Colors.transparent,
        shape: shape,
        child: InkWell(
          customBorder: shape,
          onTap: onPressed,
          child: Padding(
            padding: const .symmetric(horizontal: 10),
            child: SizedBox(
              height: 28,
              child: Center(
                widthFactor: 1,
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(
                        text: label,
                        style: TextStyle(
                          color: selected
                              ? colors.onSurface
                              : colors.onSurfaceVariant,
                          fontWeight: .w500,
                        ),
                      ),
                      if (count != null) TextSpan(text: '  $count'),
                    ],
                  ),
                  style: theme.textTheme.bodySmall,
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
