import 'package:material_ui/material_ui.dart';

import '../launcher/launcher_controls.dart';
import '../launcher/launcher_details.dart';
import '../launcher/launcher_results.dart';
import 'calculator_controller.dart';

/// Main calculator result, discovery, history, and error view for `Calc` scope.
class CalculatorResults extends StatelessWidget {
  const CalculatorResults({
    required this.controller,
    required this.compact,
    required this.onCopyResult,
    required this.onCopyText,
    required this.onApplyEdit,
    required this.onFillQuery,
    this.onClearHistory,
    super.key,
  });

  final CalculatorController controller;
  final bool compact;
  final VoidCallback onCopyResult;
  final ValueChanged<String> onCopyText;
  final ValueChanged<CalculatorTextEdit> onApplyEdit;
  final ValueChanged<String> onFillQuery;
  final VoidCallback? onClearHistory;

  @override
  Widget build(BuildContext context) {
    if (controller.backendError case final error?) {
      return _UnavailableBanner(message: error, onRetry: controller.retry);
    }

    if (controller.query.trim().isEmpty) {
      return _HistoryView(
        controller: controller,
        compact: compact,
        onCopy: onCopyText,
        onEdit: onFillQuery,
        onClearHistory: onClearHistory,
      );
    }

    final candidate = controller.selectedCandidate;
    final correctionEdit = controller.pendingCorrectionEdit;
    final showError = controller.shouldShowError;
    final completions = controller.completions;
    final catalog = controller.catalogEntries;
    final refreshRates = controller.hasRateDiagnostic
        ? controller.refreshRates
        : null;

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: 8,
      children: [
        if (controller.loadingPreview && candidate == null && !showError)
          const LinearProgressIndicator(semanticsLabel: 'Evaluating query'),
        if (showError)
          _ErrorPanel(
            diagnostics: controller.errorDiagnostics,
            selectedIndex: controller.selectedSuggestionIndex,
            fxStatusMessage: controller.fxStatusMessage,
            onRefreshRates: refreshRates,
            onSuggestionSelected: (suggestion) {
              if (suggestion.replacement case final edit?) {
                onApplyEdit(edit);
              } else {
                onFillQuery(suggestion.label);
              }
            },
          ),
        if (!showError &&
            candidate != null &&
            candidate.kind != 'catalog_search') ...[
          _HeroSection(
            candidate: candidate,
            controller: controller,
            onCopy: controller.isCopyableResult ? onCopyResult : null,
            onApplyCorrection: correctionEdit == null
                ? null
                : () => onApplyEdit(correctionEdit),
          ),
          if (candidate.floatValues.isNotEmpty)
            _FormatTable(
              title: 'ALL FORMATS',
              rows: [
                for (final id in ['fp32', 'fp16', 'bf16'])
                  for (final value in candidate.floatValues.where(
                    (v) => v.str('format') == id,
                  ))
                    (
                      id,
                      '${value.str('bitsHex')}   ${_shortDecimal(value.str('storedDecimal'))}   ${_shortDecimal(value.str('absoluteError'))}',
                    ),
              ],
              selected: controller.selectedFormatId,
              onSelect: controller.selectFormat,
            ),
          if (candidate.value?.tag == 'currency')
            _FavouriteCurrencies(candidate: candidate),
          if (candidate.value?.tag == 'time')
            _PinnedZones(
              candidate: candidate,
              onSelect: controller.selectFormat,
            ),
          if (candidate.kind == 'math')
            _ExpressionReferences(expression: candidate.normalized),
          if (controller.candidates.length == 1 &&
              correctionEdit == null &&
              (candidate.kind == 'math' || candidate.kind == 'unit_conversion'))
            _HistoryView(
              controller: controller,
              compact: compact,
              onCopy: onCopyText,
              onEdit: onFillQuery,
              relatedKind: candidate.kind,
            ),
        ],
        if (!showError && candidate == null && completions.isNotEmpty)
          _CompletionPreview(
            query: controller.query,
            completion: completions[controller.selectedCompletionIndex],
          ),
        if ((candidate == null, controller.selectedCatalogEntry?.definition)
            case (true, final definition?))
          LauncherAnswerCard(
            expression:
                '${catalog[controller.selectedCatalogIndex].symbol} · ${catalog[controller.selectedCatalogIndex].name}',
            value: _groupInteger(definition.split(' ').first),
            unit: definition.contains(' ')
                ? definition.substring(definition.indexOf(' ') + 1)
                : '',
            alternatives: const [],
            onCopy: () => onCopyText(definition),
          ),
        if (!showError && controller.candidates.length > 1)
          _CandidateList(
            candidates: controller.candidates,
            selectedId: candidate?.id,
            onSelected: controller.selectCandidate,
          ),
        if (completions.isNotEmpty)
          _CompletionList(
            completions: completions,
            selectedIndex: controller.selectedCompletionIndex,
            onApply: (completion) {
              if (completion.edit case final edit?) {
                onApplyEdit(edit);
              } else {
                onFillQuery(completion.label);
              }
            },
          ),
        if (catalog.isNotEmpty)
          _CatalogList(
            entries: catalog,
            selectedIndex: controller.selectedCatalogIndex,
            onInsert: (entry) => onFillQuery(entry.insertText),
          ),
        if (!showError &&
            !controller.loadingPreview &&
            candidate == null &&
            completions.isEmpty &&
            catalog.isEmpty)
          _EmptyHint(
            controller.hasPendingError
                ? 'Checking expression…'
                : 'No calculator result.',
          ),
      ],
    );
  }
}

