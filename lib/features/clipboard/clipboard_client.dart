import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

enum ClipboardKind {
  text('Text'),
  command('Command'),
  code('Code'),
  image('Image'),
  link('Link'),
  color('Color'),
  files('Files'),
  hidden('Hidden');

  const ClipboardKind(this.label);

  final String label;
}

class ClipboardEntry {
  ClipboardEntry.fromJson(Map<String, dynamic> json)
    : id = json['id'] as String,
      kind = ClipboardKind.values.byName(json['kind'] as String),
      title = json['title'] as String,
      summary = json['summary'] as String,
      copiedAt = DateTime.fromMillisecondsSinceEpoch(json['copiedAt'] as int),
      timesCopied = json['timesCopied'] as int,
      pinned = json['pinned'] as bool,
      mimeTypes = (json['mimeTypes'] as List<dynamic>).cast<String>(),
      thumbnailOnly = json['thumbnailOnly'] as bool,
      expiresAt = switch (json['expiresAt']) {
        final int value => DateTime.fromMillisecondsSinceEpoch(value),
        _ => null,
      },
      color = json['color'] as String?;

  final String id;
  final ClipboardKind kind;
  final String title;
  final String summary;
  final DateTime copiedAt;
  final int timesCopied;
  final bool pinned;
  final List<String> mimeTypes;
  final bool thumbnailOnly;
  final DateTime? expiresAt;
  final String? color;

  bool get canPaste => !thumbnailOnly;
}

class ClipboardDetail {
  ClipboardDetail.fromJson(Map<String, dynamic> json)
    : text = json['text'] as String?,
      thumbnail = switch (json['thumbnail']) {
        final String value => base64Decode(value),
        _ => null,
      },
      width = json['width'] as int?,
      height = json['height'] as int?,
      size = json['size'] as int?,
      files = (json['files'] as List<dynamic>?)?.cast<String>() ?? const [],
      language = json['language'] as String?;

  final String? text;
  final Uint8List? thumbnail;
  final int? width;
  final int? height;
  final int? size;
  final List<String> files;
  final String? language;
}

class ClipboardServiceException implements Exception {
  const ClipboardServiceException(this.message);

  final String message;

  @override
  String toString() => message;
}

class ClipboardClient {
  const ClipboardClient();

  Future<List<ClipboardEntry>> list({String? query}) async =>
      (await request('list', {'query': ?query}) as List<dynamic>)
          .map(
            (entry) => ClipboardEntry.fromJson(entry as Map<String, dynamic>),
          )
          .toList(growable: false);

  Future<bool> isEnabled() async =>
      (await request('preferences') as Map<String, dynamic>)['enabled'] as bool;

  Future<ClipboardDetail> detail(String id) async => ClipboardDetail.fromJson(
    await request('detail', {'id': id}) as Map<String, dynamic>,
  );

  Future<Uint8List?> thumbnail(String id) async =>
      switch (await request('thumbnail', {'id': id})) {
        final String value => base64Decode(value),
        _ => null,
      };

  Future<String?> reveal(String id) async =>
      await request('reveal', {'id': id}) as String?;

  Future<String> text(String id) async =>
      await request('text', {'id': id}) as String;

  Future<({String mime, Uint8List bytes})> image(String id) async {
    final result = await request('image', {'id': id}) as Map<String, dynamic>;
    return (
      mime: result['mime'] as String,
      bytes: base64Decode(result['data'] as String),
    );
  }

  Future<void> setPinned(String id, bool pinned) async {
    await request('setPinned', {'id': id, 'pinned': pinned});
  }

  Future<void> delete(String id) async {
    await request('delete', {'id': id});
  }

  Future<void> clear() async {
    await request('clear');
  }

  Future<void> publish(String id, {String mode = 'original'}) async {
    await request('publish', {'id': id, 'mode': mode});
  }

  Future<void> publishText(String text) async {
    await request('publish', {'text': text});
  }

  Future<Object?> request(
    String op, [
    Map<String, Object?> arguments = const {},
  ]) async {
    try {
      return await _exchange({'op': op, ...arguments})
          .timeout(const Duration(seconds: 8))
          .first;
    } on SocketException {
      throw const ClipboardServiceException(
        'Clipboard service is unavailable.',
      );
    } on TimeoutException {
      throw const ClipboardServiceException(
        'Clipboard service did not respond.',
      );
    }
  }

  Stream<void> watch() async* {
    try {
      await for (final _ in _exchange({'op': 'watch'})) {
        yield null;
      }
    } on SocketException {
      throw const ClipboardServiceException(
        'Clipboard service is unavailable.',
      );
    }
  }

  Stream<Object?> _exchange(Map<String, Object?> request) async* {
    final runtime = Platform.environment['XDG_RUNTIME_DIR'];
    if (runtime == null || !runtime.startsWith('/')) {
      throw const ClipboardServiceException(
        'Clipboard service is unavailable.',
      );
    }
    final socket = await Socket.connect(
      InternetAddress('$runtime/remya-launcher/clipboard.sock', type: .unix),
      0,
      timeout: const Duration(seconds: 2),
    );
    try {
      socket.add(utf8.encode('${jsonEncode(request)}\n'));
      await for (final line in _lines(socket)) {
        final response = jsonDecode(line) as Map<String, dynamic>;
        if (response['ok'] != true) {
          throw ClipboardServiceException(response['error'] as String);
        }
        yield response['value'];
      }
    } finally {
      socket.destroy();
    }
  }

  Stream<String> _lines(Socket socket) async* {
    final line = BytesBuilder();
    await for (final chunk in socket) {
      var start = 0;
      while (start < chunk.length) {
        final newline = chunk.indexOf(10, start);
        final end = newline < 0 ? chunk.length : newline;
        // A 20 MB source image expands to less than 28 MB in base64.
        if (line.length + end - start > 32 * 1024 * 1024) {
          throw const ClipboardServiceException(
            'Clipboard response is too large.',
          );
        }
        line.add(Uint8List.sublistView(chunk, start, end));
        if (newline < 0) break;
        yield utf8.decode(line.takeBytes());
        start = end + 1;
      }
    }
    if (line.isNotEmpty) {
      throw const ClipboardServiceException('Incomplete clipboard response.');
    }
  }
}
