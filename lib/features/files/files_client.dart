import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

class FilesException implements Exception {
  const FilesException(this.message, {this.code});

  final String message;
  final String? code;

  @override
  String toString() => message;
}

/// One helper per Launcher window. Concurrent requests share its stdio pipes.
class FilesClient {
  FilesClient({String? executable})
    : _executable =
          executable ??
          '${File(Platform.resolvedExecutable).parent.path}/remya-launcher-files';

  final String _executable;
  final _pending = <int, _Request>{};
  Process? _process;
  Future<Process>? _starting;
  int _nextId = 0;
  bool _closed = false;

  Future<Map<String, dynamic>> request(
    String op, [
    Map<String, Object?> arguments = const {},
    void Function(Map<String, dynamic>)? onPartial,
  ]) async {
    if (_closed) throw const FilesException('Files service is closed.');
    final id = ++_nextId;
    final payload = utf8.encode(
      '${jsonEncode({...arguments, 'id': id, 'op': op})}\n',
    );
    if (payload.length > 4 * 1024 * 1024) {
      throw const FilesException('Files request is too long.');
    }
    final process = _process ?? await (_starting ??= _start());
    if (_closed || _process != process) {
      throw const FilesException('Files service disconnected.');
    }
    final response = Completer<Map<String, dynamic>>();
    _pending[id] = _Request(response, onPartial);
    try {
      process.stdin.add(payload);
      return await response.future.timeout(const Duration(seconds: 15));
    } on TimeoutException {
      throw const FilesException('Files service timed out. Try again.');
    } finally {
      _pending.remove(id);
    }
  }

  Future<Process> _start() async {
    try {
      final process = await Process.start(_executable, const []);
      if (_closed) {
        process.kill();
        throw const FilesException('Files service is closed.');
      }
      _process = process;
      unawaited(process.stderr.drain<void>().catchError((Object _) {}));
      unawaited(
        process.stdin.done.catchError((Object _) => _disconnect(process)),
      );
      _read(process);
      return process;
    } on ProcessException {
      throw const FilesException('Files service could not start.');
    } finally {
      _starting = null;
    }
  }

  void _read(Process process) {
    final buffer = BytesBuilder(copy: false);
    process.stdout.listen(
      (bytes) {
        var start = 0;
        while (start < bytes.length) {
          final newline = bytes.indexOf(10, start);
          final end = newline < 0 ? bytes.length : newline;
          if (buffer.length + end - start > 8 * 1024 * 1024) {
            _disconnect(process);
            return;
          }
          buffer.add(
            bytes is Uint8List
                ? Uint8List.sublistView(bytes, start, end)
                : bytes.sublist(start, end),
          );
          if (newline < 0) return;
          try {
            _response(jsonDecode(utf8.decode(buffer.takeBytes())));
          } on Object {
            _disconnect(process);
            return;
          }
          start = end + 1;
        }
      },
      onError: (Object _) => _disconnect(process),
      onDone: () => _disconnect(process),
    );
  }

  void _response(Object? value) {
    final envelope = value as Map<String, dynamic>;
    final id = envelope['id'];
    final request = _pending[id];
    if (request == null) return;
    final error = envelope['error'];
    if (error != null) {
      _pending.remove(id);
      request.response.completeError(
        error is Map<String, dynamic>
            ? FilesException(
                error['message'] as String? ?? 'Files request failed.',
                code: error['code'] as String?,
              )
            : FilesException(error.toString()),
      );
      return;
    }
    final data = envelope['data'] as Map<String, dynamic>;
    if (envelope['done'] == false || data['done'] == false) {
      request.onPartial?.call(data);
      return;
    }
    _pending.remove(id);
    request.response.complete(data);
  }

  void _disconnect(Process process) {
    if (_process != process) return;
    _process = null;
    process.kill();
    for (final request in _pending.values) {
      request.response.completeError(
        const FilesException('Files service disconnected. Try again.'),
      );
    }
    _pending.clear();
  }

  void close() {
    _closed = true;
    if (_process case final process?) _disconnect(process);
  }
}

class _Request {
  const _Request(this.response, this.onPartial);

  final Completer<Map<String, dynamic>> response;
  final void Function(Map<String, dynamic>)? onPartial;
}