/// Compact inline calculator group rendered inside the `All` launcher scope.
class CalculatorInlineResult extends StatelessWidget {
  const CalculatorInlineResult({
    required this.controller,
    required this.onOpenInCalc,
    required this.onCopy,
    super.key,
  });

  final CalculatorController controller;
  final VoidCallback onOpenInCalc;
  final VoidCallback onCopy;

  @override
  Widget build(BuildContext context) {
    final candidate = controller.selectedCandidate;
    if (candidate == null) return const SizedBox.shrink();
    final value = candidate.primaryDisplay(controller.selectedFormatId);
    final colors = Theme.of(context).colorScheme;
    return Material(
      color: colors.primaryContainer,
      borderRadius: .circular(20),
      clipBehavior: .antiAlias,
      child: InkWell(
        onTap: onOpenInCalc,
        child: Padding(
          padding: const .symmetric(horizontal: 16, vertical: 12),
          child: Row(
            spacing: 14,
            children: [
              const SizedBox.square(
                dimension: 40,
                child: LauncherSymbolTile(.calculate, selected: true),
              ),
              Expanded(
                child: DefaultTextStyle.merge(
                  style: TextStyle(color: colors.onPrimaryContainer),
                  child: Column(
                    crossAxisAlignment: .start,
                    children: [
                      Text(
                        candidate.normalized,
                        style: const TextStyle(fontSize: 12),
                      ),
                      Text(
                        _groupInteger(value),
                        style: const TextStyle(fontSize: 24, fontWeight: .w700),
                      ),
                    ],
                  ),
                ),
              ),
              TextButton(
                onPressed: onCopy,
                child: const LauncherKeyHint(['↵'], label: 'Copy'),
              ),
              if (MediaQuery.sizeOf(context).width >= 540)
                const LauncherKeyHint(['Tab'], label: 'Open in Calc'),
            ],
          ),
        ),
      ),
    );
  }
}

String _groupInteger(String value) => RegExp(r'^-?\d+$').hasMatch(value)
    ? value.replaceAllMapped(
        RegExp(r'\d(?=(\d{3})+$)'),
        (match) => '${match[0]},',
      )
    : value;

class _HeroSection extends StatelessWidget {
  const _HeroSection({
    required this.candidate,
    required this.controller,
    this.onCopy,
    this.onApplyCorrection,
  });

  final CalculatorCandidate candidate;
  final CalculatorController controller;
  final VoidCallback? onCopy;
  final VoidCallback? onApplyCorrection;

  static final _quantity = RegExp(
    r'^([+-]?[\d,]+(?:\.\d+)?(?:[eE][+-]?\d+)?) (.+)$',
  );

