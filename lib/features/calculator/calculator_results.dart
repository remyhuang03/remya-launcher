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
            (correctionEdit != null ||
                controller.warningDiagnostics.isNotEmpty))
          _NoticeBanner(
            warnings: controller.warningDiagnostics,
            correctionEdit: correctionEdit,
            fxStatusMessage: controller.fxStatusMessage,
            onRefreshRates: refreshRates,
            onApplyCorrection: correctionEdit == null
                ? null
                : () => onApplyEdit(correctionEdit),
          ),
        if (!showError &&
            candidate != null &&
            candidate.kind != 'catalog_search') ...[
          _HeroSection(
            candidate: candidate,
            selectedFormatId: controller.selectedFormatId,
            onFormatSelected: controller.selectFormat,
            onCopy: controller.isCopyableResult ? onCopyResult : null,
          ),
          _CandidateDetails(candidate: candidate, controller: controller),
        ],
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
    final subtitleParts = [
      if (candidate.normalized.isNotEmpty) candidate.normalized,
      if (candidate.interpretation.isNotEmpty &&
          candidate.interpretation != candidate.normalized)
        candidate.interpretation,
    ];
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        const LauncherSectionHeader(
          'CALCULATOR',
          trailing: LauncherKeyHint(['Tab'], label: 'Open in Calc'),
        ),
        LauncherResultRow(
          title: TextSpan(text: value),
          subtitle: subtitleParts.isEmpty
              ? candidate.kind
              : subtitleParts.join(' · '),
          leading: const LauncherSymbolTile(.calculate, selected: true),
          monospaced: candidate.kind == 'programmer',
          selected: true,
          trailing: Row(
            mainAxisSize: .min,
            spacing: 8,
            children: [
              TextButton(onPressed: onCopy, child: const Text('Copy  ↵')),
            ],
          ),
          onPressed: onOpenInCalc,
        ),
      ],
    );
  }
}

class _HeroSection extends StatelessWidget {
  const _HeroSection({
    required this.candidate,
    required this.selectedFormatId,
    required this.onFormatSelected,
    this.onCopy,
  });

  final CalculatorCandidate candidate;
  final String? selectedFormatId;
  final ValueChanged<String> onFormatSelected;
  final VoidCallback? onCopy;

  static final _numericPrefix = RegExp(
    r'^[+-]?[\d,]+(?:\.\d+)?(?:[eE][+-]?\d+)?$',
  );

  @override
  Widget build(BuildContext context) {
    final display = candidate.primaryDisplay(selectedFormatId);
    final (value, unit) = _splitDisplay(display);
    final expression = candidate.normalized.isNotEmpty
        ? candidate.normalized
        : candidate.interpretation;
    return LauncherAnswerCard(
      expression: expression,
      value: value,
      unit: unit,
      alternatives: [for (final format in candidate.formats) format.label],
      selectedAlternative: _selectedFormatIndex(),
      onAlternativeSelected: candidate.formats.isEmpty
          ? null
          : (index) => onFormatSelected(candidate.formats[index].id),
      onCopy: onCopy,
    );
  }

  int _selectedFormatIndex() {
    if (candidate.formats.isEmpty) return 0;
    final idx = candidate.formats.indexWhere((f) => f.id == selectedFormatId);
    return idx < 0 ? 0 : idx;
  }

  (String, String) _splitDisplay(String display) {
    final parts = display.trim().split(' ');
    if (parts.length == 2 && _numericPrefix.hasMatch(parts.first)) {
      return (parts[0], parts[1]);
    }
    return (display, '');
  }
}

class _CandidateDetails extends StatelessWidget {
  const _CandidateDetails({required this.candidate, required this.controller});

  final CalculatorCandidate candidate;
  final CalculatorController controller;

