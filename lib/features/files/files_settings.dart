import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_ui/settings_ui.dart';

import '../clipboard/clipboard_policy.dart' show describeError;
import 'files_client.dart';
import 'files_preferences.dart';

part 'files_settings_fields.dart';

const filesSettingsPageId = 'files';

const _fileProviderLabels = {
  'memy': 'Memy',
  'xbel': 'System recent files (XBEL)',
};

const _directoryProviderLabels = {
  'zoxide': 'Zoxide',
  'memy': 'Memy',
  'xbel': 'System recent files (XBEL)',
};

const _externalWriteModeLabels = {
  'read_only': 'Read only',
  'launcher': 'Launcher writes',
  'delegate_editor': 'Delegate to editor',
};

RemyaSettingsPage filesSettingsPage(
  SettingsClient client, {
  FilesClient? files,
}) => RemyaSettingsPage(
  id: filesSettingsPageId,
  title: 'Files',
  icon: Icons.folder_rounded,
  category: .system,
  children: [
    RemyaSettingsPage(
      id: FilesPreferences.searchId,
      title: 'Search',
      icon: Icons.search_rounded,
      builder: (_) => _FilesSectionPage(
        key: const ValueKey(FilesPreferences.searchId),
        client: client,
        sectionId: FilesPreferences.searchId,
      ),
    ),
    RemyaSettingsPage(
      id: FilesPreferences.historyId,
      title: 'History',
      icon: Icons.history_rounded,
      builder: (_) => _FilesSectionPage(
        key: const ValueKey(FilesPreferences.historyId),
        client: client,
        sectionId: FilesPreferences.historyId,
        files: files,
      ),
    ),
    RemyaSettingsPage(
      id: FilesPreferences.previewId,
      title: 'Preview',
      icon: Icons.preview_rounded,
      builder: (_) => _FilesSectionPage(
        key: const ValueKey(FilesPreferences.previewId),
        client: client,
        sectionId: FilesPreferences.previewId,
      ),
    ),
    RemyaSettingsPage(
      id: FilesPreferences.actionsId,
      title: 'Actions',
      icon: Icons.open_in_new_rounded,
      builder: (_) => _FilesSectionPage(
        key: const ValueKey(FilesPreferences.actionsId),
        client: client,
        sectionId: FilesPreferences.actionsId,
      ),
    ),
  ],
);

class _FilesSectionPage extends StatefulWidget {
  const _FilesSectionPage({
    super.key,
    required this.client,
    required this.sectionId,
    this.files,
  });

  final SettingsClient client;
  final String sectionId;
  final FilesClient? files;

  @override
  State<_FilesSectionPage> createState() => _FilesSectionPageState();
}

typedef _ProviderStatus = ({
  String selected,
  String effective,
  String health,
  String reason,
});

typedef _HistoryStatus = ({_ProviderStatus file, _ProviderStatus directory});

class _FilesSectionPageState extends State<_FilesSectionPage> {
  late final FilesPreferences _preferences = FilesPreferences(
    client: widget.client,
  );
  FilesClient? _ownedFiles;
  FilesClient get _filesClient =>
      widget.files ?? (_ownedFiles ??= FilesClient());
  final _subscriptions = <StreamSubscription<Object?>>[];
  Map<String, dynamic>? _value;
  String? _revision;
  int _loadToken = 0;
  bool _loading = true;
  bool _busy = false;
  bool _reloadPending = false;
  String? _readError;
  String? _writeError;
  _HistoryStatus? _historyStatus;
  int _statusToken = 0;
  bool _statusBusy = false;
  String? _statusError;
  bool _clearBusy = false;
  String? _clearError;

  @override
  void initState() {
    super.initState();
    _subscriptions.add(
      widget.client.changes
          .where(
            (change) =>
                change.topic == '${FilesPreferences.appId}.${widget.sectionId}',
          )
          .listen((_) => _reload(), onError: _watchError),
    );
    _subscriptions.add(
      widget.client.serviceAvailableChanges.listen(
        (_) => _reload(),
        onError: _watchError,
      ),
    );
    unawaited(_reload());
  }