  @override
  Widget build(BuildContext context) {
    final selected = controller.selectedFormatId;
    final chipFormats = candidate.formats
        .where((f) => !f.id.startsWith('pinned:'))
        .toList();
    final float = candidate.floatValue(selected);
    final programmer = candidate.kind == 'programmer';
    final time = candidate.value?.tag == 'time' ? candidate.value : null;
    final currency = candidate.value?.tag == 'currency';
    final pinned = selected?.startsWith('pinned:') == true;
    final display =
        float?.str('storedDecimal') ?? candidate.primaryDisplay(selected);
    final approximate = display.startsWith('approx. ');
    final answer = approximate ? display.substring(8) : display;
    final quantity = _quantity.firstMatch(answer);
    final expression = programmer || approximate
        ? (approximate ? candidate.interpretation : candidate.normalized)
        : float != null
        ? '${candidate.value?.str('inputDecimal')} → ${float.str('format')}'
        : candidate.interpretation.isEmpty
        ? candidate.normalized
        : candidate.interpretation;
    return LauncherAnswerCard(
      expression: programmer
          ? '$expression · ${controller.settings.shortLabel}'
          : expression,
      value: time == null
          ? _groupInteger(quantity?[1] ?? answer)
          : (selected == 'iso' || selected == 'unix')
          ? display
          : _clockText(pinned ? display : time.str('targetCivil') ?? display),
      unit: time == null
          ? (quantity?[2] ?? '')
          : pinned
          ? selected!.substring(7)
          : _timeDate(time),
      alternatives: programmer || float != null
          ? const []
          : [for (final format in chipFormats) format.label],
      selectedAlternative: chipFormats.indexWhere(
        (format) => format.id == selected,
      ),
      onAlternativeSelected: (index) =>
          controller.selectFormat(chipFormats[index].id),
      onCopy: onCopy,
      details: [
        if (approximate)
          const Text('Approximate', style: TextStyle(fontSize: 12)),
        if (programmer) ...[
          _FormatTable(
            inset: true,
            rows: [
              for (final format in candidate.formats) (format.id, format.label),
            ],
            selected: selected,
            onSelect: controller.selectFormat,
          ),
          if (candidate.formatById('binary') case final binary?)
            _BitsPanel(
              groups: [
                (
                  '${candidate.value?.str('bitWidth') ?? ''} bits',
                  binary.copyText
                      .replaceFirst('0b', '')
                      .replaceAll('_', '')
                      .padLeft(controller.settings.bitWidth, '0'),
                ),
              ],
            ),
          _ProgrammerBar(controller: controller),
        ],
        if (float != null) _FloatBitsStrip(value: float),
        if (currency)
          _CurrencyProvenance(
            candidate: candidate,
            statusMessage: controller.fxStatusMessage,
            onRefresh: controller.refreshRates,
          ),
        if (onApplyCorrection != null ||
            (controller.warningDiagnostics.isNotEmpty && !currency))
          _NoticeBanner(
            warnings: controller.warningDiagnostics,
            correctionEdit: controller.pendingCorrectionEdit,
            onApplyCorrection: onApplyCorrection,
          ),
        if (candidate.kind == 'unit_conversion' &&
            controller.candidates.length > 1)
          Text(
            'Read as · ${candidate.assumptions.join(' · ')}',
            style: const TextStyle(fontSize: 12),
          ),
        if (candidate.kind == 'chemistry_conversion' &&
            candidate.assumptions.isNotEmpty)
          Text(
            candidate.assumptions.join(' · '),
            style: const TextStyle(fontSize: 12),
          ),
      ],
    );
  }
}

String _clockText(String civil) =>
    RegExp(r'\d{2}:\d{2}').firstMatch(civil)?.group(0) ?? civil;

String _timeDate(CalculatorValue value) {
  final target = DateTime.tryParse(value.str('targetCivil') ?? '');
  final source = DateTime.tryParse(value.str('sourceCivil') ?? '');
  if (target == null) return value.str('targetZoneId') ?? '';
  final rollover = source == null
      ? 0
      : DateTime.utc(target.year, target.month, target.day)
            .difference(DateTime.utc(source.year, source.month, source.day))
            .inDays;
  return [
    '${['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'][target.weekday - 1]}, ${['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'][target.month - 1]} ${target.day}',
    if (rollover == 1) 'next day' else if (rollover == -1) 'previous day',
  ].join(' · ');
}

String _shortDecimal(String? decimal) {
  if (decimal == null) return '';
  if (decimal.length <= 15) return decimal;
  final value = double.tryParse(decimal);
  return value == null || !value.isFinite
      ? decimal
      : value.toStringAsPrecision(11);
}

