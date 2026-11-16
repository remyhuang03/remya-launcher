import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

import 'web_models.dart';

export 'web_models.dart';

class WebException implements Exception {
  const WebException(this.code, this.message, {this.retryable = false});

  final String code, message;
  final bool retryable;

  @override
  String toString() => message;
}

/// One lazy connection to the Web sidecar for each launcher or settings window.
class WebClient {
  WebClient({
    String? executablePath,
    this._requestTimeout = const Duration(seconds: 5),
  }) : _executablePath =
           executablePath ??
           '${File(Platform.resolvedExecutable).parent.path}/remya-launcher-web';

  static const _maxRequestBytes = 32 * 1024;
  static const _maxResponseBytes = 4 * 1024 * 1024;
  static const _maxIconBytes = 256 * 1024;
  static const _invalidResponse = WebException(
    'INVALID_RESPONSE',
    'Web search returned an invalid response.',
    retryable: true,
  );
  static const _disconnected = WebException(
    'DISCONNECTED',
    'Web search disconnected. Try again.',
    retryable: true,
  );
  static const _closedError = WebException('CLOSED', 'Web search is closed.');

  final String _executablePath;
  final Duration _requestTimeout;
  late final _installed =
      Platform.isLinux && File(_executablePath).existsSync();
  final _pending = <int, Completer<Object>>{};
  Process? _process;
  Future<Process>? _starting;
  bool _closed = false;
  int _nextId = 0;

  bool get available => !_closed && _installed;

  Future<WebSearchResult> search(
    String query, {
    String? engineId,
    List<String>? profileIds,
    int? limit,
  }) => _call('web.search', {
    'query': query,
    'engineId': ?engineId,
    'profileIds': ?profileIds,
    'limit': ?limit,
  }, WebSearchResult.fromJson);

  Future<List<WebEngine>> engines() =>
      _callList('web.engines.list', const {}, WebEngine.fromJson);

  Future<List<WebProfile>> profiles() =>
      _callList('web.profiles.list', const {}, WebProfile.fromJson);

  Future<void> setProfileEnabled(String profileId, bool enabled) async {
    await _request('web.profiles.setEnabled', {
      'profileId': profileId,
      'enabled': enabled,
    });
  }

  Future<WebSyncStatus> syncStatus() =>
      _call('web.sync.status', const {}, WebSyncStatus.fromJson);

  Future<void> requestSync({
    List<String>? profileIds,
    bool full = false,
  }) async {
    await _request('web.sync.request', {
      'profileIds': ?profileIds,
      'mode': full ? 'full' : 'incremental',
    });
  }

  Future<String> defaultEngine() =>
      _call('web.prefs.get', const {}, _defaultEngineId);

  Future<String> setDefaultEngine(String engineId) => _call('web.prefs.set', {
    'values': {'defaultEngineId': engineId},
  }, _defaultEngineId);

  static String _defaultEngineId(Map<String, dynamic> data) =>
      data['defaultEngineId'] as String;

  Future<void> clear() async {
    await _request('web.prefs.clear', const {});
  }

  Future<WebCapabilities> capabilities() =>
      _call('web.capabilities', const {}, WebCapabilities.fromJson);

  Future<List<WebAction>> actions(String entryId) => _callList(
    'web.actions.available',
    {'entryId': entryId},
    WebAction.fromJson,
  );

  Future<void> perform(
    String action, {
    String? entryId,
    String? url,
    String? engineId,
    String? browserId,
    String? profileId,
    String? eventId,
  }) async {
    final isNavigation = action.startsWith('open.') || action == 'search';
    await _request('web.perform', {
      'action': action,
      'target': {'entryId': ?entryId, 'url': ?url, 'engineId': ?engineId},
      'browserId': ?browserId,
      'profileId': ?profileId,
      'eventId': ?(eventId ?? (isNavigation ? _newEventId() : null)),
    });
  }

  Future<Uint8List?> favicon(
    String url, {
    String? profileId,
    String? iconKey,
  }) async {
    try {
      if (iconKey != null) {
        try {
          return await _readIcon(iconKey);
        } on WebException catch (error) {
          if (error.code != 'ICON_NOT_FOUND') return null;
        }
      }
      final icon = await _call('web.favicon.resolve', {
        'url': url,
        'profileId': ?profileId,
      }, WebFavicon.fromJson);
      if (icon.status != 'ready' || icon.iconKey == null) return null;
      return await _readIcon(icon.iconKey!);
    } on WebException {
      return null;
    }
  }

  Future<Uint8List> _readIcon(String iconKey) =>
      _call('web.favicon.read', {'iconKey': iconKey}, (data) {
        final encoded = data['pngBase64'] as String;
        if (encoded.length > ((_maxIconBytes + 2) ~/ 3) * 4) {
          throw _invalidResponse;
        }
        final bytes = base64Decode(encoded);
        if (bytes.isEmpty || bytes.length > _maxIconBytes) {
          throw _invalidResponse;
        }
        return bytes;
      });

