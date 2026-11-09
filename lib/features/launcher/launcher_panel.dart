import 'package:material_ui/material_ui.dart';

import 'launcher_controls.dart';

class LauncherPanel extends StatelessWidget {
  const LauncherPanel({
    required this.search,
    required this.scope,
    required this.body,
    required this.footer,
    this.width = 720,
    this.onScopeSelected,
    this.actions,
    super.key,
  });

  final LauncherSearchField search;
  final LauncherScope scope;
  final Widget body;
  final LauncherFooter footer;
  final double width;
  final ValueChanged<LauncherScope>? onScopeSelected;
  final Widget? actions;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return SizedBox(
      width: width,
      child: Material(
        color: colors.surfaceContainer,
        shadowColor: Colors.black.withValues(alpha: .4),
        elevation: 16,
        clipBehavior: .antiAlias,
        shape: RoundedRectangleBorder(
          side: BorderSide(color: colors.surfaceContainerHighest),
          borderRadius: .circular(32),
        ),
        child: Stack(
          children: [
            Padding(
              padding: const .all(13),
              child: Column(
                mainAxisSize: .min,
                crossAxisAlignment: .stretch,
                spacing: 8,
                children: [
                  search,
                  _ScopeStrip(selected: scope, onSelected: onScopeSelected),
                  Flexible(child: SingleChildScrollView(child: body)),
                  Divider(
                    height: 1,
                    thickness: 1,
                    color: colors.outlineVariant,
                  ),
                  footer,
                ],
              ),
            ),
            if (actions case final actions?)
              PositionedDirectional(
                top: 53,
                start: 21,
                end: 21,
                bottom: 18,
                child: Align(
                  alignment: AlignmentDirectional.topEnd,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 300),
                    child: SingleChildScrollView(child: actions),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _ScopeStrip extends StatelessWidget {
  const _ScopeStrip({required this.selected, this.onSelected});

  final LauncherScope selected;
  final ValueChanged<LauncherScope>? onSelected;

  @override
  Widget build(BuildContext context) => SingleChildScrollView(
    scrollDirection: .horizontal,
    padding: const .symmetric(horizontal: 4),
    child: Row(
      spacing: 6,
      children: [
        for (final scope in LauncherScope.values)
          LauncherScopeChip(
            scope: scope,
            selected: scope == selected,
            onPressed: onSelected == null ? null : () => onSelected!(scope),
          ),
      ],
    ),
  );
}

class LauncherFooter extends StatelessWidget {
  const LauncherFooter({
    required this.leading,
    this.trailing = const [],
    super.key,
  });

  final List<LauncherKeyHint> leading;
  final List<LauncherKeyHint> trailing;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const .only(left: 12, right: 12, bottom: 2),
    child: Wrap(
      alignment: .spaceBetween,
      spacing: 16,
      runSpacing: 6,
      children: [
        Wrap(spacing: 16, runSpacing: 6, children: leading),
        if (trailing.isNotEmpty)
          Wrap(spacing: 16, runSpacing: 6, children: trailing),
      ],
    ),
  );
}

class LauncherSplitBody extends StatelessWidget {
  const LauncherSplitBody({
    required this.results,
    required this.preview,
    required this.previewWidth,
    super.key,
  });

  final Widget results;
  final Widget preview;
  final double previewWidth;

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, constraints) {
      if (constraints.maxWidth < previewWidth + 330) {
        return Column(
          crossAxisAlignment: .stretch,
          spacing: 8,
          children: [results, preview],
        );
      }
      return IntrinsicHeight(
        child: Row(
          crossAxisAlignment: .stretch,
          spacing: 8,
          children: [
            Expanded(child: results),
            SizedBox(width: previewWidth, child: preview),
          ],
        ),
      );
    },
  );
}