class _FormatTable extends StatelessWidget {
  const _FormatTable({
    required this.rows,
    required this.selected,
    required this.onSelect,
    this.title,
    this.inset = false,
  });
  final List<(String, String)> rows;
  final String? selected;
  final ValueChanged<String> onSelect;
  final String? title;
  final bool inset;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Material(
      color: inset ? colors.surfaceContainer : Colors.transparent,
      borderRadius: .circular(16),
      child: Padding(
        padding: .all(inset ? 6 : 0),
        child: Column(
          crossAxisAlignment: .stretch,
          children: [
            if (title case final title?) LauncherSectionHeader(title),
            for (final (index, (id, label)) in rows.indexed)
              InkWell(
                borderRadius: .circular(10),
                onTap: () => onSelect(id),
                child: Ink(
                  decoration: BoxDecoration(
                    color: selected == id ? colors.secondaryContainer : null,
                    borderRadius: .circular(10),
                  ),
                  child: Padding(
                    padding: const .symmetric(horizontal: 12, vertical: 6),
                    child: Row(
                      spacing: 8,
                      children: [
                        SizedBox(
                          width: 54,
                          child: Text(
                            switch (id) {
                              'decimal' => 'DEC',
                              'octal' => 'OCT',
                              'binary' => 'BIN',
                              _ => id.toUpperCase(),
                            },
                            style: TextStyle(
                              fontSize: 11,
                              color: colors.onSurfaceVariant,
                            ),
                          ),
                        ),
                        Expanded(
                          child: Text(
                            label,
                            style: TextStyle(
                              color: colors.onSurface,
                              fontFamily: 'monospace',
                            ),
                          ),
                        ),
                        LauncherKeycap('${index + 1}', compact: true),
                      ],
                    ),
                  ),
                ),
              ),
          ],
        ),
      ),
    );
  }
}

class _ProgrammerBar extends StatelessWidget {
  const _ProgrammerBar({required this.controller});

  final CalculatorController controller;

  @override
  Widget build(BuildContext context) {
    final settings = controller.settings;
    return Wrap(
      spacing: 8,
      runSpacing: 6,
      crossAxisAlignment: .center,
      children: [
        for (final (width, signed) in [
          (8, false),
          (16, false),
          (32, false),
          (64, false),
          (32, true),
          if (settings.bitWidth == 128 ||
              settings.signed && settings.bitWidth != 32)
            (settings.bitWidth, settings.signed),
        ])
          _SettingPill(
            label: '${signed ? 'i' : 'u'}$width',
            selected: settings.bitWidth == width && settings.signed == signed,
            onPressed: () =>
                controller.setProgrammerMode(width, signed: signed),
          ),
        const LauncherKeyHint(['Ctrl', 'B'], label: 'Bit width'),
      ],
    );
  }
}

class _SettingPill extends StatelessWidget {
  const _SettingPill({
    required this.label,
    required this.onPressed,
    this.selected = false,
  });

  final String label;
  final VoidCallback onPressed;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final shape = RoundedRectangleBorder(
      borderRadius: .circular(10),
      side: BorderSide(color: colors.outlineVariant),
    );
    return Material(
      color: selected ? colors.onPrimaryContainer : colors.primaryContainer,
      shape: shape,
      child: InkWell(
        customBorder: shape,
        onTap: onPressed,
        child: Padding(
          padding: const .symmetric(horizontal: 10, vertical: 4),
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
    );
  }
}

class _FloatBitsStrip extends StatelessWidget {
  const _FloatBitsStrip({required this.value});

  final CalculatorValue value;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: .stretch,
      spacing: 8,
      children: [
        Text(
          [
            if (value.str('bitsHex') case final bits?) 'stored as $bits',
            if (value.str('absoluteError') case final error?) 'error $error',
            if (value.str('relativeErrorPercent') case final percent?)
              '($percent%)',
          ].join(' · '),
          style: const TextStyle(fontSize: 12),
        ),
        _BitsPanel(
          groups: [
            if (value.str('signBit') case final bits?) ('sign', bits),
            if (value.str('exponentBits') case final bits?) ('exponent', bits),
            if (value.str('fractionBits') case final bits?) ('mantissa', bits),
          ],
        ),
      ],
    );
  }
}

class _BitsPanel extends StatelessWidget {
  const _BitsPanel({required this.groups});
  final List<(String, String)> groups;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Container(
      width: double.infinity,
      padding: const .all(12),
      decoration: BoxDecoration(
        color: colors.surfaceContainer,
        borderRadius: .circular(16),
      ),
      child: Wrap(
        alignment: .center,
        spacing: 18,
        runSpacing: 6,
        children: [
          for (final (label, bits) in groups)
            Column(
              mainAxisSize: .min,
              spacing: 4,
              children: [
                Wrap(
                  alignment: .center,
                  spacing: 2,
                  runSpacing: 3,
                  children: [
                    for (final bit in bits.split(''))
                      Container(
                        padding: const .symmetric(horizontal: 3, vertical: 2),
                        decoration: BoxDecoration(
                          color: bit == '1'
                              ? (label == 'exponent'
                                    ? colors.tertiaryContainer
                                    : colors.primaryContainer)
                              : null,
                          border: Border.all(color: colors.outlineVariant),
                          borderRadius: .circular(3),
                        ),
                        child: Text(
                          bit,
                          style: TextStyle(
                            fontSize: 10,
                            fontFamily: 'monospace',
                            color: bit == '1'
                                ? (label == 'exponent'
                                      ? colors.onTertiaryContainer
                                      : colors.onPrimaryContainer)
                                : colors.onSurface,
                          ),
                        ),
                      ),
                  ],
                ),
                Text(
                  label,
                  style: TextStyle(
                    fontSize: 11,
                    color: colors.onSurfaceVariant,
                  ),
                ),
              ],
            ),
        ],
      ),
    );
  }
}

