import 'package:flutter_svg/flutter_svg.dart';
import 'package:material_ui/material_ui.dart';

enum LauncherIconName {
  search,
  apps,
  calculate,
  folder,
  description,
  image,
  public,
  history,
  terminal,
  clipboard,
  mood,
  window,
  bolt,
  power,
  star,
  copy,
  openNew,
  swap,
  tune,
  arrow,
  more,
  code,
  trash,
  lock,
  text,
  palette,
  pi,
  dataObject,
  warning,
  memory,
  money,
  schedule,
  straighten,
  functions,
  visibilityOff,
  download,
  bookmark,
}

class LauncherIcon extends StatelessWidget {
  const LauncherIcon(this.icon, {this.size = 20, this.color, super.key});

  final LauncherIconName icon;
  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: FittedBox(
        child: SvgPicture.asset(
          'assets/icons/${icon.name.toLowerCase()}.svg',
          colorFilter: ColorFilter.mode(
            color ?? Theme.of(context).colorScheme.onSurfaceVariant,
            .srcIn,
          ),
        ),
      ),
    );
  }
}

double _textWidth(BuildContext context, String text, TextStyle style) {
  final resolved = DefaultTextStyle.of(context).style.merge(style);
  final painter = TextPainter(
    text: TextSpan(
      text: text,
      style: MediaQuery.boldTextOf(context)
          ? resolved.merge(const TextStyle(fontWeight: .bold))
          : resolved,
    ),
    textDirection: Directionality.of(context),
    textScaler: MediaQuery.textScalerOf(context),
    maxLines: 1,
  )..layout();
  final width = painter.width;
  painter.dispose();
  return width;
}

class LauncherKeycap extends StatelessWidget {
  const LauncherKeycap(this.label, {this.compact = false, super.key});

  final String label;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final height = compact ? 16.0 : 20.0;
    return Container(
      height: height,
      constraints: BoxConstraints(minWidth: height),
      alignment: Alignment.center,
      padding: EdgeInsets.symmetric(horizontal: compact ? 3 : 5),
      decoration: ShapeDecoration(
        shape: RoundedRectangleBorder(
          borderRadius: .circular(6),
          side: BorderSide(color: theme.colorScheme.outlineVariant),
        ),
      ),
      child: Text(
        label,
        style: theme.textTheme.labelSmall?.copyWith(
          fontSize: compact ? 10 : 11,
          letterSpacing: 0,
          color: theme.colorScheme.onSurfaceVariant,
        ),
      ),
    );
  }
}

class LauncherKeyHint extends StatelessWidget {
  const LauncherKeyHint(this.keys, {this.label, super.key});

  final List<String> keys;
  final String? label;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: .min,
    spacing: 4,
    children: [
      for (final key in keys) LauncherKeycap(key),
      if (label case final label?)
        Flexible(
          child: Text(label, style: Theme.of(context).textTheme.bodySmall),
        ),
    ],
  );
}

class LauncherQueryToken extends StatelessWidget {
  const LauncherQueryToken(
    this.label, {
    required this.onRemove,
    this.icon = .search,
    required this.removeLabel,
    super.key,
  });

