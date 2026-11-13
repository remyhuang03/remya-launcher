import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:remya_client/remya_client.dart';

import 'clipboard_client.dart';

typedef ClipboardPolicy = ({bool enabled, int maxItems, int retentionDays});

const clipboardAppId = 'remya-launcher';
const clipboardHistoryKey = 'clipboard-history';
const clipboardMaxItemsKey = 'clipboard-max-items';
const clipboardRetentionDaysKey = 'clipboard-retention-days';

/// Clipboard policy stored by Remya Core, plus Clear history on the launcher socket.
///
/// Keeps the last valid policy while Core or the clipboard service fails.
class ClipboardPolicyController extends ChangeNotifier {
  ClipboardPolicyController({
    required SettingsClient client,
    required this._clipboard,
  }) : _client = client,
       _preferences = AppPreferences(clipboardAppId, client: client);

  final SettingsClient _client;
  final ClipboardClient _clipboard;
  final AppPreferences _preferences;
  final _subscriptions = <StreamSubscription<Object?>>[];

  ClipboardPolicy? policy;
  String? policyError;
  String? historyError;
  bool busy = false;
  bool _refreshPending = false;
  bool _disposed = false;

  /// Subscribes before the first read so a concurrent write cannot be missed.
  void start() {
    _subscriptions
      ..add(
        _client.changes
            .where((change) => change.topic.startsWith('$clipboardAppId.'))
            .listen((_) => unawaited(refresh()), onError: _signalError),
      )
      // Core restarting or appearing later makes the policy readable again.
      ..add(
        _client.serviceAvailableChanges.listen(
          (_) => unawaited(refresh()),
          onError: _signalError,
        ),
      );
    unawaited(refresh());
  }

  Future<void> refresh() async {
    if (busy) {
      _refreshPending = true;
      return;
    }
    await _run();
  }

  Future<void> setEnabled(bool value) => _run(clipboardHistoryKey, value);

  Future<void> setMaxItems(int value) => _run(clipboardMaxItemsKey, value);

  Future<void> setRetentionDays(int value) =>
      _run(clipboardRetentionDaysKey, value);

  Future<bool> clearHistory() async {
    if (busy) return false;
    _update(() => busy = true);
    try {
      await _clipboard.clear();
      _update(() => historyError = null);
      return true;
    } on Object catch (error) {
      _update(() => historyError = describeError(error));
      return false;
    } finally {
      _finish();
    }
  }

  Future<void> _run([String? key, Object? value]) async {
    if (busy || _disposed) return;
    _update(() => busy = true);
    try {
      if (key != null) _validate(key, await _preferences.set(key, value));
      _refreshPending = false;
      final (enabled, maxItems, retentionDays) = await (
        _read(clipboardHistoryKey),
        _read(clipboardMaxItemsKey),
        _read(clipboardRetentionDaysKey),
      ).wait;
      _update(() {
        policy = (
          enabled: enabled as bool,
          maxItems: maxItems as int,
          retentionDays: retentionDays as int,
        );
        policyError = null;
      });
    } on ParallelWaitError<dynamic, dynamic> catch (error) {
      final first = [
        error.errors.$1,
        error.errors.$2,
        error.errors.$3,
      ].whereType<AsyncError>().first;
      _update(() => policyError = describeError(first.error));
    } on Object catch (error) {
      _update(() => policyError = describeError(error));
    } finally {
      _finish();
    }
  }

  Future<Object> _read(String key) async =>
      _validate(key, await _preferences.get(key));

  /// Rejects replies that do not match the registered schema.
  static Object _validate(String key, Map<String, dynamic> reply) {
    final value = reply['value'];
    final valid =
        reply['id'] == '$clipboardAppId.$key' &&
        reply['availability'] == 'available' &&
        switch (key) {
          clipboardHistoryKey => value is bool,
          clipboardMaxItemsKey => value is int && value >= 1 && value <= 5000,
          _ => value is int && value >= 1 && value <= 365,
        };
    if (!valid) {
      throw FormatException('Remya Core returned an invalid $key value.');
    }
    return value as Object;
  }

  void _signalError(Object error) =>
      _update(() => policyError = describeError(error));

  void _finish() {
    _update(() => busy = false);
    if (_refreshPending && !_disposed) {
      _refreshPending = false;
      unawaited(refresh());
    }
  }

  void _update(VoidCallback change) {
    if (_disposed) return;
    change();
    notifyListeners();
  }

  @override
  void dispose() {
    _disposed = true;
    for (final subscription in _subscriptions) {
      unawaited(subscription.cancel());
    }
    super.dispose();
  }
}

/// D-Bus errors read as `Name: message` without the interface prefix.
String describeError(Object error) => switch (error) {
  ClipboardServiceException(:final message) => message,
  FormatException(:final message) => message,
  TimeoutException() => 'Remya Core did not respond.',
  _ => '$error'.replaceFirst(RegExp(r'^(?:[\w-]+\.)+(?=\w+: )'), ''),
};