class _PinnedZones extends StatelessWidget {
  const _PinnedZones({required this.candidate, required this.onSelect});
  final CalculatorCandidate candidate;
  final ValueChanged<String> onSelect;

  @override
  Widget build(BuildContext context) {
    final pinned = candidate.formats.where(
      (format) => format.id.startsWith('pinned:'),
    );
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        if (pinned.isNotEmpty) const LauncherSectionHeader('PINNED ZONES'),
        for (final format in pinned)
          LauncherResultRow(
            title: TextSpan(
              text:
                  '${format.id.substring(7).split('/').last.replaceAll('_', ' ')} · ${_clockText(format.label)}',
            ),
            subtitle: format.label
                .substring(_clockText(format.label).length)
                .trim(),
            leading: const LauncherSymbolTile(.schedule),
            onPressed: () => onSelect(format.id),
          ),
      ],
    );
  }
}

class _ExpressionReferences extends StatelessWidget {
  const _ExpressionReferences({required this.expression});
  final String expression;

  @override
  Widget build(BuildContext context) {
    final hasRoot = RegExp(r'\bsqrt\b').hasMatch(expression);
    final hasPi =
        expression.contains('π') ||
        RegExp(r'(?<![a-zA-Z_])pi\b').hasMatch(expression);
    if (!hasRoot && !hasPi) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        const LauncherSectionHeader('IN THIS EXPRESSION'),
        if (hasRoot)
          const LauncherResultRow(
            title: TextSpan(text: 'sqrt(x)'),
            subtitle: 'Square root',
            leading: LauncherSymbolTile(.functions),
          ),
        if (hasPi)
          const LauncherResultRow(
            title: TextSpan(text: 'π'),
            subtitle: 'Pi · 3.141592653589793…',
            leading: LauncherSymbolTile(.pi),
          ),
      ],
    );
  }
}

class _CompletionPreview extends StatelessWidget {
  const _CompletionPreview({required this.query, required this.completion});
  final String query;
  final CalculatorCompletion completion;

  @override
  Widget build(BuildContext context) {
    final value = completion.edit?.applyTo(TextEditingValue(text: query));
    if (value == null) return const SizedBox.shrink();
    return Container(
      padding: const .symmetric(horizontal: 24, vertical: 18),
      decoration: BoxDecoration(
        color: Theme.of(context).colorScheme.surfaceContainerHighest,
        borderRadius: .circular(28),
      ),
      child: Column(
        crossAxisAlignment: .start,
        spacing: 6,
        children: [
          const Text('Preview with the selected completion'),
          Text(value.text, style: Theme.of(context).textTheme.headlineSmall),
        ],
      ),
    );
  }
}

class _FavouriteCurrencies extends StatelessWidget {
  const _FavouriteCurrencies({required this.candidate});
  final CalculatorCandidate candidate;

  @override
  Widget build(BuildContext context) {
    final rows = candidate.value?.raw['favourites'];
    if (rows is! List || rows.isEmpty) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        const LauncherSectionHeader('FAVOURITE CURRENCIES'),
        for (final row in rows.whereType<Map>())
          LauncherResultRow(
            title: TextSpan(text: row['display']?.toString()),
            subtitle: row['name']?.toString() ?? '',
            leading: const LauncherSymbolTile(.money),
            trailing: Text(row['currency']?.toString() ?? ''),
          ),
      ],
    );
  }
}

class _CurrencyProvenance extends StatelessWidget {
  const _CurrencyProvenance({
    required this.candidate,
    required this.statusMessage,
    required this.onRefresh,
  });

