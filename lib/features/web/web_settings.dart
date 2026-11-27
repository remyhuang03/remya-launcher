import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:remya_ui/settings_ui.dart';

import 'web_client.dart';

RemyaSettingsPage webSettingsPage(WebClient client) => RemyaSettingsPage(
  id: 'web',
  title: 'Web',
  icon: Icons.public_rounded,
  category: .system,
  builder: (_) => _WebSettings(client: client),
);

class _WebSettings extends StatefulWidget {
  const _WebSettings({required this.client});

  final WebClient client;

  @override
  State<_WebSettings> createState() => _WebSettingsState();
}

class _WebSettingsState extends State<_WebSettings> {
  WebSyncStatus? _status;
  String? _error, _defaultEngine;
  Timer? _poll;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    unawaited(_update());
  }

  @override
  void dispose() {
    _poll?.cancel();
    super.dispose();
  }

  Future<void> _update({
    Future<void> Function()? action,
    String? failure,
  }) async {
    if (_busy) return;
    _poll?.cancel();
    setState(() {
      _busy = true;
      _error = null;
    });
    String? error;
    if (action != null) {
      try {
        await action();
      } on Object {
        error = failure;
      }
    }
    if (!mounted) return;
    WebSyncStatus? status;
    try {
      status = await widget.client.syncStatus();
    } on Object {
      error ??= 'Could not load browser profiles.';
    }
    String? engine = _defaultEngine;
    if (engine == null && status != null) {
      try {
        engine = await widget.client.defaultEngine();
      } on Object {
        error ??= 'Could not load the default search engine.';
      }
    }
    if (!mounted) return;
    setState(() {
      _defaultEngine = engine;
      _status = status;
      _error = error;
      _busy = false;
    });
    if (status?.syncing == true && error == null) {
      _poll = Timer(const Duration(seconds: 1), () => unawaited(_update()));
    }
  }

  Future<void> _confirmClear() => _update(
    failure: 'Could not clear Launcher Web data.',
    action: () async {
      final confirmed = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Clear Launcher Web data?'),
          content: const Text(
            'This turns off all browser profiles and clears indexed pages, '
            'bookmarks, icons, hidden sites, and Web usage from Launcher. '
            'Your browser keeps its own history and bookmarks.',
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              style: destructiveFilledButtonStyle(context),
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Clear data'),
            ),
          ],
        ),
      );
      if (mounted && confirmed == true) await widget.client.clear();
    },
  );

  Widget _profileRow(BuildContext context, WebProfile profile) {
    final label = '${profile.browserLabel} · ${profile.label}';
    final status = switch ((profile.errorCode ?? profile.status)
        .toUpperCase()) {
      'SOURCE_LOCKED' || 'LOCKED' => 'Browser data is temporarily locked',
      'PROFILE_PERMISSION_DENIED' ||
      'PERMISSION_DENIED' => 'Browser data is inaccessible',
      'UNSUPPORTED_SCHEMA' => 'Browser data format is unsupported',
      'MISSING' ||
      'PROFILE_MISSING' ||
      'PROFILE_NOT_FOUND' => 'Profile not found',
      'SYNC_FAILED' || 'ERROR' => 'Could not sync this profile',
      _ when !profile.enabled => 'Off',
      'SYNCING' || 'PENDING' || 'QUEUED' => 'Syncing…',
      _ when profile.lastGoodSyncMs != null => 'Indexed',
      _ => 'Waiting for first sync',
    };
    final lastSync = profile.lastGoodSyncMs;
    final localizations = MaterialLocalizations.of(context);
    final date = lastSync == null
        ? null
        : DateTime.fromMillisecondsSinceEpoch(lastSync);
    return SettingRow(
      key: ValueKey(profile.profileId),
      id: profile.profileId,
      label: SettingLabel(
        label,
        description: date == null || !profile.enabled
            ? status
            : '$status · Last synced ${localizations.formatShortDate(date)} '
                  '${localizations.formatTimeOfDay(TimeOfDay.fromDateTime(date))}',
      ),
      control: Semantics(
        label: 'Search ${profile.browserLabel} profile ${profile.label}',
        child: Switch(
          value: profile.enabled,
          onChanged: _busy
              ? null
              : (enabled) => _update(
                  action: () => widget.client.setProfileEnabled(
                    profile.profileId,
                    enabled,
                  ),
                  failure: 'Could not update the browser profile.',
                ),
        ),
      ),
    );
  }

  Widget _searchSettings() => SettingsGroup(
    title: 'Search',
    children: [
      SettingRow(
        id: 'default-engine',
        label: const SettingLabel(
          'Default search engine',
          description:
              'Used by Ctrl+Enter and the first suggestion in Web search.',
        ),
        control: SettingsChoice<String>(
          label: 'Default search engine',
          selected: _defaultEngine,
          choices: const {'duckduckgo': 'DuckDuckGo', 'google': 'Google'},
          onSelected: _busy
              ? null
              : (engine) => unawaited(
                  _update(
                    action: () async {
                      final defaultEngine = await widget.client
                          .setDefaultEngine(engine);
                      if (mounted) _defaultEngine = defaultEngine;
                    },
                    failure: 'Could not change the default search engine.',
                  ),
                ),
        ),
      ),
    ],
  );

  @override
  Widget build(BuildContext context) {
    final status = _status;
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        const SettingsSurface(
          child: Text(
            'Turn on a browser profile to search its recent history and '
            'bookmarks. Launcher keeps the index on this device and removes '
            'a profile’s indexed data when you turn it off.',
          ),
        ),
        if (_error case final error?)
          Padding(
            padding: const .only(top: 12),
            child: SettingsErrorCard(
              message: error,
              onRetry: _busy ? null : _update,
            ),
          ),
        if (_busy && status == null) const RemyaLoadingBar(),
        if (_defaultEngine == 'duckduckgo' || _defaultEngine == 'google')
          _searchSettings(),
        if (status != null) ...[
          SettingsGroup(
            title: 'Browser profiles',
            trailing: TextButton.icon(
              onPressed: _busy || status.syncing
                  ? null
                  : () => _update(
                      action: () => widget.client.requestSync(full: true),
                      failure: 'Could not start browser sync.',
                    ),
              icon: const Icon(Icons.refresh_rounded),
              label: Text(status.syncing ? 'Syncing…' : 'Refresh'),
            ),
            children: [
              if (status.profiles.isEmpty)
                const Padding(
                  padding: .all(20),
                  child: Text('No supported browser profiles found.'),
                ),
              for (final profile in status.profiles)
                _profileRow(context, profile),
            ],
          ),
          SettingsGroup(
            title: 'Local Web data',
            children: [
              SettingRow(
                id: 'clear-web-data',
                label: const SettingLabel(
                  'Clear Web data',
                  description: 'Remove the Web index and saved Web activity from Launcher.',
                ),
                control: SettingsButton(
                  variant: .destructive,
                  onPressed: _busy ? null : _confirmClear,
                  label: 'Clear…',
                ),
              ),
            ],
          ),
        ],
      ],
    );
  }
}