  Future<void> _reload() async {
    if (_busy) {
      _reloadPending = true;
      return;
    }
    final token = ++_loadToken;
    setState(() {
      _loading = true;
      _readError = null;
    });
    try {
      final snapshot = await _preferences.readSection(widget.sectionId);
      if (!mounted || token != _loadToken) return;
      setState(() {
        _value = snapshot.value;
        _revision = snapshot.revision;
        _loading = false;
        _readError = null;
        _writeError = null;
      });
      if (widget.sectionId == FilesPreferences.historyId) {
        unawaited(_loadHistoryStatus());
      }
    } on Object catch (error) {
      if (mounted && token == _loadToken) _watchError(error);
    }
  }

  void _watchError(Object error) => setState(() {
    _loadToken++;
    _loading = false;
    _readError = describeError(error);
  });

  Future<void> _save(String key, Object value) async {
    final current = _value;
    final revision = _revision;
    if (_busy ||
        _loading ||
        _readError != null ||
        current == null ||
        revision == null) {
      return;
    }
    final token = ++_loadToken;
    setState(() {
      _busy = true;
      _writeError = null;
    });
    try {
      final updated = await _preferences.replaceSectionValue(
        widget.sectionId,
        key,
        value,
        revision: revision,
        current: current,
      );
      if (!mounted || token != _loadToken) return;
      setState(() {
        _value = updated.value;
        _revision = updated.revision;
        _readError = null;
        _writeError = null;
      });
      if (widget.sectionId == FilesPreferences.historyId) {
        unawaited(_loadHistoryStatus());
      }
    } on Object catch (error) {
      if (mounted) {
        setState(() => _writeError = describeError(error));
      }
    } finally {
      if (mounted) {
        setState(() => _busy = false);
        if (_reloadPending) {
          _reloadPending = false;
          if (_writeError == null) unawaited(_reload());
        }
      }
    }
  }

  bool get _editable =>
      !_busy &&
      !_loading &&
      _readError == null &&
      _value != null &&
      _revision != null;

  Future<void> _editList(_ListField spec, List<String> current) async {
    final next = await showDialog<List<String>>(
      context: context,
      builder: (_) => _LineListDialog(spec: spec, current: current),
    );
    if (!mounted || next == null || _listEquals(next, current)) return;
    await _save(spec.key, next);
  }

  Future<void> _editNumber(_NumberField spec, int current) async {
    final next = await showDialog<int>(
      context: context,
      builder: (_) => _NumberDialog(spec: spec, current: current),
    );
    if (!mounted || next == null || next == current) return;
    await _save(spec.key, next);
  }

  Future<void> _editText(_TextField spec, String current) async {
    final next = await showDialog<String>(
      context: context,
      builder: (_) => _TextDialog(spec: spec, current: current),
    );
    if (!mounted || next == null || next == current) return;
    await _save(spec.key, next);
  }

  SettingRow _toggle(
    String key,
    String title,
    String description,
    bool? value,
  ) => SettingRow(
    id: key,
    label: SettingLabel(title, description: description),
    control: value == null
        ? const Text('Unavailable')
        : Semantics(
            label: title,
            child: Switch(
              value: value,
              onChanged: _editable ? (next) => _save(key, next) : null,
            ),
          ),
  );

  SettingRow _dropdown(
    String key,
    String title,
    String description,
    String? value,
    Map<String, String> choices,
  ) => SettingRow(
    id: key,
    label: SettingLabel(title, description: description),
    control: value == null
        ? const Text('Unavailable')
        : SettingsDropdown<String>(
            label: title,
            selected: choices.containsKey(value) ? value : null,
            choices: choices,
            onSelected: _editable
                ? (next) => unawaited(_save(key, next))
                : null,
          ),
  );

  SettingRow _listRow(_ListField spec, List<String>? value) => SettingRow(
    id: spec.key,
    label: SettingLabel(spec.title, description: spec.description),
    control: value == null
        ? const Text('Unavailable')
        : _ValueButton(
            onPressed: _editable ? () => _editList(spec, value) : null,
            label: spec.summary(value),
          ),
  );

  SettingRow _numberRow(_NumberField spec, int? value) => SettingRow(
    id: spec.key,
    label: SettingLabel(spec.title, description: spec.description),
    control: value == null
        ? const Text('Unavailable')
        : SettingsButton(
            onPressed: _editable ? () => _editNumber(spec, value) : null,
            label: spec.format(value),
          ),
  );