  final CalculatorCandidate candidate;
  final String? statusMessage;
  final VoidCallback onRefresh;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final val = candidate.value;
    final prov = candidate.provenance ?? const {};
    final source =
        (prov['providerName'] ??
                prov['source'] ??
                prov['sourceId'] ??
                prov['provider'])
            ?.toString() ??
        val?.str('rateId');
    final asOf =
        (val?.str('asOf') ??
                val?.str('effectiveAt') ??
                val?.str('fetchedAt') ??
                prov['asOf'] ??
                prov['effectiveAt'] ??
                prov['fetchedAt'])
            ?.toString();
    final freshness = (val?.str('freshness') ?? prov['freshness'])?.toString();
    final stale =
        val?.flag('stale') == true ||
        prov['stale'] == true ||
        freshness == 'stale_usable' ||
        freshness == 'expired_blocked';

    return Row(
      spacing: 8,
      children: [
        Expanded(
          child: Text(
            [
              if (val?.str('rateDecimal') case final rate?)
                '1 ${val?.str('source')} = $rate ${val?.str('target')}',
              if (source != null && source.isNotEmpty) 'Source: $source',
              if (asOf != null && asOf.isNotEmpty) 'As of: $asOf',
              if (stale) 'Stale cached rate',
              ?statusMessage,
            ].join(' · '),
            style: theme.textTheme.bodySmall?.copyWith(
              color: stale ? colors.error : null,
            ),
          ),
        ),
        TextButton(onPressed: onRefresh, child: const Text('Refresh')),
      ],
    );
  }
}

class _CandidateList extends StatelessWidget {
  const _CandidateList({
    required this.candidates,
    required this.selectedId,
    required this.onSelected,
  });

  final List<CalculatorCandidate> candidates;
  final String? selectedId;
  final ValueChanged<String> onSelected;

  @override
  Widget build(BuildContext context) => Column(
    mainAxisSize: .min,
    crossAxisAlignment: .stretch,
    children: [
      const LauncherSectionHeader('OTHER READINGS'),
      for (final candidate in candidates)
        LauncherResultRow(
          title: TextSpan(text: candidate.primaryDisplay()),
          subtitle: [
            if (candidate.interpretation.isNotEmpty) candidate.interpretation,
            ...candidate.assumptions,
          ].join(' · '),
          leading: LauncherSymbolTile(
            _iconForKind(candidate.kind),
            selected: candidate.id == selectedId,
          ),
          selected: candidate.id == selectedId,
          onPressed: () => onSelected(candidate.id),
        ),
    ],
  );
}

class _CompletionList extends StatelessWidget {
  const _CompletionList({
    required this.completions,
    required this.selectedIndex,
    required this.onApply,
  });

  final List<CalculatorCompletion> completions;
  final int selectedIndex;
  final ValueChanged<CalculatorCompletion> onApply;

  @override
  Widget build(BuildContext context) => Column(
    mainAxisSize: .min,
    crossAxisAlignment: .stretch,
    children: [
      const LauncherSectionHeader(
        'COMPLETIONS',
        trailing: LauncherKeyHint(['Tab'], label: 'Accept'),
      ),
      for (final (index, completion) in completions.indexed)
        LauncherResultRow(
          title: TextSpan(text: completion.label),
          subtitle: [completion.kind, ?completion.subtitle].join(' · '),
          leading: LauncherSymbolTile(
            completion.kind == 'correction' ? .swap : .straighten,
            selected: index == selectedIndex,
          ),
          trailing: index == selectedIndex
              ? const LauncherKeyHint(['Tab'])
              : null,
          selected: index == selectedIndex,
          onPressed: () => onApply(completion),
        ),
    ],
  );
}

class _CatalogList extends StatelessWidget {
  const _CatalogList({
    required this.entries,
    required this.selectedIndex,
    required this.onInsert,
  });

  final List<CalculatorCatalogEntry> entries;
  final int selectedIndex;
  final ValueChanged<CalculatorCatalogEntry> onInsert;

  @override
  Widget build(BuildContext context) => Column(
    mainAxisSize: .min,
    crossAxisAlignment: .stretch,
    children: [
      const LauncherSectionHeader(
        'CATALOG',
        trailing: LauncherKeyHint(['Tab'], label: 'Insert'),
      ),
      for (final (index, entry) in entries.indexed)
        LauncherResultRow(
          title: TextSpan(
            text: entry.symbol.isNotEmpty && entry.symbol != entry.name
                ? '${entry.name} · ${entry.symbol}'
                : entry.name,
          ),
          subtitle: [
            entry.kind,
            if (entry.description.isNotEmpty) entry.description,
            if (entry.example case final ex?) 'e.g. $ex',
          ].join(' · '),
          leading: LauncherSymbolTile(switch (entry.kind) {
            'constant' => .pi,
            'function' => .functions,
            _ => .straighten,
          }, selected: index == selectedIndex),
          trailing: index == selectedIndex
              ? const LauncherKeyHint(['Tab'])
              : null,
          selected: index == selectedIndex,
          onPressed: () => onInsert(entry),
        ),
    ],
  );
}

