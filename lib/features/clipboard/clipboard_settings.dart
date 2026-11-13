import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';

import 'clipboard_client.dart';
import 'clipboard_policy.dart';

/// The Launcher settings window; Clipboard is its only page.
class ClipboardSettingsWindow extends StatefulWidget {
  const ClipboardSettingsWindow({
    super.key,
    this.client,
    this.clipboard = const ClipboardClient(),
  });

  /// Null connects to the session bus and closes it with the window.
  final SettingsClient? client;
  final ClipboardClient clipboard;

  @override
  State<ClipboardSettingsWindow> createState() =>
      _ClipboardSettingsWindowState();
}

class _ClipboardSettingsWindowState extends State<ClipboardSettingsWindow> {
  late final SettingsClient _client = widget.client ?? SettingsClient();
  late final _controller = ClipboardPolicyController(
    client: _client,
    clipboard: widget.clipboard,
  )..start();

  @override
  void dispose() {
    _controller.dispose();
    if (widget.client == null) unawaited(_client.close());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    return Scaffold(
      body: SafeArea(
        child: Center(
          child: ConstrainedBox(
            constraints: const .new(maxWidth: 840),
            child: ListView(
              padding: const .fromLTRB(24, 32, 24, 32),
              children: [
                Text('Clipboard', style: text.headlineMedium),
                const SizedBox(height: 4),
                Text(
                  'Remya Launcher clipboard history and retention',
                  style: text.bodyMedium?.copyWith(
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
                const SizedBox(height: 24),
                ListenableBuilder(
                  listenable: _controller,
                  builder: (context, _) =>
                      _ClipboardPage(controller: _controller),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _ClipboardPage extends StatelessWidget {
  const _ClipboardPage({required this.controller});

  final ClipboardPolicyController controller;

  bool get _canChange =>
      !controller.busy &&
      controller.policyError == null &&
      controller.policy != null;

  Future<void> _changeLimit(
    BuildContext context,
    _Limit limit,
    int current,
  ) async {
    final value = await showDialog<int>(
      context: context,
      builder: (_) => _LimitDialog(limit: limit, current: current),
    );
    if (value == null || !_canChange || value == current) return;
    await (limit == .maxItems
        ? controller.setMaxItems(value)
        : controller.setRetentionDays(value));
  }

  Future<void> _clearHistory(BuildContext context) async {
    final colors = Theme.of(context).colorScheme;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        icon: const Icon(Icons.delete_sweep_rounded),
        title: const Text('Clear clipboard history?'),
        content: const Text(
          'This removes every entry from Remya Launcher’s history, including '
          'pinned entries.',
        ),
        actions: [
          TextButton(
            autofocus: true,
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(
              backgroundColor: colors.error,
              foregroundColor: colors.onError,
            ),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Clear history'),
          ),
        ],
      ),
    );
    if (confirmed != true || !context.mounted) return;
    if (await controller.clearHistory() && context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Clipboard history cleared')),
      );
    }
  }

  Widget _limit(BuildContext context, _Limit limit, int? value) =>
      OutlinedButton(
        onPressed: value == null || !_canChange
            ? null
            : () => _changeLimit(context, limit, value),
        child: Text(value == null ? 'Unavailable' : '$value ${limit.unit}'),
      );

  @override
  Widget build(BuildContext context) {
    final policy = controller.policy;
    final colors = Theme.of(context).colorScheme;
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        if (controller.policyError case final error?)
          _ErrorCard(
            icon: Icons.sync_problem_rounded,
            title: 'Remya Core cannot provide the clipboard policy',
            message: policy == null
                ? error
                : '$error\nShowing the last values read.',
            retryable: true,
            onRetry: controller.busy ? null : controller.refresh,
          ),
        if (controller.historyError case final error?)
          _ErrorCard(
            icon: Icons.content_paste_off_rounded,
            title: 'Clipboard history service error',
            message: error,
          ),
        if (controller.busy && policy == null) const LinearProgressIndicator(),
        Padding(
          padding: const .only(top: 16, bottom: 12),
          child: Text(
            'History',
            style: Theme.of(context).textTheme.titleMedium,
          ),
        ),
        Material(
          color: colors.surfaceContainerLow,
          borderRadius: .circular(24),
          clipBehavior: .antiAlias,
          child: Column(
            children: [
              _SettingRow(
                title: 'Clipboard history',
                description:
                    'Record new items in Remya Launcher. Turning this off '
                    'keeps the existing history.',
                control: policy == null
                    ? const Text('Unavailable')
                    : Switch(
                        value: policy.enabled,
                        onChanged: _canChange ? controller.setEnabled : null,
                      ),
              ),
              // Limits only apply while history records new items.
              if (policy?.enabled ?? true) ...[
                const Divider(height: 1, indent: 20, endIndent: 20),
                _SettingRow(
                  title: _Limit.maxItems.title,
                  description:
                      'A lower limit immediately removes the oldest unpinned '
                      'items above the limit.',
                  control: _limit(context, .maxItems, policy?.maxItems),
                ),
                const Divider(height: 1, indent: 20, endIndent: 20),
                _SettingRow(
                  title: _Limit.retentionDays.title,
                  description:
                      'A shorter period immediately removes unpinned items '
                      'older than the new limit.',
                  control: _limit(
                    context,
                    .retentionDays,
                    policy?.retentionDays,
                  ),
                ),
              ],
              const Divider(height: 1, indent: 20, endIndent: 20),
              _SettingRow(
                title: 'Clear clipboard history',
                description:
                    'Remove every saved item, including pinned items, from '
                    'Remya Launcher.',
                control: FilledButton.tonal(
                  style: FilledButton.styleFrom(
                    backgroundColor: colors.errorContainer,
                    foregroundColor: colors.onErrorContainer,
                  ),
                  onPressed: controller.busy
                      ? null
                      : () => _clearHistory(context),
                  child: const Text('Clear history'),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

enum _Limit {
  maxItems('Maximum history items', 'items', 5000),
  retentionDays('Retention period', 'days', 365);

  const _Limit(this.title, this.unit, this.max);

  final String title;
  final String unit;
  final int max;
}

/// Information on the left and the control on the right; stacked when narrow.
class _SettingRow extends StatelessWidget {
  const _SettingRow({
    required this.title,
    required this.description,
    required this.control,
  });

  final String title;
  final String description;
  final Widget control;

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    final label = Column(
      crossAxisAlignment: .start,
      spacing: 6,
      children: [
        Text(title, style: text.bodyLarge),
        Text(description, style: text.bodySmall),
      ],
    );
    return Padding(
      padding: const .symmetric(horizontal: 20, vertical: 18),
      child: MergeSemantics(
        child: LayoutBuilder(
          builder: (context, constraints) => constraints.maxWidth < 480
              ? Column(
                  crossAxisAlignment: .start,
                  spacing: 12,
                  children: [label, control],
                )
              : Row(
                  spacing: 24,
                  children: [
                    Expanded(child: label),
                    control,
                  ],
                ),
        ),
      ),
    );
  }
}

class _ErrorCard extends StatelessWidget {
  const _ErrorCard({
    required this.icon,
    required this.title,
    required this.message,
    this.retryable = false,
    this.onRetry,
  });

  final IconData icon;
  final String title;
  final String message;
  final bool retryable;

  /// Null keeps Retry visible but disabled while another operation runs.
  final VoidCallback? onRetry;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    final text = Theme.of(context).textTheme;
    return Padding(
      padding: const .only(bottom: 12),
      child: Material(
        color: colors.errorContainer,
        borderRadius: .circular(24),
        child: Padding(
          padding: const .fromLTRB(20, 16, 12, 16),
          child: Row(
            spacing: 16,
            children: [
              Icon(icon, color: colors.onErrorContainer),
              Expanded(
                child: Semantics(
                  liveRegion: true,
                  child: Column(
                    crossAxisAlignment: .start,
                    spacing: 4,
                    children: [
                      Text(
                        title,
                        style: text.titleSmall?.copyWith(
                          color: colors.onErrorContainer,
                        ),
                      ),
                      Text(
                        message,
                        style: text.bodyMedium?.copyWith(
                          color: colors.onErrorContainer,
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              if (retryable)
                TextButton.icon(
                  style: TextButton.styleFrom(
                    foregroundColor: colors.onErrorContainer,
                  ),
                  onPressed: onRetry,
                  icon: const Icon(Icons.refresh_rounded),
                  label: const Text('Retry'),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _LimitDialog extends StatefulWidget {
  const _LimitDialog({required this.limit, required this.current});

  final _Limit limit;
  final int current;

  @override
  State<_LimitDialog> createState() => _LimitDialogState();
}

class _LimitDialogState extends State<_LimitDialog> {
  final _form = GlobalKey<FormState>();
  late int? _value = widget.current;

  void _submit() {
    if (_form.currentState!.validate()) Navigator.pop(context, _value);
  }

  @override
  Widget build(BuildContext context) {
    final limit = widget.limit;
    return AlertDialog(
      title: Text(limit.title),
      content: Form(
        key: _form,
        child: TextFormField(
          autofocus: true,
          initialValue: '${widget.current}',
          keyboardType: .number,
          decoration: InputDecoration(
            labelText: 'Enter 1 to ${limit.max} ${limit.unit}',
          ),
          onChanged: (text) => _value = int.tryParse(text),
          onFieldSubmitted: (_) => _submit(),
          validator: (text) {
            final value = int.tryParse(text ?? '');
            return value == null || value < 1 || value > limit.max
                ? 'Enter a whole number from 1 to ${limit.max}.'
                : null;
          },
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Set limit')),
      ],
    );
  }
}
