import 'dart:math' as math;

import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import '../launcher/launcher_results.dart';
import 'emoji_search.dart';

String emojiLocaleTag(Locale locale) => switch (locale.languageCode) {
  'zh' =>
    locale.scriptCode == 'Hant' ||
            (locale.scriptCode == null &&
                const {'HK', 'MO', 'TW'}.contains(locale.countryCode))
        ? 'zh-Hant'
        : 'zh-Hans',
  'ja' || 'ru' || 'de' || 'es' => locale.languageCode,
  _ => 'en',
};

String emojiSearchPlaceholder(Locale locale) =>
    switch (emojiLocaleTag(locale)) {
      'zh-Hans' => '按名称、情绪或短码搜索表情…',
      'zh-Hant' => '按名稱、情緒或短碼搜尋表情…',
      'ja' => '名前、感情、ショートコードで絵文字を検索…',
      'ru' => 'Поиск эмодзи по названию, эмоции или короткому коду…',
      'de' => 'Emoji nach Name, Gefühl oder Kurzcode suchen…',
      'es' => 'Busca emojis por nombre, emoción o código corto…',
      _ => 'Search emoji by name, feeling, or shortcode…',
    };

class EmojiResults extends StatefulWidget {
  const EmojiResults({
    required this.results,
    required this.selectedIndex,
    required this.onCopy,
    required this.onVariants,
    required this.onSelected,
    required this.hasVariants,
    this.loading = false,
    this.unavailable = false,
    super.key,
  });

  final List<EmojiResult> results;
  final int selectedIndex;
  final ValueChanged<EmojiResult> onCopy;
  final ValueChanged<EmojiResult> onVariants;
  final ValueChanged<int> onSelected;
  final bool Function(EmojiResult) hasVariants;
  final bool loading;
  final bool unavailable;

  @override
  State<EmojiResults> createState() => _EmojiResultsState();
}

class _EmojiResultsState extends State<EmojiResults> {
  final _scroll = ScrollController();

  double get _rowExtent {
    final scaler = MediaQuery.textScalerOf(context);
    final subtitle = Theme.of(context).textTheme.bodySmall;
    final titleHeight = scaler.scale(15) * (20 / 15);
    final subtitleHeight =
        scaler.scale(subtitle?.fontSize ?? 12) * (subtitle?.height ?? 1.5);
    return math.max(56, titleHeight + subtitleHeight + 8) + 2;
  }

  @override
  void didUpdateWidget(EmojiResults oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.selectedIndex == oldWidget.selectedIndex &&
        identical(widget.results, oldWidget.results)) {
      return;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || !_scroll.hasClients) return;
      final index = widget.selectedIndex;
      if (index < 0) return;
      final position = _scroll.position;
      final start = index * _rowExtent;
      final end = start + _rowExtent;
      final next = start < position.pixels
          ? start
          : end > position.pixels + position.viewportDimension
          ? math.min(start, end - position.viewportDimension)
          : position.pixels;
      final offset = next.clamp(0.0, position.maxScrollExtent);
      if (offset != position.pixels) _scroll.jumpTo(offset);
    });
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (widget.loading) {
      return const LinearProgressIndicator(semanticsLabel: 'Loading emoji');
    }
    if (widget.unavailable || widget.results.isEmpty) {
      return Padding(
        padding: const .all(12),
        child: Text(
          widget.unavailable
              ? 'Emoji search unavailable'
              : 'No matching emoji. Try a name or shortcode.',
        ),
      );
    }

    final extent = _rowExtent;
    return SizedBox(
      height: math.min(widget.results.length, 8) * extent,
      child: ListView.builder(
        controller: _scroll,
        padding: .zero,
        itemExtent: extent,
        itemCount: widget.results.length,
        itemBuilder: (context, index) {
          final item = widget.results[index];
          void select() {
            if (index != widget.selectedIndex) widget.onSelected(index);
          }

          return _EmojiRow(
            key: ValueKey(item.emoji),
            item: item,
            height: extent - 2,
            selected: index == widget.selectedIndex,
            onFocus: select,
            onCopy: () {
              select();
              widget.onCopy(item);
            },
            onVariants: widget.hasVariants(item)
                ? () {
                    select();
                    widget.onVariants(item);
                  }
                : null,
          );
        },
      ),
    );
  }
}

class _EmojiRow extends StatelessWidget {
  const _EmojiRow({
    required this.item,
    required this.height,
    required this.selected,
    required this.onFocus,
    required this.onCopy,
    required this.onVariants,
    super.key,
  });

  final EmojiResult item;
  final double height;
  final bool selected;
  final VoidCallback onFocus;
  final VoidCallback onCopy;
  final VoidCallback? onVariants;

  @override
  Widget build(BuildContext context) => Focus(
    canRequestFocus: false,
    onFocusChange: (focused) {
      if (focused) onFocus();
    },
    child: Padding(
      padding: const .only(bottom: 2),
      child: LauncherResultRow(
        height: height,
        title: TextSpan(text: item.name),
        subtitle: [
          item.matchedTerm ?? item.group,
          item.codePoints,
        ].where((value) => value.isNotEmpty).join(' · '),
        leading: Tooltip(
          message: item.codePoints,
          excludeFromSemantics: true,
          child: _EmojiGlyph(item.emoji),
        ),
        selected: selected,
        trailing: selected || onVariants != null
            ? Row(
                mainAxisSize: .min,
                children: [
                  if (onVariants != null)
                    IconButton(
                      icon: const LauncherIcon(.more, size: 18),
                      tooltip: 'Variants for ${item.name}',
                      onPressed: onVariants,
                    ),
                  if (selected) const LauncherKeyHint(['↵'], label: 'Copy'),
                ],
              )
            : null,
        onPressed: onCopy,
      ),
    ),
  );
}

class _EmojiGlyph extends StatelessWidget {
  const _EmojiGlyph(this.emoji);

  final String emoji;

  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: Center(
      child: Text(
        emoji,
        maxLines: 1,
        textScaler: TextScaler.noScaling,
        softWrap: false,
        overflow: .clip,
        style: const TextStyle(
          fontSize: 28,
          fontFamilyFallback: ['Noto Color Emoji', 'emoji'],
        ),
      ),
    ),
  );
}

Future<EmojiResult?> showEmojiVariants(
  BuildContext context, {
  required List<EmojiResult> variants,
}) => showDialog<EmojiResult>(
  context: context,
  builder: (context) => SimpleDialog(
    title: const Text('Emoji variants'),
    children: [
      for (final (index, item) in variants.indexed)
        ListTile(
          leading: SizedBox.square(
            dimension: 42,
            child: _EmojiGlyph(item.emoji),
          ),
          title: Text(item.name),
          subtitle: Text(item.codePoints),
          autofocus: index == 0,
          onTap: () => Navigator.of(context).pop(item),
        ),
    ],
  ),
);
