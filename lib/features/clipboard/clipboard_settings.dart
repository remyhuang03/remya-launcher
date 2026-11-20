import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_ui/settings_ui.dart';

import '../apps/apps_client.dart';
import '../apps/apps_settings.dart';
import '../emoji/emoji_settings.dart';
import '../files/files_client.dart';
import '../files/files_settings.dart';
import '../launcher/startup_settings.dart';
import '../web/web_client.dart';
import '../web/web_settings.dart';
import 'clipboard_client.dart';
import 'clipboard_policy.dart';

const _repository = 'https://github.com/remyhuang03/remya-launcher';
const _appIcon = 'assets/icons/app_icon.svg';

/// The undecorated settings window's native close and move requests.
const _window = MethodChannel('top.remya.launcher/window');

/// The Launcher settings window.
class ClipboardSettingsWindow extends StatefulWidget {
  const ClipboardSettingsWindow({
    super.key,
    this.initialPage,
    this.client,
    this.clipboard = const ClipboardClient(),
    this.apps,
    this.web,
    this.files,
  });

  /// Initial settings page id; null or empty opens Clipboard.
  final String? initialPage;

  /// Null connects to the session bus and closes it with the window.
  final SettingsClient? client;
  final ClipboardClient clipboard;
  final AppsClient? apps;
  final WebClient? web;
  final FilesClient? files;

  @override
  State<ClipboardSettingsWindow> createState() =>
      _ClipboardSettingsWindowState();
}

class _ClipboardSettingsWindowState extends State<ClipboardSettingsWindow> {
  late final SettingsClient _client = widget.client ?? SettingsClient();
  late final AppsClient _apps = widget.apps ?? AppsClient();
  late final WebClient _web = widget.web ?? WebClient();
  FilesClient? _ownedFiles;
  FilesClient get _files => widget.files ?? (_ownedFiles ??= FilesClient());
  late final _controller = ClipboardPolicyController(
    client: _client,
    clipboard: widget.clipboard,
  )..start();
  final _version = remyaAppVersion();
  late String _selected = switch (widget.initialPage) {
    null || '' => 'clipboard',
    final page => page,
  };

  @override
  void dispose() {
    _controller.dispose();
    if (widget.client == null) unawaited(_client.close());
    if (widget.apps == null) unawaited(_apps.close());
    if (widget.web == null) unawaited(_web.close());
    _ownedFiles?.close();
    super.dispose();
  }

  void _select(String id) => setState(() => _selected = id);

  @override
  Widget build(BuildContext context) => Column(
    children: [
      RemyaTitleBar(
        icon: SvgPicture.asset(_appIcon),
        title: 'Remy\'a Launcher Settings',
        dragArea: (child) => GestureDetector(
          behavior: .opaque,
          onPanStart: (_) => unawaited(_window.invokeMethod<void>('beginMove')),
          child: child,
        ),
        onClose: () => unawaited(_window.invokeMethod<void>('close')),
      ),
      Expanded(
        child: ListenableBuilder(
          listenable: _controller,
          builder: (context, _) => RemyaSettingsScaffold(
            selected: _selected,
            onSelected: _select,
            loading:
                _selected == 'clipboard' &&
                _controller.busy &&
                _controller.policy == null,
            onBack: _selected == 'licenses' ? () => _select('about') : null,
            pages: [
              startupSettingsPage(_client),
              RemyaSettingsPage(
                id: 'clipboard',
                title: 'Clipboard',
                icon: Icons.content_paste_rounded,
                category: .system,
                builder: (_) => _ClipboardPage(controller: _controller),
              ),
              appsSettingsPage(_apps),
              webSettingsPage(_web),
              filesSettingsPage(_client, files: _files),
              emojiSettingsPage(_client),
              remyaMaterialThemePage(client: _client),
            ],
            footerPages: [
              RemyaSettingsPage(
                id: 'about',
                title: 'About',
                icon: Icons.info_outline_rounded,
                tabs: false,
                children: [
                  RemyaSettingsPage(
                    id: 'about',
                    title: 'About',
                    icon: Icons.info_outline_rounded,
                    builder: (_) => _AboutPage(
                      version: _version,
                      onLicenses: () => _select('licenses'),
                    ),
                  ),
                  RemyaSettingsPage(
                    id: 'licenses',
                    title: 'Licenses',
                    icon: Icons.description_outlined,
                    fill: true,
                    builder: (_) => const RemyaLicensesView(),
                  ),
                ],
              ),
            ],
          ),
        ),
      ),
    ],
  );
}

class _AboutPage extends StatelessWidget {
  const _AboutPage({required this.version, required this.onLicenses});

  final Future<String?> version;
  final VoidCallback onLicenses;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: .stretch,
    children: [
      FutureBuilder(
        future: version,
        builder: (context, snapshot) => RemyaAboutHeader(
          icon: SvgPicture.asset(
            _appIcon,
            semanticsLabel: 'Remy\'a Launcher application icon',
          ),
          name: 'Remy\'a Launcher',
          version: snapshot.data,
        ),
      ),
      RemyaAboutLinks(repository: _repository, onLicenses: onLicenses),
    ],
  );
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
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        icon: const Icon(Icons.delete_sweep_rounded),
        title: const Text('Clear clipboard history?'),
        content: const Text(
          'This removes every entry from Remy\'a Launcher’s history, including '
          'pinned entries.',
        ),
        actions: [
          TextButton(
            autofocus: true,
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: destructiveFilledButtonStyle(context),
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
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        if (controller.policyError case final error?)
          SettingsErrorCard(
            message: policy == null
                ? 'Remy\'a Core cannot provide the clipboard policy.\n$error'
                : 'Remy\'a Core cannot provide the clipboard policy.\n$error\n'
                      'Showing the last values read.',
            onRetry: controller.busy ? null : controller.refresh,
          ),
        if (controller.historyError case final error?)
          SettingsErrorCard(
            message: 'Clipboard history service error.\n$error',
            actionLabel: null,
          ),
        SettingsGroup(
          title: 'History',
          children: [
            SettingRow(
              id: clipboardHistoryKey,
              label: const SettingLabel(
                'Clipboard history',
                description:
                    'Record new items in Remy\'a Launcher. Turning this off '
                    'keeps the existing history.',
              ),
              control: policy == null
                  ? const Text('Unavailable')
                  : Switch(
                      value: policy.enabled,
                      onChanged: _canChange ? controller.setEnabled : null,
                    ),
            ),
            // Limits only apply while history records new items.
            if (policy?.enabled ?? true) ...[
              SettingRow(
                id: clipboardMaxItemsKey,
                label: SettingLabel(
                  _Limit.maxItems.title,
                  description:
                      'A lower limit immediately removes the oldest unpinned '
                      'items above the limit.',
                ),
                control: _limit(context, .maxItems, policy?.maxItems),
              ),
              SettingRow(
                id: clipboardRetentionDaysKey,
                label: SettingLabel(
                  _Limit.retentionDays.title,
                  description:
                      'A shorter period immediately removes unpinned items '
                      'older than the new limit.',
                ),
                control: _limit(context, .retentionDays, policy?.retentionDays),
              ),
            ],
            SettingRow(
              id: 'clear-history',
              label: const SettingLabel(
                'Clear clipboard history',
                description:
                    'Remove every saved item, including pinned items, from '
                    'Remy\'a Launcher.',
              ),
              control: FilledButton.tonal(
                style: destructiveTonalButtonStyle(context),
                onPressed: controller.busy
                    ? null
                    : () => _clearHistory(context),
                child: const Text('Clear history'),
              ),
            ),
          ],
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