  @override
  Widget build(BuildContext context) {
    final value = candidate.value;
    final showProgrammer = candidate.kind == 'programmer';
    final showFloat = value?.tag == 'float_bits';
    final showTime = value?.tag == 'time';
    final showCurrency =
        value?.tag == 'currency' || candidate.kind == 'currency';
    final showInterpretation =
        candidate.interpretation.isNotEmpty &&
        candidate.interpretation != candidate.normalized;
    final hasAny =
        showProgrammer ||
        showFloat ||
        showTime ||
        showCurrency ||
        showInterpretation ||
        candidate.assumptions.isNotEmpty;
    if (!hasAny) return const SizedBox.shrink();

    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    return Container(
      padding: const .all(12),
      decoration: BoxDecoration(
        color: colors.surfaceContainerHighest,
        borderRadius: .circular(16),
      ),
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        spacing: 8,
        children: [
          if (showInterpretation)
            Text(candidate.interpretation, style: theme.textTheme.bodySmall),
          if (showProgrammer) _ProgrammerBar(controller: controller),
          if (showFloat && value != null) _FloatBitsStrip(value: value),
          if (showTime && value != null) _TimeDetails(value: value),
          if (showCurrency)
            _CurrencyProvenance(
              candidate: candidate,
              statusMessage: controller.fxStatusMessage,
              onRefresh: controller.refreshRates,
            ),
          if (candidate.assumptions.isNotEmpty)
            Wrap(
              spacing: 6,
              runSpacing: 4,
              children: [
                for (final assumption in candidate.assumptions)
                  Container(
                    padding: const .symmetric(horizontal: 8, vertical: 3),
                    decoration: BoxDecoration(
                      color: colors.surfaceContainer,
                      borderRadius: .circular(8),
                    ),
                    child: Text(assumption, style: theme.textTheme.bodySmall),
                  ),
              ],
            ),
        ],
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
        _SettingPill(
          label: '${settings.bitWidth}-bit',
          onPressed: controller.cycleBitWidth,
        ),
        _SettingPill(
          label: settings.signed ? 'Signed' : 'Unsigned',
          onPressed: controller.toggleSigned,
        ),
        _SettingPill(
          label: 'Overflow: ${settings.overflow}',
          onPressed: controller.toggleOverflow,
        ),
        const LauncherKeyHint(['Ctrl', 'B'], label: 'Bit width'),
        const LauncherKeyHint(['Alt', '1–4'], label: 'Copy format'),
      ],
    );
  }
}

class _SettingPill extends StatelessWidget {
  const _SettingPill({required this.label, required this.onPressed});

  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final shape = RoundedRectangleBorder(
      borderRadius: .circular(10),
      side: BorderSide(color: colors.outlineVariant),
    );
    return Material(
      color: colors.surfaceContainer,
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
              color: colors.onSurface,
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
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final format = value.str('format') ?? 'Float';
    final bitsHex = value.str('bitsHex', 'bits_hex');
    final signBit = value.str('signBit', 'sign_bit');
    final expBits = value.str('exponentBits', 'exponent_bits');
    final fracBits = value.str('fractionBits', 'fraction_bits');
    final absError = value.str('absoluteError', 'error');
    final relError = value.str(
      'relativeErrorPercent',
      'relative_error_percent',
    );
    final classification = value.str('classification');
    final rounding = value.str('roundingMode', 'rounding_mode');

    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      spacing: 6,
      children: [
        Wrap(
          spacing: 12,
          runSpacing: 4,
          children: [
            Text(
              format.toUpperCase(),
              style: theme.textTheme.labelSmall?.copyWith(
                color: colors.onSurface,
              ),
            ),
            if (bitsHex != null)
              Text(bitsHex, style: theme.textTheme.bodySmall),
            if (classification != null)
              Text(classification, style: theme.textTheme.bodySmall),
            if (rounding != null)
              Text(rounding, style: theme.textTheme.bodySmall),
          ],
        ),
        if (signBit != null || expBits != null || fracBits != null)
          Wrap(
            spacing: 6,
            runSpacing: 4,
            children: [
              if (signBit != null)
                _BitSegment(
                  label: 'S',
                  bits: signBit,
                  background: colors.tertiaryContainer,
                  foreground: colors.onTertiaryContainer,
                ),
              if (expBits != null)
                _BitSegment(
                  label: 'E',
                  bits: expBits,
                  background: colors.primaryContainer,
                  foreground: colors.onPrimaryContainer,
                ),
              if (fracBits != null)
                _BitSegment(
                  label: 'F',
                  bits: fracBits,
                  background: colors.secondaryContainer,
                  foreground: colors.onSecondaryContainer,
                ),
            ],
          ),
        if (absError != null || relError != null)
          Text(
            [
              if (absError != null) 'Error: $absError',
              if (relError != null) '($relError%)',
            ].join(' '),
            style: theme.textTheme.bodySmall,
          ),
      ],
    );
  }
}

