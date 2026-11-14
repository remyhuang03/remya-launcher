import 'package:material_ui/material_ui.dart';

import 'launcher_controls.dart';

class LauncherAnswerCard extends StatelessWidget {
  const LauncherAnswerCard({
    required this.expression,
    required this.value,
    required this.unit,
    required this.alternatives,
    this.selectedAlternative = 0,
    this.onAlternativeSelected,
    this.onCopy,
    this.details = const [],
    super.key,
  });

  final String expression;
  final String value;
  final String unit;
  final List<String> alternatives;
  final int selectedAlternative;
  final ValueChanged<int>? onAlternativeSelected;
  final VoidCallback? onCopy;
  final List<Widget> details;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Material(
      color: colors.primaryContainer,
      borderRadius: .circular(28),
      child: Padding(
        padding: const .symmetric(horizontal: 24, vertical: 20),
        child: DefaultTextStyle.merge(
          style: TextStyle(color: colors.onPrimaryContainer),
          child: Column(
            crossAxisAlignment: .start,
            spacing: 10,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      expression,
                      style: const TextStyle(fontSize: 13),
                    ),
                  ),
                  if (onCopy != null)
                    InkWell(
                      onTap: onCopy,
                      borderRadius: .circular(6),
                      child: const Row(
                        spacing: 6,
                        children: [
                          Text('Copy', style: TextStyle(fontSize: 12)),
                          LauncherKeycap('↵'),
                        ],
                      ),
                    ),
                ],
              ),
              Text.rich(
                TextSpan(
                  text: value,
                  children: [
                    TextSpan(
                      text: unit.isEmpty ? '' : ' $unit',
                      style: const TextStyle(fontSize: 18, fontWeight: .w500),
                    ),
                  ],
                ),
                style: const TextStyle(
                  fontSize: 44,
                  height: 52 / 44,
                  fontWeight: .w700,
                ),
              ),
              ...details,
              if (alternatives.isNotEmpty)
                Wrap(
                  spacing: 8,
                  runSpacing: 6,
                  children: [
                    for (final (index, label) in alternatives.indexed)
                      _AnswerOption(
                        label: label,
                        selected: index == selectedAlternative,
                        onPressed: onAlternativeSelected == null
                            ? null
                            : () => onAlternativeSelected!(index),
                      ),
                  ],
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _AnswerOption extends StatelessWidget {
  const _AnswerOption({
    required this.label,
    required this.selected,
    this.onPressed,
  });

  final String label;
  final bool selected;
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final shape = RoundedRectangleBorder(
      borderRadius: .circular(selected ? 8 : 14),
      side: selected ? .none : BorderSide(color: colors.onPrimaryContainer),
    );
    return Semantics(
      selected: selected,
      child: Material(
        color: selected ? colors.onPrimaryContainer : Colors.transparent,
        shape: shape,
        child: InkWell(
          customBorder: shape,
          onTap: onPressed,
          child: Padding(
            padding: .symmetric(horizontal: selected ? 12 : 13),
            child: SizedBox(
              height: 28,
              child: Center(
                widthFactor: 1,
                child: Text(
                  label,
                  style: TextStyle(
                    fontSize: 12,
                    fontWeight: .w500,
                    color: selected
                        ? colors.primaryContainer
                        : colors.onPrimaryContainer,
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

class LauncherFilePreview extends StatelessWidget {
  const LauncherFilePreview({
    required this.title,
    required this.summary,
    required this.path,
    required this.entries,
    super.key,
  });

  final String title;
  final String summary;
  final String path;
  final List<String> entries;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Container(
      decoration: BoxDecoration(
        color: colors.surfaceContainerHighest,
        borderRadius: .circular(20),
      ),
      padding: const .all(16),
      child: Column(
        crossAxisAlignment: .stretch,
        spacing: 12,
        children: [
          Align(
            alignment: .centerLeft,
            child: Container(
              width: 56,
              height: 56,
              alignment: .center,
              decoration: BoxDecoration(
                color: colors.primaryContainer,
                borderRadius: .circular(16),
              ),
              child: LauncherIcon(
                .folder,
                size: 28,
                color: colors.onPrimaryContainer,
              ),
            ),
          ),
          Column(
            crossAxisAlignment: .start,
            spacing: 2,
            children: [
              Text(
                title,
                style: const TextStyle(
                  fontSize: 16,
                  fontWeight: .w600,
                  height: 20 / 16,
                ),
              ),
              Text(summary, style: theme.textTheme.bodySmall),
            ],
          ),
          Column(
            spacing: 6,
            children: [
              for (final entry in entries)
                Row(
                  spacing: 8,
                  children: [
                    const LauncherIcon(.description, size: 16),
                    Expanded(
                      child: Text(
                        entry,
                        maxLines: 1,
                        overflow: .ellipsis,
                        style: theme.textTheme.bodySmall,
                      ),
                    ),
                  ],
                ),
            ],
          ),
          Divider(height: 1, color: colors.outlineVariant),
          Text(path, style: theme.textTheme.bodySmall?.copyWith(fontSize: 11)),
        ],
      ),
    );
  }
}

class LauncherAction {
  const LauncherAction(
    this.label,
    this.icon,
    this.keys, {
    this.destructive = false,
    this.onPressed,
  });

  final String label;
  final LauncherIconName icon;
  final List<String> keys;
  final bool destructive;
  final VoidCallback? onPressed;
}

class LauncherActionsMenu extends StatelessWidget {
  const LauncherActionsMenu({
    required this.title,
    required this.actions,
    this.selectedIndex = 0,
    super.key,
  });

  final String title;
  final List<LauncherAction> actions;
  final int selectedIndex;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Material(
      color: colors.surfaceContainerHigh,
      borderRadius: .circular(24),
      elevation: 8,
      child: Padding(
        padding: const .all(8),
        child: Column(
          mainAxisSize: .min,
          spacing: 2,
          children: [
            Padding(
              padding: const .only(left: 12, right: 8, top: 8, bottom: 8),
              child: Row(
                spacing: 8,
                children: [
                  Expanded(
                    child: Text(
                      title,
                      maxLines: 1,
                      overflow: .ellipsis,
                      style: const TextStyle(
                        fontSize: 13,
                        height: 16 / 13,
                        fontWeight: .w600,
                      ),
                    ),
                  ),
                  Text(
                    'type to filter',
                    style: theme.textTheme.bodySmall?.copyWith(fontSize: 11),
                  ),
                ],
              ),
            ),
            for (final (index, action) in actions.indexed) ...[
              if (action.destructive)
                Divider(height: 6, color: colors.outlineVariant),
              _ActionRow(action, selected: index == selectedIndex),
            ],
          ],
        ),
      ),
    );
  }
}

class _ActionRow extends StatelessWidget {
  const _ActionRow(this.action, {required this.selected});

  final LauncherAction action;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final foreground = action.destructive
        ? colors.error
        : selected
        ? colors.onSecondaryContainer
        : colors.onSurface;
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
          onTap: action.onPressed,
          child: SizedBox(
            height: 40,
            child: Padding(
              padding: const .only(left: 12, right: 8),
              child: Row(
                spacing: 10,
                children: [
                  LauncherIcon(
                    action.icon,
                    size: 18,
                    color: action.destructive || selected ? foreground : null,
                  ),
                  Expanded(
                    child: Text(
                      action.label,
                      maxLines: 1,
                      overflow: .ellipsis,
                      style: TextStyle(
                        color: foreground,
                        fontSize: 14,
                        fontWeight: .w500,
                      ),
                    ),
                  ),
                  for (final key in action.keys) LauncherKeycap(key),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
