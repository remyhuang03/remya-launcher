import 'dart:async';

import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_ui/settings_ui.dart';

import '../clipboard/clipboard_policy.dart' show describeError;
import 'emoji_recents.dart';

const _appId = 'remya-launcher';
const emojiSettingsPageId = 'emoji';
const emojiAutoPasteKey = 'emoji-auto-paste';
const emojiRecentsKey = 'emoji-recents';

class EmojiPreferences {
  const EmojiPreferences({this.autoPaste = false, this.recents = false});

  final bool autoPaste;
  final bool recents;

  static Future<EmojiPreferences> read([SettingsClient? client]) async {
    final preferences = AppPreferences(_appId, client: client);
    try {
      return EmojiPreferences(
        autoPaste: await _read(preferences, emojiAutoPasteKey),
        recents: await _read(preferences, emojiRecentsKey),
      );
    } finally {
      await preferences.close();
    }
  }

  static Future<bool> _read(AppPreferences preferences, String key) async =>
      _validate(key, await preferences.get(key));

  static bool _validate(String key, Map<String, dynamic> reply) {
    final value = reply['value'];
    if (reply['id'] == '$_appId.$key' &&
        reply['availability'] == 'available' &&
        value is bool) {
      return value;
    }
    throw FormatException('Remy\'a Core returned an invalid $key value.');
  }
}

RemyaSettingsPage emojiSettingsPage(SettingsClient client) => RemyaSettingsPage(
  id: emojiSettingsPageId,
  title: 'Emoji',
  icon: Icons.mood_rounded,
  category: .system,
  builder: (_) => _EmojiSettings(client: client),
);

class _EmojiSettings extends StatefulWidget {
  const _EmojiSettings({required this.client});

  final SettingsClient client;

  @override
  State<_EmojiSettings> createState() => _EmojiSettingsState();
}

class _EmojiSettingsState extends State<_EmojiSettings> {
  late final AppPreferences _appPreferences = AppPreferences(
    _appId,
    client: widget.client,
  );
  final _subscriptions = <StreamSubscription<Object?>>[];
  EmojiPreferences? _preferences;
  int _revision = 0;
  bool _loading = true;
  bool _busy = false;
  String? _readError;
  String? _error;

  @override
  void initState() {
    super.initState();
    _subscriptions.add(
      widget.client.changes
          .where(
            (change) =>
                change.topic == '$_appId.$emojiAutoPasteKey' ||
                change.topic == '$_appId.$emojiRecentsKey',
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
    final revision = ++_revision;
    setState(() {
      _loading = true;
      _readError = null;
    });
    try {
      final preferences = await EmojiPreferences.read(widget.client);
      if (mounted && revision == _revision) {
        setState(() {
          _preferences = preferences;
          _loading = false;
        });
      }
    } on Object catch (error) {
      if (mounted && revision == _revision) _watchError(error);
    }
  }

  void _watchError(Object error) => setState(() {
    _revision++;
    _loading = false;
    _readError = describeError(error);
  });

  Future<void> _perform(
    String failureMessage,
    Future<void> Function() action,
  ) async {
    if (_busy) return;
    setState(() {
      _error = null;
      _busy = true;
    });
    try {
      await action();
    } on Object catch (error) {
      if (mounted) {
        setState(() => _error = '$failureMessage\n${describeError(error)}');
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _save(String key, bool enabled) => _perform(
    'Could not save Emoji preferences.',
    () async {
      EmojiPreferences._validate(key, await _appPreferences.set(key, enabled));
      if (mounted) await _reload();
    },
  );

  Future<void> _clear() async {
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Clear recent emoji?'),
        content: const Text('Remove all saved recent emoji from this device.'),
        actions: [
          TextButton(
            autofocus: true,
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: destructiveFilledButtonStyle(context),
            onPressed: () => Navigator.pop(context, true),
            child: const Text('Clear recents'),
          ),
        ],
      ),
    );
    if (!mounted || confirmed != true) return;
    await _perform('Could not clear recent emoji.', () async {
      await EmojiRecents().clear();
      if (!mounted) return;
      ScaffoldMessenger.of(context)
          .showSnackBar(const SnackBar(content: Text('Recent emoji cleared')));
    });
  }

  SettingRow _toggle(
    String key,
    String title,
    String description,
    bool? enabled,
    bool editable,
  ) => SettingRow(
    id: key,
    label: SettingLabel(title, description: description),
    control: enabled == null
        ? const Text('Unavailable')
        : Semantics(
            label: title,
            child: Switch(
              value: enabled,
              onChanged: editable ? (value) => _save(key, value) : null,
            ),
          ),
  );

  @override
  void dispose() {
    for (final subscription in _subscriptions) {
      unawaited(subscription.cancel());
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final editable = !_busy && !_loading && _readError == null;
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        if (_loading) const RemyaLoadingBar(),
        if (_readError case final error?)
          SettingsErrorCard(
            message:
                'Remy\'a Core cannot provide Emoji preferences.\n'
                '$error',
            onRetry: _busy ? null : _reload,
          ),
        if (_error case final error?)
          SettingsErrorCard(message: error, actionLabel: null),
        SettingsGroup(
          title: 'Emoji search',
          children: [
            _toggle(
              emojiAutoPasteKey,
              'Copy and paste emoji',
              'Use Ctrl+Enter to copy and paste into the previously focused window.',
              _preferences?.autoPaste,
              editable,
            ),
            _toggle(
              emojiRecentsKey,
              'Remember recent emoji',
              'Save copied emoji locally so they appear first when the search is empty.',
              _preferences?.recents,
              editable,
            ),
            SettingRow(
              id: 'clear-emoji-recents',
              label: const SettingLabel('Clear recent emoji'),
              control: FilledButton.tonal(
                style: destructiveTonalButtonStyle(context),
                onPressed: _busy ? null : _clear,
                child: const Text('Clear recents'),
              ),
            ),
          ],
        ),
      ],
    );
  }
}
