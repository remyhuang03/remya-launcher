import 'dart:async';
import 'dart:convert';
import 'dart:io';

List<T> _entries<T>(Object? value, T Function(Map<String, dynamic>) parse) =>
    value is List
    ? value.cast<Map<String, dynamic>>().map(parse).toList(growable: false)
    : [];

class LauncherAppEntry {
  LauncherAppEntry.fromJson(Map<String, dynamic> json)
    : id = json['id'] as String,
      name = json['name'] as String,
      description = json['description'] as String? ?? '',
      icon = json['icon'] as String?,
      source = json['source'] as String?,
      terminal = json['terminal'] == true,
      windows = _entries(json['windows'], AppWindow.fromJson),
      actions = _entries(json['actions'], AppDesktopAction.fromJson),
      matchKind = json['matchKind'] as String?,
      matchText = json['matchText'] as String?,
      actionId = json['actionId'] as String?,
      pinned = json['pinned'] == true,
      hidden = json['hidden'] == true;

  final String id, name, description;
  final String? icon, source, matchKind, matchText, actionId;
  final bool terminal, pinned, hidden;
  final List<AppWindow> windows;
  final List<AppDesktopAction> actions;
}

class AppWindow {
  AppWindow.fromJson(Map<String, dynamic> json)
    : id = json['id'] as String,
      title = json['title'] as String,
      workspace = json['workspace'].toString();

  final String id, title, workspace;
}

/// An open Hyprland window, with the desktop app it belongs to when known.
class LauncherWindow {
  LauncherWindow.fromJson(Map<String, dynamic> json)
    : id = json['id'] as String,
      title = json['title'] as String,
      windowClass = json['class'] as String,
      workspace = json['workspace'].toString(),
      appId = json['appId'] as String?,
      appName = json['appName'] as String?,
      icon = json['icon'] as String?;

  final String id, title, windowClass, workspace;
  final String? appId, appName, icon;
}

class AppDesktopAction {
  AppDesktopAction.fromJson(Map<String, dynamic> json)
    : id = json['id'] as String,
      name = json['name'] as String;

  final String id, name;
}

class AppPackage {
  AppPackage.fromJson(Map<String, dynamic> json)
    : id = json['id'] as String,
      name = json['name'] as String,
      description = json['description'] as String? ?? '',
      source = json['source'] as String,
      command = json['command'] as String;

  final String id, name, description, source, command;
}

class AppsSearch {
  const AppsSearch({
    this.total = 0,
    this.matched = 0,
    this.apps = const [],
    this.frequent = const [],
    this.pinned = const [],
    this.packages = const [],
    this.hidden = false,
  });

  AppsSearch.fromJson(Map<String, dynamic> json)
    : total = json['total'] as int,
      matched = json['matched'] as int,
      apps = _entries(json['apps'], LauncherAppEntry.fromJson),
      frequent = _entries(json['frequent'], LauncherAppEntry.fromJson),
      pinned = _entries(json['pinned'], LauncherAppEntry.fromJson),
      packages = _entries(json['packages'], AppPackage.fromJson),
      hidden = json['hidden'] == true;

  final int total, matched;
  final List<LauncherAppEntry> apps, frequent, pinned;
  final List<AppPackage> packages;
  final bool hidden;
}

class AppsException implements Exception {
  const AppsException(this.message);
  final String message;
  @override
  String toString() => message;
}

/// One connection to the bundled Linux Apps helper, shared across queries.
class AppsClient {
  Process? _process;
  Future<Process>? _starting;
  final _pending = <int, Completer<Map<String, dynamic>>>{};
  int _nextId = 0;
  bool _closed = false;

  Future<AppsSearch> search(String query, {required bool packages}) async =>
      AppsSearch.fromJson(
        await _request('search', {'query': query, 'packages': packages}),
      );

  Future<Map<String, dynamic>> preferences([Map<String, Object?>? values]) =>
      _request('preferences', {'values': ?values});

  Future<List<LauncherAppEntry>> hidden() async => _entries(
    (await _request('hidden', const {}))['apps'],
    LauncherAppEntry.fromJson,
  );

  Future<List<LauncherWindow>> windows(String query) async => _entries(
    (await _request('windows', {'query': query}))['windows'],
    LauncherWindow.fromJson,
  );

  Future<String?> perform(
    String action, {
    String? appId,
    String query = '',
    String? target,
    String? packageId,
    String? source,
  }) async =>
      (await _request('perform', {
            'action': action,
            'query': query,
            'appId': ?appId,
            'target': ?target,
            'packageId': ?packageId,
            'source': ?source,
          }))['text']
          as String?;

  Future<Map<String, dynamic>> _request(
    String op,
    Map<String, Object?> arguments,
  ) async {
    if (_closed) throw const AppsException('Apps search is closed.');
    final process = _process ?? await (_starting ??= _start());
    if (_process != process) {
      throw const AppsException('Apps search is unavailable.');
    }
    final id = ++_nextId;
    final response = Completer<Map<String, dynamic>>();
    _pending[id] = response;
    try {
      process.stdin.writeln(jsonEncode({'id': id, 'op': op, ...arguments}));
      return await response.future.timeout(const Duration(seconds: 5));
    } on TimeoutException {
      _disconnect(process);
      throw const AppsException('Apps search timed out. Try again.');
    } finally {
      _pending.remove(id);
    }
  }

  Future<Process> _start() async {
    try {
      final executable = File(Platform.resolvedExecutable);
      final path = '${executable.parent.path}/remya-launcher-apps';
      final process = await Process.start(path, const []);
      if (_closed) {
        process.kill();
        throw const AppsException('Apps search is closed.');
      }
      _process = process;
      unawaited(process.stderr.drain<void>());
      unawaited(
        process.stdin.done.catchError((Object _) {
          _disconnect(process);
        }),
      );
      process.stdout
          .transform(utf8.decoder)
          .transform(const LineSplitter())
          .listen(
            (line) => _receive(process, line),
            onError: (Object _) => _disconnect(process),
            onDone: () => _disconnect(process),
            cancelOnError: true,
          );
      return process;
    } on ProcessException {
      throw const AppsException('Apps search is unavailable.');
    } finally {
      _starting = null;
    }
  }

  void _receive(Process process, String line) {
    if (process != _process) return;
    try {
      final frame = jsonDecode(line) as Map<String, dynamic>;
      final data = frame['data'];
      final error = frame['error'];
      final pending = _pending.remove(frame['id']);
      if (pending == null) return;
      if (data is Map<String, dynamic>) {
        pending.complete(data);
      } else {
        pending.completeError(
          AppsException(error is String ? error : 'Apps request failed.'),
        );
      }
    } on Object {
      _disconnect(process);
    }
  }

  void _disconnect(Process process) {
    if (_process != process) return;
    _process = null;
    process.kill();
    for (final pending in _pending.values) {
      pending.completeError(const AppsException('Apps search disconnected.'));
    }
    _pending.clear();
  }

  Future<void> close() async {
    _closed = true;
    if (_process case final process?) _disconnect(process);
  }
}