  SettingRow _textRow(_TextField spec, String? value) => SettingRow(
    id: spec.key,
    label: SettingLabel(spec.title, description: spec.description),
    control: value == null
        ? const Text('Unavailable')
        : _ValueButton(
            onPressed: _editable ? () => _editText(spec, value) : null,
            label: value.isEmpty ? 'Automatic' : value,
          ),
  );

  Future<void> _loadHistoryStatus({bool refresh = false}) async {
    final token = ++_statusToken;
    setState(() {
      _statusBusy = true;
      _statusError = null;
    });
    try {
      final preferences = await _preferences.read();
      final reply = await _filesClient.request(
        refresh ? 'history.refresh' : 'history.status',
        {'preferences': preferences},
      );
      final file = _parseProviderStatus(reply['file']);
      final directory = _parseProviderStatus(reply['directory']);
      if (file == null || directory == null) {
        throw const FormatException(
          'Files service returned an invalid history status.',
        );
      }
      if (!mounted || token != _statusToken) return;
      setState(() {
        _historyStatus = (file: file, directory: directory);
        _statusError = null;
      });
    } on Object catch (error) {
      if (!mounted || token != _statusToken) return;
      setState(() => _statusError = describeError(error));
    } finally {
      if (mounted && token == _statusToken) {
        setState(() => _statusBusy = false);
      }
    }
  }

  Future<void> _clearHistory() async {
    if (_clearBusy) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        icon: const Icon(Icons.delete_sweep_rounded),
        title: const Text('Clear Launcher history?'),
        content: const Text(
          'This removes recent files and folders recorded by Remy\'a Launcher. '
          'System recent files (XBEL) and external providers like Memy and '
          'Zoxide are not changed.',
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
    if (confirmed != true || !mounted) return;
    setState(() {
      _clearBusy = true;
      _clearError = null;
    });
    try {
      await _filesClient.request('history.clear', const {'scope': 'launcher'});
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Launcher files history cleared')),
      );
      unawaited(_loadHistoryStatus());
    } on Object catch (error) {
      if (mounted) {
        setState(() => _clearError = describeError(error));
      }
    } finally {
      if (mounted) {
        setState(() => _clearBusy = false);
      }
    }
  }