class _HistoryView extends StatelessWidget {
  const _HistoryView({
    required this.controller,
    required this.compact,
    required this.onCopy,
    required this.onEdit,
    this.onClearHistory,
    this.relatedKind,
  });

  final CalculatorController controller;
  final bool compact;
  final ValueChanged<String> onCopy;
  final ValueChanged<String> onEdit;
  final VoidCallback? onClearHistory;
  final String? relatedKind;

  @override
  Widget build(BuildContext context) {
    final entries = relatedKind == null
        ? controller.historyEntries
        : controller.historyEntries
              .where((entry) => entry.candidate?.kind == relatedKind)
              .take(3)
              .toList();
    final variables = relatedKind == null
        ? controller.variableEntries
        : const <CalculatorVariableEntry>[];
    if (relatedKind != null && entries.isEmpty) return const SizedBox.shrink();
    if (controller.loadingHistory && entries.isEmpty && variables.isEmpty) {
      return const LinearProgressIndicator(
        semanticsLabel: 'Loading calculator history',
      );
    }
    if (entries.isEmpty && variables.isEmpty) {
      return const _EmptyHint(
        'Type an expression, unit conversion, programmer value, float format, '
        'time zone, or currency. For chemistry, try 2 mmol to mg using NaCl.',
      );
    }
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        if (variables.isNotEmpty) ...[
          const LauncherSectionHeader('VARIABLES'),
          for (final variable in variables)
            LauncherResultRow(
              title: TextSpan(
                text: '${variable.name} = ${variable.expression}',
              ),
              subtitle: 'Variable',
              leading: const LauncherSymbolTile(.dataObject),
              trailing: IconButton(
                icon: const LauncherIcon(.trash),
                tooltip: 'Delete ${variable.name}',
                onPressed: () => controller.deleteVariable(variable.name),
              ),
              onPressed: () => onEdit(variable.name),
            ),
        ],
        if (entries.isNotEmpty) ...[
          LauncherSectionHeader(
            'HISTORY',
            trailing: Row(
              mainAxisSize: .min,
              spacing: 8,
              children: [
                const LauncherKeyHint(['↑', '↓'], label: 'Select'),
                if (onClearHistory != null)
                  TextButton(
                    onPressed: onClearHistory,
                    child: const Text('Clear'),
                  ),
              ],
            ),
          ),
          for (final entry in entries)
            _HistoryRow(
              entry: entry,
              selected: entry.historyId == controller.selectedHistoryId,
              compact: compact,
              onSelect: () => controller.selectHistory(entry.historyId),
              onCopy: () => onCopy(entry.copyText),
              onEdit: () => onEdit(entry.query),
              onTogglePin: () => controller.setHistoryPinned(
                entry.historyId,
                pinned: !entry.isPinned,
              ),
            ),
        ],
      ],
    );
  }
}

class _HistoryRow extends StatelessWidget {
  const _HistoryRow({
    required this.entry,
    required this.selected,
    required this.compact,
    required this.onSelect,
    required this.onCopy,
    required this.onEdit,
    required this.onTogglePin,
  });

  final CalculatorHistoryEntry entry;
  final bool selected;
  final bool compact;
  final VoidCallback onSelect;
  final VoidCallback onCopy;
  final VoidCallback onEdit;
  final VoidCallback onTogglePin;

  @override
  Widget build(BuildContext context) => LauncherResultRow(
    title: TextSpan(text: entry.retainedResult),
    subtitle: entry.isPinned ? 'Pinned · ${entry.query}' : entry.query,
    leading: LauncherSymbolTile(
      entry.isPinned ? .star : .history,
      selected: selected,
    ),
    selected: selected,
    monospaced: entry.candidate?.kind == 'programmer',
    trailing: selected && !compact
        ? Row(
            mainAxisSize: .min,
            spacing: 4,
            children: [
              TextButton(
                onPressed: onTogglePin,
                child: Text(entry.isPinned ? 'Unpin' : 'Pin'),
              ),
              TextButton(onPressed: onEdit, child: const Text('Edit  Tab')),
              TextButton(onPressed: onCopy, child: const Text('Copy  ↵')),
            ],
          )
        : null,
    onPressed: onSelect,
  );
}

class _NoticeBanner extends StatelessWidget {
  const _NoticeBanner({
    required this.warnings,
    required this.correctionEdit,
    this.onApplyCorrection,
  });