  final String label;
  final LauncherIconName icon;
  final String removeLabel;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Tooltip(
      message: removeLabel,
      child: Material(
        color: colors.tertiaryContainer,
        borderRadius: .circular(10),
        child: InkWell(
          borderRadius: .circular(10),
          onTap: onRemove,
          child: Padding(
            padding: const .symmetric(horizontal: 10, vertical: 7),
            child: Row(
              mainAxisSize: .min,
              spacing: 6,
              children: [
                LauncherIcon(icon, size: 16, color: colors.onTertiaryContainer),
                Text(
                  label,
                  style: TextStyle(
                    color: colors.onTertiaryContainer,
                    fontSize: 13,
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

enum LauncherScope {
  all('All', .search),
  apps('Apps', .apps),
  files('Files', .folder),
  web('Web', .public),
  calculator('Calc', .calculate),
  clipboard('Clipboard', .clipboard),
  windows('Windows', .window),
  emoji('Chars', .mood),
  actions('Actions', .bolt);

  const LauncherScope(this.label, this.icon);

  final String label;
  final LauncherIconName icon;

  String get shortcutNumber => (index + 1).toString();

  static LauncherScope? fromWindowRequest(Object? value) => switch (value) {
    'clipboard' => .clipboard,
    'characters' => .emoji,
    _ => null,
  };
}

class LauncherScopeChip extends StatelessWidget {
  const LauncherScopeChip({
    required this.scope,
    this.selected = false,
    this.onPressed,
    super.key,
  });

  final LauncherScope scope;
  final bool selected;
  final VoidCallback? onPressed;

  static const _spacing = 6.0;
  static const _iconSize = 18.0;
  static const _labelStyle = TextStyle(fontSize: 13, fontWeight: .w500);
  double get _padding => selected ? 8 : 9;

  double preferredWidth(BuildContext context) =>
      2 * _padding +
      _iconSize +
      _spacing +
      _textWidth(context, scope.label, _labelStyle);

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final foreground = selected
        ? colors.onSecondaryContainer
        : colors.onSurfaceVariant;
    final shape = RoundedRectangleBorder(
      borderRadius: .circular(selected ? 10 : 16),
      side: selected ? .none : BorderSide(color: colors.outlineVariant),
    );
    return Semantics(
      selected: selected,
      child: Material(
        shape: shape,
        color: selected ? colors.secondaryContainer : Colors.transparent,
        child: InkWell(
          customBorder: shape,
          onTap: onPressed,
          child: Padding(
            padding: .symmetric(horizontal: _padding),
            child: SizedBox(
              height: 32,
              child: Row(
                mainAxisSize: .min,
                spacing: _spacing,
                children: [
                  LauncherIcon(scope.icon, size: _iconSize, color: foreground),
                  Text(
                    scope.label,
                    style: _labelStyle.copyWith(color: foreground),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class LauncherSearchField extends StatelessWidget {
  const LauncherSearchField({
    this.initialQuery = '',
    this.placeholder = 'Search apps, files, history…',
    this.scope = .all,
    this.trailing,
    this.queryToken,
    this.completionSuffix,
    this.autofocus = false,
    this.compact = false,
    this.controller,
    this.focusNode,
    this.onChanged,
    this.onSubmitted,
    super.key,
  });

  final String initialQuery;
  final String placeholder;
  final LauncherScope scope;
  final Widget? trailing, queryToken;
  final String? completionSuffix;
  final bool autofocus;
  final bool compact;
  final TextEditingController? controller;
  final FocusNode? focusNode;
  final ValueChanged<String>? onChanged;
  final ValueChanged<String>? onSubmitted;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final textStyle = TextStyle(fontSize: 18, color: colors.onSurface);
    final suffix = controller == null ? null : completionSuffix;
    final field = Semantics(
      label: placeholder,
      child: TextFormField(
        initialValue: controller == null ? initialQuery : null,
        controller: controller,
        focusNode: focusNode,
        autofocus: autofocus,
        onChanged: onChanged,
        onFieldSubmitted: onSubmitted,
        cursorHeight: 24,
        cursorOpacityAnimates: true,
        style: textStyle,
        decoration: InputDecoration(
          hintText: suffix == null ? placeholder : null,
          hintStyle: TextStyle(color: colors.onSurfaceVariant),
          isCollapsed: true,
          border: .none,
        ),
      ),
    );
    return Container(
      height: compact ? 46 : 56,
      padding: .only(left: scope == .all ? 16 : 12, right: 14),
      decoration: BoxDecoration(
        color: colors.surfaceContainerHighest,
        borderRadius: .circular(28),
      ),
      child: Row(
        spacing: 10,
        children: [
          if (scope == .all)
            const LauncherIcon(.search, size: 24)
          else
            _ScopeToken(scope, compact: compact),
          ?queryToken,
          Expanded(
            key: const ValueKey('launcher-search-field'),
            child: Stack(
              alignment: .centerLeft,
              children: [
                RepaintBoundary(child: field),
                if (suffix != null)
                  Positioned.fill(
                    child: IgnorePointer(
                      child: ExcludeSemantics(
                        child: Align(
                          alignment: .centerLeft,
                          child: Text.rich(
                            TextSpan(
                              style: textStyle,
                              children: [
                                TextSpan(
                                  text: controller!.text,
                                  style: const TextStyle(
                                    color: Colors.transparent,
                                  ),
                                ),
                                TextSpan(
                                  text: suffix,
                                  style: TextStyle(
                                    color: colors.onSurfaceVariant,
                                  ),
                                ),
                              ],
                            ),
                            maxLines: 1,
                            softWrap: false,
                            overflow: .clip,
                          ),
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          ),
          if (!compact) ?trailing,
        ],
      ),
    );
  }
}

class _ScopeToken extends StatelessWidget {
  const _ScopeToken(this.scope, {required this.compact});

  final LauncherScope scope;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Container(
      height: 32,
      padding: .only(left: 8, right: compact ? 8 : 10),
      decoration: BoxDecoration(
        color: colors.primaryContainer,
        borderRadius: .circular(10),
      ),
      child: Row(
        spacing: 6,
        children: [
          Tooltip(
            message: scope.label,
            excludeFromSemantics: !compact,
            child: LauncherIcon(
              scope.icon,
              size: 18,
              color: colors.onPrimaryContainer,
            ),
          ),
          if (!compact)
            Text(
              scope.label,
              style: TextStyle(fontSize: 14, color: colors.onPrimaryContainer),
            ),
        ],
      ),
    );
  }
}