  @override
  void dispose() {
    for (final subscription in _subscriptions) {
      unawaited(subscription.cancel());
    }
    _ownedFiles?.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final value = _value;
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        if (_loading) const RemyaLoadingBar(),
        if (_readError case final error?)
          SettingsErrorCard(
            message: value == null
                ? 'Remy\'a Core cannot provide Files preferences.\n$error'
                : 'Remy\'a Core cannot provide Files preferences.\n$error\n'
                      'Showing the last values read.',
            onRetry: _busy ? null : _reload,
          ),
        if (_writeError case final error?)
          SettingsErrorCard(
            message: 'Could not save Files preferences.\n$error',
            actionLabel: 'Reload',
            onRetry: _busy ? null : _reload,
          ),
        ...switch (widget.sectionId) {
          FilesPreferences.searchId => _buildSearch(value),
          FilesPreferences.historyId => _buildHistory(value),
          FilesPreferences.previewId => _buildPreview(value),
          _ => _buildActions(value),
        },
      ],
    );
  }

  List<Widget> _buildSearch(Map<String, dynamic>? value) => [
    SettingsGroup(
      title: 'Locations',
      children: [
        _listRow(_ListField.roots, _stringList(value, 'roots')),
        _listRow(_ListField.exclusions, _stringList(value, 'exclusions')),
      ],
    ),
    SettingsGroup(
      title: 'Matching and limits',
      children: [
        _toggle(
          'respect_ignore',
          'Respect ignore files',
          'Skip paths matched by .gitignore and .ignore files.',
          value?['respect_ignore'] as bool?,
        ),
        _toggle(
          'show_hidden',
          'Show hidden files',
          'Include dotfiles and hidden directories in search results.',
          value?['show_hidden'] as bool?,
        ),
        _toggle(
          'contents_enabled',
          'Search file contents',
          'Search inside text files in addition to file and folder names.',
          value?['contents_enabled'] as bool?,
        ),
        _numberRow(
          _NumberField.contentsMaxBytes,
          value?['contents_max_bytes'] as int?,
        ),
        _numberRow(_NumberField.resultLimit, value?['result_limit'] as int?),
      ],
    ),
  ];

  List<Widget> _buildHistory(Map<String, dynamic>? value) => [
    const SettingsSurface(
      child: Text(
        'Files history is off by default for privacy. File history and '
        'directory history use independent providers. System XBEL recent '
        'files and external providers are opt-in and only change when you '
        'explicitly enable them.',
      ),
    ),
    SettingsGroup(
      title: 'Privacy and sources',
      children: [
        _toggle(
          'enabled',
          'Files history',
          'Off by default for privacy. Use recent files and folders when '
              'ranking and browsing in Launcher.',
          value?['enabled'] as bool?,
        ),
        _toggle(
          'record_local',
          'Record local launches',
          'Save files and folders opened from Launcher in local history.',
          value?['record_local'] as bool?,
        ),
        _toggle(
          'read_system_xbel',
          'Read system recent files (XBEL)',
          'Opt-in: read the desktop recently-used.xbel store populated by '
              'other apps.',
          value?['read_system_xbel'] as bool?,
        ),
        _toggle(
          'external_enabled',
          'Enable external providers',
          'Opt-in: allow Launcher to query the selected external file and '
              'directory providers.',
          value?['external_enabled'] as bool?,
        ),
      ],
    ),
    SettingsGroup(
      title: 'Providers',
      children: [
        _dropdown(
          'file_provider',
          'File history provider',
          'Independent provider used for recent files.',
          value?['file_provider'] as String?,
          _fileProviderLabels,
        ),
        _dropdown(
          'directory_provider',
          'Directory history provider',
          'Independent provider used for recent folders.',
          value?['directory_provider'] as String?,
          _directoryProviderLabels,
        ),
        _dropdown(
          'external_write_mode',
          'External provider write mode',
          'Controls whether external stores stay read-only, receive writes '
              'from Launcher, or delegate to the launched editor.',
          value?['external_write_mode'] as String?,
          _externalWriteModeLabels,
        ),
      ],
    ),
    if (_statusError case final error?)
      Padding(
        padding: const .only(top: 20),
        child: SettingsErrorCard(
          message: 'Could not check Files history status.\n$error',
          onRetry: _statusBusy || value == null
              ? null
              : () => _loadHistoryStatus(),
        ),
      ),
    SettingsGroup(
      title: 'Provider status',
      trailing: TextButton.icon(
        onPressed: _statusBusy || value == null
            ? null
            : () => _loadHistoryStatus(refresh: true),
        icon: const Icon(Icons.refresh_rounded),
        label: Text(_statusBusy ? 'Refreshing…' : 'Refresh'),
      ),
      children: [
        _statusRow(
          context,
          'file-history-status',
          'File history status',
          _historyStatus?.file,
        ),
        _statusRow(
          context,
          'directory-history-status',
          'Directory history status',
          _historyStatus?.directory,
        ),
      ],
    ),
    SettingsGroup(
      title: 'Retention',
      children: [
        _numberRow(
          _NumberField.retentionDays,
          value?['retention_days'] as int?,
        ),
        _numberRow(
          _NumberField.maxRecordsPerKind,
          value?['max_records_per_kind'] as int?,
        ),
      ],
    ),
    if (_clearError case final error?)
      Padding(
        padding: const .only(top: 20),
        child: SettingsErrorCard(
          message: 'Could not clear Launcher files history.\n$error',
          actionLabel: null,
        ),
      ),
    SettingsGroup(
      title: 'Local history',
      children: [
        SettingRow(
          id: 'clear-launcher-history',
          label: const SettingLabel(
            'Clear Launcher history',
            description:
                'Remove Launcher’s saved recent files and folders. System '
                'recent files and external providers are not changed.',
          ),
          control: SettingsButton(
            variant: .destructive,
            onPressed: _clearBusy ? null : _clearHistory,
            label: 'Clear history',
          ),
        ),
      ],
    ),
  ];

  SettingRow _statusRow(
    BuildContext context,
    String id,
    String title,
    _ProviderStatus? status,
  ) {
    final theme = Theme.of(context);
    final colors = theme.colorScheme;
    if (status == null) {
      return SettingRow(
        id: id,
        label: SettingLabel(title),
        control: Text(_statusBusy ? 'Checking…' : 'Unavailable'),
      );
    }
    final selected = _providerLabel(status.selected);
    final effective = _providerLabel(status.effective);
    final (badge, badgeColor, detail) = switch ((
      status.health,
      status.reason,
    )) {
      ('disabled', 'history_disabled') => (
        'History is off',
        colors.onSurfaceVariant,
        'Selected: $selected · Effective: $effective',
      ),
      ('disabled', 'consent_required') => (
        'External off',
        colors.onSurfaceVariant,
        'Selected: $selected · Effective: $effective · External providers are off',
      ),
      ('disabled', _) => (
        'Disabled',
        colors.onSurfaceVariant,
        'Selected: $selected · Effective: $effective',
      ),
      ('ready', _) => (
        'Ready',
        colors.primary,
        'Selected: $selected · Effective: $effective',
      ),
      ('available', _) => (
        'Available',
        colors.primary,
        'Selected: $selected · Effective: $effective · Ready to query on refresh',
      ),
      ('unavailable', final reason) => (
        'Unavailable',
        colors.error,
        'Selected: $selected · Effective: $effective · ${_describeStatusReason(reason)}',
      ),
      (final health, _) => (
        health,
        colors.onSurfaceVariant,
        'Selected: $selected · Effective: $effective',
      ),
    };
    return SettingRow(
      id: id,
      label: SettingLabel(title, description: detail),
      control: Text(
        badge,
        style: theme.textTheme.bodyMedium?.copyWith(color: badgeColor),
      ),
    );
  }

  List<Widget> _buildPreview(Map<String, dynamic>? value) => [
    SettingsGroup(
      title: 'Preview pane',
      children: [
        _toggle(
          'show_preview',
          'Show file preview',
          'Display a preview pane for the selected file or folder in Launcher.',
          value?['show_preview'] as bool?,
        ),
        _numberRow(_NumberField.textMaxBytes, value?['text_max_bytes'] as int?),
        _numberRow(
          _NumberField.imageMaxBytes,
          value?['image_max_bytes'] as int?,
        ),
      ],
    ),
  ];

  List<Widget> _buildActions(Map<String, dynamic>? value) => [
    SettingsGroup(
      title: 'Open with',
      children: [
        _textRow(_TextField.editor, value?['editor'] as String?),
        _textRow(_TextField.terminal, value?['terminal'] as String?),
        _textRow(_TextField.fileManager, value?['file_manager'] as String?),
      ],
    ),
  ];
}

