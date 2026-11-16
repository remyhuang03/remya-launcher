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
    this.enabledScopes,
    this.scrollBody = true,
    this.onOverlay = false,
    this.compact = false,
    this.actions,
    this.actionsBodyHeight = 286,
    this.actionsWidth = 300,
    super.key,
  });

  final LauncherSearchField search;
  final LauncherScope scope;
  final Widget body;
  final LauncherFooter footer;
  final double width;
  final ValueChanged<LauncherScope>? onScopeSelected;
  final Set<LauncherScope>? enabledScopes;
  final bool scrollBody;
  final bool onOverlay;
  final bool compact;
  final Widget? actions;
  final double actionsBodyHeight, actionsWidth;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final radius = BorderRadius.circular(compact ? 22 : 32);
    final outline = BorderSide(color: colors.surfaceContainerHighest);
    final shadow = Colors.black.withValues(alpha: .4);
    final contents = Stack(
      children: [
        Padding(
          padding: .all(compact ? 8 : 13),
          child: Column(
            mainAxisSize: .min,
            crossAxisAlignment: .stretch,
            spacing: compact ? 5 : 8,
            children: [
              search,
              _ScopeStrip(
                selected: scope,
                onSelected: onScopeSelected,
                enabledScopes: enabledScopes,
                compact: compact,
              ),
              Flexible(
                child: scrollBody
                    ? SingleChildScrollView(
                        child: actions == null
                            ? body
                            : ConstrainedBox(
                                constraints: BoxConstraints(
                                  minHeight: actionsBodyHeight,
                                ),
                                child: body,
                              ),
                      )
                    : body,
              ),
              Divider(height: 1, thickness: 1, color: colors.outlineVariant),
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
                constraints: BoxConstraints(maxWidth: actionsWidth),
                child: SingleChildScrollView(child: actions),
              ),
            ),
          ),
      ],
    );
    return SizedBox(
      width: width,
      child: onOverlay
          ? DecoratedBox(
              decoration: BoxDecoration(
                color: colors.surfaceContainer,
                border: Border.fromBorderSide(outline),
                borderRadius: radius,
                boxShadow: [BoxShadow(color: shadow, blurRadius: 24)],
              ),
              child: ClipRRect(
                borderRadius: radius,
                child: Material(type: .transparency, child: contents),
              ),
            )
          : Material(
              color: colors.surfaceContainer,
              shadowColor: shadow,
              elevation: 16,
              clipBehavior: .antiAlias,
              shape: RoundedRectangleBorder(
                side: outline,
                borderRadius: radius,
              ),
              child: contents,
            ),
    );
  }
}

class _ScopeStrip extends StatelessWidget {
  const _ScopeStrip({
    required this.selected,
    this.onSelected,
    this.enabledScopes,
    required this.compact,
  });

  final LauncherScope selected;
  final ValueChanged<LauncherScope>? onSelected;
  final Set<LauncherScope>? enabledScopes;
  final bool compact;

  @override
  Widget build(BuildContext context) => SingleChildScrollView(
    scrollDirection: .horizontal,
    padding: const .symmetric(horizontal: 4),
    child: Row(
      spacing: 6,
      children: [
        for (final scope in LauncherScope.values)
          if (scope == selected ||
              enabledScopes == null ||
              (scope != .run && !compact) ||
              enabledScopes!.contains(scope))
            LauncherScopeChip(
              scope: scope,
              selected: scope == selected,
              onPressed:
                  onSelected == null ||
                      !(enabledScopes?.contains(scope) ?? true)
                  ? null
                  : () => onSelected!(scope),
            ),
      ],
    ),
  );
}

class LauncherFooter extends StatelessWidget {
  const LauncherFooter({
    required this.leading,
    this.trailing = const [],
    this.singleLine = false,
    super.key,
  });

  final List<LauncherKeyHint> leading;
  final List<LauncherKeyHint> trailing;
  final bool singleLine;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const .only(left: 12, right: 12, bottom: 2),
    child: singleLine
        ? SingleChildScrollView(
            scrollDirection: .horizontal,
            child: Row(spacing: 10, children: [...leading, ...trailing]),
          )
        : Wrap(
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