  Future<T> _call<T>(
    String op,
    Map<String, Object?> args,
    T Function(Map<String, dynamic>) parse,
  ) async {
    final data = await _request(op, args);
    try {
      return parse(data as Map<String, dynamic>);
    } on Object {
      throw _invalidResponse;
    }
  }

  Future<List<T>> _callList<T>(
    String op,
    Map<String, Object?> args,
    T Function(Map<String, dynamic>) parse,
  ) async {
    final data = await _request(op, args);
    try {
      return (data as List)
          .cast<Map<String, dynamic>>()
          .map(parse)
          .toList(growable: false);
    } on Object {
      throw _invalidResponse;
    }
  }

  Future<Object> _request(String op, Map<String, Object?> args) async {
    if (_closed) throw _closedError;
    final id = ++_nextId;
    final bytes = utf8.encode('${jsonEncode({'id': id, 'op': op, ...args})}\n');
    if (bytes.length > _maxRequestBytes) {
      throw const WebException('REQUEST_TOO_LARGE', 'Web request is too long.');
    }
    final process = _process ?? await (_starting ??= _start());
    if (_closed) throw _closedError;
    if (process != _process) throw _disconnected;
    final response = Completer<Object>();
    _pending[id] = response;
    try {
      try {
        process.stdin.add(bytes);
      } on Object {
        _pending.remove(id);
        _disconnect(process, _disconnected);
        throw _disconnected;
      }
      return await response.future.timeout(_requestTimeout);
    } on TimeoutException {
      const error = WebException(
        'TIMEOUT',
        'Web search timed out. Try again.',
        retryable: true,
      );
      _pending.remove(id);
      _disconnect(process, error);
      throw error;
    } finally {
      _pending.remove(id);
    }
  }

  Future<Process> _start() async {
    try {
      final process = await Process.start(_executablePath, const [
        '--standalone',
      ]);
      if (_closed) {
        process.kill(ProcessSignal.sigkill);
        throw _closedError;
      }
      _process = process;
      unawaited(process.stderr.drain<void>().catchError((Object _) {}));
      unawaited(
        process.stdin.done.then<void>(
          (_) => _disconnect(process, _disconnected),
          onError: (Object _) => _disconnect(process, _disconnected),
        ),
      );
      _lines(process.stdout).listen(
        (line) => _receive(process, line),
        onError: (Object _) => _disconnect(process, _invalidResponse),
        onDone: () => _disconnect(process, _disconnected),
        cancelOnError: true,
      );
      return process;
    } on ProcessException {
      throw const WebException(
        'UNAVAILABLE',
        'Web search is unavailable.',
        retryable: true,
      );
    } finally {
      _starting = null;
    }
  }

  void _receive(Process process, String line) {
    if (process != _process) return;
    try {
      final frame = jsonDecode(line) as Map<String, dynamic>;
      final id = frame['id'] as int;
      final data = frame['data'];
      final error = frame['error'];
      if (data != null && error == null) {
        _pending.remove(id)?.complete(data);
      } else if (data == null &&
          error is Map<String, dynamic> &&
          error['code'] is String &&
          error['message'] is String) {
        _pending
            .remove(id)
            ?.completeError(
              WebException(
                error['code'] as String,
                error['message'] as String,
                retryable: error['retryable'] == true,
              ),
            );
      } else {
        _disconnect(process, _invalidResponse);
      }
    } on Object {
      _disconnect(process, _invalidResponse);
    }
  }

  void _disconnect(Process process, WebException error) {
    if (_process != process) return;
    _process = null;
    process.kill(ProcessSignal.sigkill);
    for (final pending in _pending.values) {
      pending.completeError(error);
    }
    _pending.clear();
  }

  Future<void> close() async {
    _closed = true;
    if (_process case final process?) {
      _disconnect(process, _closedError);
      await process.exitCode;
    }
  }

  static Stream<String> _lines(Stream<List<int>> chunks) async* {
    final frame = BytesBuilder(copy: false);
    await for (final chunk in chunks) {
      var start = 0;
      while (start < chunk.length) {
        final newline = chunk.indexOf(10, start);
        final end = newline < 0 ? chunk.length : newline;
        if (frame.length + end - start > _maxResponseBytes) {
          throw _invalidResponse;
        }
        frame.add(chunk.sublist(start, end));
        if (newline < 0) break;
        yield utf8.decode(frame.takeBytes());
        start = newline + 1;
      }
    }
    if (frame.isNotEmpty) throw _invalidResponse;
  }

  static String _newEventId() {
    final random = Random.secure();
    final bytes = Uint8List.fromList(
      List<int>.generate(16, (_) => random.nextInt(256)),
    );
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    final hex = bytes
        .map((byte) => byte.toRadixString(16).padLeft(2, '0'))
        .join();
    return [
      hex.substring(0, 8),
      hex.substring(8, 12),
      hex.substring(12, 16),
      hex.substring(16, 20),
      hex.substring(20),
    ].join('-');
  }
}