List<String>? _stringList(Map<String, dynamic>? map, String key) {
  final raw = map?[key];
  return raw is List ? raw.cast<String>() : null;
}

_ProviderStatus? _parseProviderStatus(Object? raw) {
  if (raw is! Map) return null;
  final selected = raw['selected'];
  final effective = raw['effective'];
  final health = raw['health'];
  final reason = raw['reason'];
  if (selected is! String ||
      effective is! String ||
      health is! String ||
      reason is! String) {
    return null;
  }
  return (
    selected: selected,
    effective: effective,
    health: health,
    reason: reason,
  );
}

String _providerLabel(String provider) => switch (provider) {
  'memy' => 'Memy',
  'zoxide' => 'Zoxide',
  'xbel' => 'XBEL',
  'none' => 'None',
  _ => provider,
};

String _describeStatusReason(String reason) => switch (reason) {
  'history_disabled' => 'History is off',
  'consent_required' => 'External providers are off',
  'invalid_xbel' => 'Local XBEL file is unreadable',
  'not_installed' => 'Provider is not installed',
  'spawn_failed' => 'Provider could not start',
  'unsupported_version' => 'Unsupported provider version',
  'timeout' => 'Provider timed out',
  'output_too_large' => 'Provider output exceeded limit',
  'provider_error' => 'Provider returned an error',
  'invalid_output' => 'Provider returned invalid output',
  '' => 'Provider is unavailable',
  _ => reason,
};