class _BitSegment extends StatelessWidget {
  const _BitSegment({
    required this.label,
    required this.bits,
    required this.background,
    required this.foreground,
  });

  final String label;
  final String bits;
  final Color background;
  final Color foreground;

  @override
  Widget build(BuildContext context) => Container(
    padding: const .symmetric(horizontal: 8, vertical: 4),
    decoration: BoxDecoration(color: background, borderRadius: .circular(8)),
    child: Text(
      '$label $bits',
      style: TextStyle(
        fontSize: 12,
        fontFamily: 'JetBrains Mono',
        fontFamilyFallback: const ['monospace'],
        color: foreground,
      ),
    ),
  );
}

class _TimeDetails extends StatelessWidget {
  const _TimeDetails({required this.value});

  final CalculatorValue value;

  @override
  Widget build(BuildContext context) {
    final sourceZone = value.str('sourceZoneId', 'sourceZone');
    final targetZone = value.str('targetZoneId', 'targetZone');
    final sourceCivil = value.str('sourceCivil');
    final targetCivil = value.str('targetCivil', 'localDateTime');
    final dst = value.str('dstResolution', 'dst_resolution');
    final dateNote = value.str('dateAssumption', 'date_assumption');

    final lines = [
      if (sourceCivil != null && sourceZone != null)
        'Source: $sourceCivil ($sourceZone)',
      if (targetCivil != null && targetZone != null)
        'Target: $targetCivil ($targetZone)',
      if (dst != null) 'DST: $dst',
      ?dateNote,
    ];
    if (lines.isEmpty) return const SizedBox.shrink();
    return Text(
      lines.join(' · '),
      style: Theme.of(context).textTheme.bodySmall,
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
        (prov['source'] ?? prov['sourceId'] ?? prov['provider'])?.toString() ??
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
      const LauncherSectionHeader('INTERPRETATIONS'),
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
          subtitle: [
            completion.kind,
            ?completion.scoreTier,
            ?completion.subtitle,
          ].join(' · '),
          leading: LauncherSymbolTile(
            completion.kind == 'correction' ? .swap : .calculate,
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
          leading: LauncherSymbolTile(
            .calculate,
            selected: index == selectedIndex,
          ),
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
  });

  final CalculatorController controller;
  final bool compact;
  final ValueChanged<String> onCopy;
  final ValueChanged<String> onEdit;
  final VoidCallback? onClearHistory;

  @override
  Widget build(BuildContext context) {
    final entries = controller.historyEntries;
    final variables = controller.variableEntries;
    if (controller.loadingHistory && entries.isEmpty && variables.isEmpty) {
      return const LinearProgressIndicator(
        semanticsLabel: 'Loading calculator history',
      );
    }
    if (entries.isEmpty && variables.isEmpty) {
      return const _EmptyHint(
        'Type an expression, unit conversion, programmer value, float format, time zone, or currency.',
      );
    }
    return Column(
      mainAxisSize: .min,
      crossAxisAlignment: .stretch,
      children: [
        if (variables.isNotEmpty) ...[
          const LauncherSectionHeader('VARIABLES'),
          Padding(
            padding: const .only(bottom: 8),
            child: Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [
                for (final variable in variables)
                  InputChip(
                    label: Text('${variable.name} = ${variable.expression}'),
                    visualDensity: .compact,
                    onPressed: () => onEdit(variable.name),
                    onDeleted: () => controller.deleteVariable(variable.name),
                    deleteButtonTooltipMessage: 'Delete ${variable.name}',
                  ),
              ],
            ),
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
    this.fxStatusMessage,
    this.onRefreshRates,
    this.onApplyCorrection,
  });

  final List<CalculatorDiagnostic> warnings;
  final CalculatorTextEdit? correctionEdit;
  final String? fxStatusMessage;
  final VoidCallback? onRefreshRates;
  final VoidCallback? onApplyCorrection;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final parts = [
      if (warnings.isNotEmpty)
        warnings.map((w) => w.displayMessage).join(' · ')
      else if (correctionEdit != null)
        'Suggested fix: ${correctionEdit!.text}',
      ?fxStatusMessage,
    ];
    final message = parts.join(' · ');
    if (message.isEmpty) return const SizedBox.shrink();
    return Container(
      padding: const .symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: colors.surfaceContainerHigh,
        borderRadius: .circular(14),
      ),
      child: Row(
        spacing: 8,
        children: [
          Expanded(child: Text(message, style: theme.textTheme.bodySmall)),
          if (onRefreshRates != null)
            TextButton(onPressed: onRefreshRates, child: const Text('Refresh')),
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
      ),
    );
  }
}

class _ErrorPanel extends StatelessWidget {
  const _ErrorPanel({
    required this.diagnostics,
    required this.onSuggestionSelected,
    this.fxStatusMessage,
    this.onRefreshRates,
  });

  final List<CalculatorDiagnostic> diagnostics;
  final ValueChanged<CalculatorSuggestion> onSuggestionSelected;
  final String? fxStatusMessage;
  final VoidCallback? onRefreshRates;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    final primary = diagnostics.firstOrNull;
    final suggestions = [for (final diag in diagnostics) ...diag.suggestions];
    return Container(
      padding: const .all(14),
      decoration: BoxDecoration(
        color: colors.errorContainer,
        borderRadius: .circular(20),
      ),
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        spacing: 8,
        children: [
          Row(
            spacing: 8,
            children: [
              Icon(
                Icons.error_outline_rounded,
                size: 18,
                color: colors.onErrorContainer,
              ),
              Expanded(
                child: Text(
                  primary?.displayMessage ?? 'Unable to evaluate expression',
                  style: TextStyle(
                    color: colors.onErrorContainer,
                    fontWeight: .w600,
                    fontSize: 14,
                  ),
                ),
              ),
              if (onRefreshRates != null)
                TextButton(
                  onPressed: onRefreshRates,
                  child: const Text('Refresh rates'),
                ),
            ],
          ),
          if (fxStatusMessage case final status?)
            Text(
              status,
              style: theme.textTheme.bodySmall?.copyWith(
                color: colors.onErrorContainer,
              ),
            ),
          if (suggestions.isNotEmpty)
            Wrap(
              spacing: 8,
              runSpacing: 6,
              children: [
                for (final suggestion in suggestions)
                  _SuggestionChip(
                    label: suggestion.label,
                    onPressed: () => onSuggestionSelected(suggestion),
                  ),
              ],
            ),
        ],
      ),
    );
  }
}

class _SuggestionChip extends StatelessWidget {
  const _SuggestionChip({required this.label, required this.onPressed});

  final String label;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final shape = RoundedRectangleBorder(
      borderRadius: .circular(10),
      side: BorderSide(color: colors.onErrorContainer.withValues(alpha: .35)),
    );
    return Material(
      color: colors.surfaceContainer,
      shape: shape,
      child: InkWell(
        customBorder: shape,
        onTap: onPressed,
        child: Padding(
          padding: const .symmetric(horizontal: 10, vertical: 6),
          child: Text(
            label,
            style: TextStyle(
              fontSize: 12,
              fontWeight: .w500,
              color: colors.onSurface,
            ),
          ),
        ),
      ),
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
  'programmer' => .code,
  'time_conversion' => .history,
  'currency' => .swap,
  'unit_conversion' => .tune,
  _ => .calculate,
};
