import 'dart:async';
import 'dart:io';

import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_ui/settings_ui.dart';

import 'startup_mode.dart';

const startupSettingsPageId = 'startup';

RemyaSettingsPage startupSettingsPage(SettingsClient client) =>
    RemyaSettingsPage(
      id: startupSettingsPageId,
      title: 'Startup',
      icon: Icons.speed_rounded,
      category: .system,
      builder: (_) => _StartupSettings(client: client),
    );

class _StartupSettings extends StatefulWidget {
  const _StartupSettings({required this.client});

  final SettingsClient client;

  @override
  State<_StartupSettings> createState() => _StartupSettingsState();
}

class _StartupSettingsState extends State<_StartupSettings> {
  late final _preferences = AppPreferences(
    launcherPreferencesId,
    client: widget.client,
  );
  StreamSubscription<bool>? _watch;
  bool? _resident;
  bool _saving = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    _listen();
  }

  void _listen() {
    unawaited(_watch?.cancel());
    _watch = _preferences
        .watch(residentStartupKey)
        .map(readResidentStartup)
        .listen(
          (resident) => setState(() => _resident = resident),
          onError: (Object _) => setState(() {
            _resident = null;
            _error = 'Could not read the startup preference from Remy\'a Core.';
          }),
        );
  }

  void _retry() {
    setState(() => _error = null);
    _listen();
  }

  Future<void> _save(bool resident) async {
    setState(() {
      _saving = true;
      _error = null;
    });
    var saved = false;
    try {
      final value = readResidentStartup(
        await _preferences.set(residentStartupKey, resident),
      );
      saved = true;
      if (mounted) setState(() => _resident = value);
      if (value) {
        final result = await Process.run('systemctl', [
          '--user',
          'start',
          'top.remya.launcher.gui.service',
        ]);
        if (result.exitCode != 0) throw StateError('Could not start Launcher.');
      }
    } on Object {
      if (mounted) {
        setState(
          () => _error = saved
              ? 'Fast startup was saved. Launcher could not prewarm in this session.'
              : 'Could not save the startup preference.',
        );
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  void dispose() {
    unawaited(_watch?.cancel());
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: .stretch,
    spacing: 12,
    children: [
      if (_resident == null && _error == null) const RemyaLoadingBar(),
      if (_error case final message?)
        SettingsErrorCard(
          message: message,
          onRetry: _resident == null ? _retry : null,
        ),
      SettingRow(
        id: residentStartupKey,
        label: const SettingLabel(
          'Startup mode',
          description:
              'On demand exits when closed. Fast startup keeps Launcher '
              'in memory and preloads it at sign-in.',
        ),
        control: SettingsSegmentedButton<bool>(
          segments: const [
            SettingsSegment(false, 'On demand'),
            SettingsSegment(true, 'Fast startup'),
          ],
          selected: _resident,
          onSelected: _resident == null || _saving ? null : _save,
        ),
      ),
    ],
  );
}
