import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';

class FileMenuItem {
  const FileMenuItem(
    this.label,
    this.icon,
    this.run, {
    this.keys = const [],
    this.danger = false,
  });

  final String label;
  final IconData icon;
  final VoidCallback run;
  final List<String> keys;
  final bool danger;
}

class FilesActionsMenu extends StatelessWidget {
  const FilesActionsMenu({
    required this.title,
    required this.items,
    required this.selected,
    required this.search,
    required this.focus,
    required this.onChanged,
    this.loading = false,
    this.indexes = false,
    this.setDefault,
    this.onSetDefault,
    super.key,
  });

  final String title;
  final List<FileMenuItem> items;
  final int selected;
  final TextEditingController search;
  final FocusNode focus;
  final ValueChanged<String> onChanged;
  final bool loading, indexes;
  final bool? setDefault;
  final ValueChanged<bool>? onSetDefault;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Material(
      elevation: 8,
      clipBehavior: .antiAlias,
      color: colors.surfaceContainerHigh,
      borderRadius: .circular(24),
      child: Padding(
        padding: const .all(8),
        child: Column(
          mainAxisSize: .min,
          crossAxisAlignment: .stretch,
          children: [
            Padding(
              padding: const .fromLTRB(12, 6, 12, 8),
              child: Row(
                spacing: 8,
                children: [
                  Expanded(
                    child: Text(
                      title,
                      maxLines: 1,
                      overflow: .ellipsis,
                      style: theme.textTheme.labelLarge,
                    ),
                  ),
                  SizedBox(
                    width: 102,
                    child: TextField(
                      controller: search,
                      focusNode: focus,
                      onChanged: onChanged,
                      style: theme.textTheme.bodySmall,
                      decoration: const InputDecoration(
                        hintText: 'Type to filter',
                        isCollapsed: true,
                        border: .none,
                      ),
                    ),
                  ),
                ],
              ),
            ),
            if (loading) const LinearProgressIndicator(),
            if (setDefault case final checked?)
              CheckboxListTile(
                dense: true,
                title: const Text('Set as default'),
                value: checked,
                onChanged: (value) => onSetDefault?.call(value ?? false),
              ),
            for (final (i, item) in items.indexed) ...[
              if (item.danger && i > 0 && !items[i - 1].danger)
                const Divider(height: 1),
              _FileMenuRow(
                item,
                selected: selected == i,
                index: indexes && i < 9 ? i + 1 : null,
              ),
            ],
            if (items.isEmpty && !loading)
              const Padding(
                padding: .all(12),
                child: Text('No matching actions.'),
              ),
          ],
        ),
      ),
    );
  }
}

class _FileMenuRow extends StatelessWidget {
  const _FileMenuRow(this.item, {required this.selected, this.index});

  final FileMenuItem item;
  final bool selected;
  final int? index;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final foreground = item.danger
        ? colors.error
        : selected
        ? colors.onSecondaryContainer
        : colors.onSurface;
    final shape = RoundedRectangleBorder(borderRadius: .circular(12));
    return Semantics(
      selected: selected,
      child: Material(
        color: selected ? colors.secondaryContainer : Colors.transparent,
        shape: shape,
        child: InkWell(
          onTap: item.run,
          customBorder: shape,
          child: SizedBox(
            height: 40,
            child: Padding(
              padding: const .symmetric(horizontal: 12),
              child: Row(
                spacing: 10,
                children: [
                  Icon(item.icon, size: 18, color: foreground),
                  Expanded(
                    child: Text(
                      item.label,
                      maxLines: 1,
                      overflow: .ellipsis,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: foreground,
                      ),
                    ),
                  ),
                  if (index case final index?)
                    LauncherKeycap('$index')
                  else if (item.keys.isNotEmpty)
                    LauncherKeyHint(item.keys),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}