  final List<CalculatorDiagnostic> warnings;
  final CalculatorTextEdit? correctionEdit;
  final VoidCallback? onApplyCorrection;

  @override
  Widget build(BuildContext context) {
    final parts = [
      if (warnings.isNotEmpty)
        warnings.map((w) => w.displayMessage).join(' · ')
      else if (correctionEdit != null)
        'Suggested fix: ${correctionEdit!.text}',
    ];
    final message = parts.join(' · ');
    if (message.isEmpty) return const SizedBox.shrink();
    return Row(
      spacing: 8,
      children: [
        const LauncherIcon(.bolt, size: 14),
        Expanded(child: Text(message, style: const TextStyle(fontSize: 12))),
        if (onApplyCorrection != null)
          InkWell(
            onTap: onApplyCorrection,
            borderRadius: .circular(8),
            child: const Padding(
              padding: .symmetric(horizontal: 6, vertical: 2),
              child: LauncherKeyHint(['Tab'], label: 'Apply fix'),
            ),
          ),
      ],
    );
  }
}

class _ErrorPanel extends StatelessWidget {
  const _ErrorPanel({
    required this.diagnostics,
    required this.selectedIndex,
    required this.onSuggestionSelected,
    this.fxStatusMessage,
    this.onRefreshRates,
  });

  final List<CalculatorDiagnostic> diagnostics;
  final int selectedIndex;
  final ValueChanged<CalculatorSuggestion> onSuggestionSelected;
  final String? fxStatusMessage;
  final VoidCallback? onRefreshRates;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final primary = diagnostics.firstOrNull;
    final suggestions = [for (final diag in diagnostics) ...diag.suggestions];
    return Column(
      crossAxisAlignment: .stretch,
      spacing: 8,
      children: [
        Container(
          padding: const .symmetric(horizontal: 24, vertical: 18),
          decoration: BoxDecoration(
            color: colors.surfaceContainerHighest,
            borderRadius: .circular(28),
          ),
          child: Column(
            crossAxisAlignment: .start,
            spacing: 8,
            children: [
              Row(
                spacing: 8,
                children: [
                  LauncherIcon(.warning, size: 18, color: colors.error),
                  Expanded(
                    child: Text(
                      primary?.displayMessage ??
                          'Unable to evaluate expression',
                      style: TextStyle(color: colors.error, fontWeight: .w600),
                    ),
                  ),
                  if (onRefreshRates != null)
                    TextButton(
                      onPressed: onRefreshRates,
                      child: const Text('Refresh rates'),
                    ),
                ],
              ),
              if (primary?.code == 'dimension_mismatch')
                Text(
                  'Choose a unit with the same dimension, or change the operation.',
                  style: theme.textTheme.bodySmall,
                ),
              if (fxStatusMessage case final status?) Text(status),
            ],
          ),
        ),
        if (suggestions.isNotEmpty) ...[
          const LauncherSectionHeader('TRY INSTEAD'),
          for (final (index, suggestion) in suggestions.indexed)
            LauncherResultRow(
              title: TextSpan(text: suggestion.label),
              subtitle: primary?.code == 'dimension_mismatch'
                  ? (suggestion.label.contains('×')
                        ? 'Weight under standard gravity'
                        : 'Same dimension')
                  : '',
              leading: LauncherSymbolTile(
                suggestion.label.contains('×') ? .functions : .straighten,
              ),
              selected: index == selectedIndex,
              trailing: index == selectedIndex
                  ? const LauncherKeyHint(['↵'], label: 'Use')
                  : null,
              onPressed: () => onSuggestionSelected(suggestion),
            ),
        ],
      ],
    );
  }
}

class _UnavailableBanner extends StatelessWidget {
  const _UnavailableBanner({required this.message, required this.onRetry});

  final String message;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) => Wrap(
    crossAxisAlignment: .center,
    children: [
      Padding(
        padding: const .symmetric(horizontal: 12, vertical: 12),
        child: Text(message, style: Theme.of(context).textTheme.bodySmall),
      ),
      TextButton(onPressed: onRetry, child: const Text('Retry')),
    ],
  );
}

class _EmptyHint extends StatelessWidget {
  const _EmptyHint(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const .symmetric(horizontal: 12, vertical: 12),
    child: Text(text, style: Theme.of(context).textTheme.bodySmall),
  );
}

LauncherIconName _iconForKind(String kind) => switch (kind) {
  'programmer' => .memory,
  'time_conversion' => .schedule,
  'currency' => .money,
  'unit_conversion' => .straighten,
  _ => .calculate,
};
