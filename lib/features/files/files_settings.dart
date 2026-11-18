import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_ui/settings_ui.dart';

import '../clipboard/clipboard_policy.dart' show describeError;
import 'files_client.dart';
import 'files_preferences.dart';

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
        : OutlinedButton(
            onPressed: _editable ? () => _editList(spec, value) : null,
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 260),
              child: Text(spec.summary(value), overflow: .ellipsis),
            ),
          ),
  );

  SettingRow _numberRow(_NumberField spec, int? value) => SettingRow(
    id: spec.key,
    label: SettingLabel(spec.title, description: spec.description),
    control: value == null
        ? const Text('Unavailable')
        : OutlinedButton(
            onPressed: _editable ? () => _editNumber(spec, value) : null,
            child: Text(spec.format(value)),
          ),
  );

  SettingRow _textRow(_TextField spec, String? value) => SettingRow(
    id: spec.key,
    label: SettingLabel(spec.title, description: spec.description),
    control: value == null
        ? const Text('Unavailable')
        : OutlinedButton(
            onPressed: _editable ? () => _editText(spec, value) : null,
            child: ConstrainedBox(
              constraints: const BoxConstraints(maxWidth: 260),
              child: Text(
                value.isEmpty ? 'Automatic' : value,
                overflow: .ellipsis,
              ),
            ),
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
      trailing: OutlinedButton.icon(
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
          control: FilledButton.tonal(
            style: destructiveTonalButtonStyle(context),
            onPressed: _clearBusy ? null : _clearHistory,
            child: const Text('Clear history'),
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

bool _listEquals(List<String> a, List<String> b) {
  if (a.length != b.length) return false;
  for (var i = 0; i < a.length; i++) {
    if (a[i] != b[i]) return false;
  }
  return true;
}

String _formatBytesLabel(int bytes) {
  const kib = 1024;
  const mib = 1024 * 1024;
  if (bytes >= mib) {
    final formatted = bytes % mib == 0
        ? '${bytes ~/ mib}'
        : (bytes / mib).toStringAsFixed(1);
    return '$formatted MiB ($bytes B)';
  }
  if (bytes >= kib) {
    final formatted = bytes % kib == 0
        ? '${bytes ~/ kib}'
        : (bytes / kib).toStringAsFixed(1);
    return '$formatted KiB ($bytes B)';
  }
  return '$bytes B';
}

enum _ListField {
  roots(
    'roots',
    'Search roots',
    'Folders searched by Launcher, one absolute path (/...) or ~/... path '
        'per line.',
    'Folders (one per line)',
    'Enter one absolute path (/...) or ~/... path per line.',
    minItems: 1,
    maxItems: FilesPreferences.maxRoots,
  ),
  exclusions(
    'exclusions',
    'Excluded paths',
    'Literal local paths skipped during search, one absolute path (/...) or '
        '~/... path per line (no glob or relative syntax).',
    'Excluded paths (one per line)',
    'Enter one literal absolute path (/...) or ~/... path per line (for '
        'example, ~/Projects/build). Leave empty for none.',
    minItems: 0,
    maxItems: FilesPreferences.maxExclusions,
  );

  const _ListField(
    this.key,
    this.title,
    this.description,
    this.inputLabel,
    this.helperText, {
    required this.minItems,
    required this.maxItems,
  });

  final String key;
  final String title;
  final String description;
  final String inputLabel;
  final String helperText;
  final int minItems;
  final int maxItems;

  String summary(List<String> items) => switch (this) {
    roots => items.length == 1 ? items.single : '${items.length} folders',
    exclusions =>
      items.isEmpty
          ? 'None'
          : items.length == 1
          ? items.single
          : '${items.length} paths',
  };

  String? validateItems(List<String> items) {
    if (items.length < minItems) {
      return 'Enter at least one root path.';
    }
    if (items.length > maxItems) {
      return 'Enter at most $maxItems entries.';
    }
    if (items.toSet().length != items.length) {
      return 'Remove duplicate entries.';
    }
    for (final item in items) {
      final ok = switch (this) {
        roots => FilesPreferences.isValidRootPath(item),
        exclusions => FilesPreferences.isValidExclusion(item),
      };
      if (!ok) {
        return switch (this) {
          roots =>
            'Each root must be an absolute path (/...) or ~/... path '
                'without control characters.',
          exclusions =>
            'Each excluded path must be a literal absolute path (/...) or '
                '~/... path of 1 to ${FilesPreferences.maxExclusionLength} '
                'characters without control characters.',
        };
      }
    }
    return null;
  }
}

enum _NumberField {
  contentsMaxBytes(
    'contents_max_bytes',
    'Content search size limit',
    'Skip searching file contents for files larger than this byte limit.',
    'Size in bytes',
    FilesPreferences.minBytesLimit,
    FilesPreferences.maxContentsBytes,
    isBytes: true,
  ),
  resultLimit(
    'result_limit',
    'Result limit',
    'Maximum number of file results returned per search query.',
    'Maximum results',
    FilesPreferences.minResultLimit,
    FilesPreferences.maxResultLimit,
    unit: 'results',
  ),
  retentionDays(
    'retention_days',
    'Retention period',
    'How many days local file and folder history entries are kept.',
    'Days',
    1,
    FilesPreferences.maxRetentionDays,
    unit: 'days',
  ),
  maxRecordsPerKind(
    'max_records_per_kind',
    'Maximum records per kind',
    'Maximum number of file entries and directory entries stored separately.',
    'Maximum records',
    1,
    FilesPreferences.maxHistoryRecords,
    unit: 'records',
  ),
  textMaxBytes(
    'text_max_bytes',
    'Text preview size limit',
    'Maximum file size in bytes loaded into the text preview pane.',
    'Size in bytes',
    FilesPreferences.minBytesLimit,
    FilesPreferences.maxTextPreviewBytes,
    isBytes: true,
  ),
  imageMaxBytes(
    'image_max_bytes',
    'Image preview size limit',
    'Maximum file size in bytes decoded for the image preview pane.',
    'Size in bytes',
    FilesPreferences.minBytesLimit,
    FilesPreferences.maxImagePreviewBytes,
    isBytes: true,
  );

  const _NumberField(
    this.key,
    this.title,
    this.description,
    this.inputLabel,
    this.min,
    this.max, {
    this.unit = '',
    this.isBytes = false,
  });

  final String key;
  final String title;
  final String description;
  final String inputLabel;
  final int min;
  final int max;
  final String unit;
  final bool isBytes;

  String format(int value) =>
      isBytes ? _formatBytesLabel(value) : '$value $unit';

  String helperText(int current) => isBytes
      ? 'Enter $min to $max bytes (current: ${_formatBytesLabel(current)}).'
      : 'Enter $min to $max $unit.';
}

enum _TextField {
  editor(
    'editor',
    'Editor',
    'Executable name or absolute pathname (no arguments or shell command). '
        'Leave empty to use VISUAL/EDITOR, then the default text app.',
    'Editor executable or path',
    'Enter an executable name (for example, nvim) or an absolute pathname '
        '(for example, /usr/bin/hx). Leave empty to use VISUAL/EDITOR, then '
        'the default text app.',
    'Enter an executable name (without spaces) or an absolute path (/...) '
        'without shell operators or control characters.',
  ),
  terminal(
    'terminal',
    'Terminal',
    'Executable name or absolute pathname (no arguments or shell command). '
        'Leave empty for automatic selection.',
    'Terminal executable or path',
    'Enter an executable name (for example, kitty) or an absolute pathname '
        '(for example, /usr/bin/foot). Leave empty for automatic.',
    'Enter an executable name (without spaces) or an absolute path (/...) '
        'without shell operators or control characters.',
  ),
  fileManager(
    'file_manager',
    'File manager',
    'Executable name or absolute pathname (no arguments or shell command). '
        'Leave empty for automatic selection.',
    'File manager executable or path',
    'Enter an executable name (for example, nautilus) or an absolute pathname '
        '(for example, /usr/bin/dolphin). Leave empty for automatic.',
    'Enter an executable name (without spaces) or an absolute path (/...) '
        'without shell operators or control characters.',
  );

  const _TextField(
    this.key,
    this.title,
    this.description,
    this.inputLabel,
    this.helperText,
    this.invalidMessage,
  );

  final String key;
  final String title;
  final String description;
  final String inputLabel;
  final String helperText;
  final String invalidMessage;
}

class _LineListDialog extends StatefulWidget {
  const _LineListDialog({required this.spec, required this.current});

  final _ListField spec;
  final List<String> current;

  @override
  State<_LineListDialog> createState() => _LineListDialogState();
}

class _LineListDialogState extends State<_LineListDialog> {
  final _form = GlobalKey<FormState>();
  late String _text = widget.current.join('\n');

  List<String> _parse(String raw) => raw
      .split('\n')
      .map((line) => line.trim())
      .where((line) => line.isNotEmpty)
      .toList();

  void _submit() {
    if (_form.currentState!.validate()) {
      Navigator.pop(context, _parse(_text));
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.spec.title),
    content: SizedBox(
      width: 440,
      child: Form(
        key: _form,
        child: TextFormField(
          autofocus: true,
          initialValue: _text,
          minLines: 4,
          maxLines: 10,
          keyboardType: .multiline,
          decoration: InputDecoration(
            labelText: widget.spec.inputLabel,
            helperText: widget.spec.helperText,
            helperMaxLines: 3,
          ),
          onChanged: (value) => _text = value,
          validator: (text) => widget.spec.validateItems(_parse(text ?? '')),
        ),
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(onPressed: _submit, child: const Text('Save')),
    ],
  );
}

class _NumberDialog extends StatefulWidget {
  const _NumberDialog({required this.spec, required this.current});

  final _NumberField spec;
  final int current;

  @override
  State<_NumberDialog> createState() => _NumberDialogState();
}

class _NumberDialogState extends State<_NumberDialog> {
  final _form = GlobalKey<FormState>();
  late int? _value = widget.current;

  void _submit() {
    if (_form.currentState!.validate()) Navigator.pop(context, _value);
  }

  @override
  Widget build(BuildContext context) {
    final spec = widget.spec;
    return AlertDialog(
      title: Text(spec.title),
      content: Form(
        key: _form,
        child: TextFormField(
          autofocus: true,
          initialValue: '${widget.current}',
          keyboardType: .number,
          decoration: InputDecoration(
            labelText: spec.inputLabel,
            helperText: spec.helperText(widget.current),
            helperMaxLines: 2,
          ),
          onChanged: (text) => _value = int.tryParse(text.trim()),
          onFieldSubmitted: (_) => _submit(),
          validator: (text) {
            final value = int.tryParse(text?.trim() ?? '');
            return value == null || value < spec.min || value > spec.max
                ? 'Enter a whole number from ${spec.min} to ${spec.max}.'
                : null;
          },
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Save')),
      ],
    );
  }
}

class _TextDialog extends StatefulWidget {
  const _TextDialog({required this.spec, required this.current});

  final _TextField spec;
  final String current;

  @override
  State<_TextDialog> createState() => _TextDialogState();
}

class _TextDialogState extends State<_TextDialog> {
  final _form = GlobalKey<FormState>();
  late String _text = widget.current;

  void _submit() {
    if (_form.currentState!.validate()) {
      Navigator.pop(context, _text.trim());
    }
  }

  @override
  Widget build(BuildContext context) {
    final spec = widget.spec;
    return AlertDialog(
      title: Text(spec.title),
      content: SizedBox(
        width: 440,
        child: Form(
          key: _form,
          child: TextFormField(
            autofocus: true,
            initialValue: _text,
            keyboardType: .text,
            decoration: InputDecoration(
              labelText: spec.inputLabel,
              helperText: spec.helperText,
              helperMaxLines: 3,
            ),
            onChanged: (value) => _text = value,
            onFieldSubmitted: (_) => _submit(),
            validator: (text) =>
                FilesPreferences.isValidActionTarget((text ?? '').trim())
                ? null
                : spec.invalidMessage,
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(onPressed: _submit, child: const Text('Save')),
      ],
    );
  }
}
