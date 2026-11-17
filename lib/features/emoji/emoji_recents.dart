import 'dart:convert';
import 'dart:io';

/// An ordered list of canonical code-point IDs. Queries never enter this file.
class EmojiRecents {
  EmojiRecents({Directory? directory})
    : _directory = directory ?? _stateDirectory();

  static const _limit = 30;
  static const _maxBytes = 16 * 1024;
  static final _id = RegExp(r'^U\+[0-9A-F]{4,6}( U\+[0-9A-F]{4,6}){0,31}$');
  static Future<void> _pending = Future.value();

  final Directory _directory;

  File get _file => File('${_directory.path}/emoji-recents.json');

  Future<List<String>> read() async {
    final file = _file;
    try {
      if (!await file.exists() || await file.length() > _maxBytes) {
        return const [];
      }
      final value = jsonDecode(await file.readAsString());
      if (value is! List) return const [];
      return value
          .whereType<String>()
          .where(_id.hasMatch)
          .toSet()
          .take(_limit)
          .toList(growable: false);
    } on FormatException {
      return const [];
    } on FileSystemException {
      if (!await file.exists()) return const [];
      rethrow;
    }
  }

  Future<List<String>> record(String id) {
    if (!_id.hasMatch(id)) throw ArgumentError.value(id, 'id');
    return _mutate(() async {
      final previous = await read();
      final updated = [
        id,
        ...previous.where((entry) => entry != id),
      ].take(_limit).toList(growable: false);
      final staging = await _directory.createTemp('.emoji-recents-');
      try {
        final file = File('${staging.path}/recents.json');
        await file.writeAsString(jsonEncode(updated), flush: true);
        await file.rename(_file.path);
      } finally {
        await staging.delete(recursive: true);
      }
      return updated;
    });
  }

  Future<void> clear() => _mutate(() async {
    final file = _file;
    if (await file.exists()) await file.delete();
  });

  Future<T> _mutate<T>(Future<T> Function() action) {
    final result = _pending.then((_) async {
      await _ensureDirectory();
      final lock = await File('${_directory.path}/.emoji-recents.lock')
          .open(mode: .append);
      try {
        await lock.lock(.blockingExclusive);
        return await action();
      } finally {
        await lock.close();
      }
    });
    _pending = result.then((_) {}, onError: (Object _, StackTrace _) {});
    return result;
  }

  Future<void> _ensureDirectory() async {
    if (await _directory.exists()) return;
    await _directory.parent.create(recursive: true);
    final staging = await _directory.parent.createTemp('.emoji-state-');
    try {
      await staging.rename(_directory.path);
    } on FileSystemException {
      if (!await _directory.exists()) rethrow;
    } finally {
      if (await staging.exists()) await staging.delete();
    }
  }

  static Directory _stateDirectory() {
    final environment = Platform.environment;
    final state = environment['XDG_STATE_HOME'];
    final home = environment['HOME'];
    final base = switch ((state, home)) {
      (final String state, _) when state.startsWith('/') => state,
      (_, final String home) when home.startsWith('/') => '$home/.local/state',
      _ => throw const FileSystemException(
        'A user state directory is required',
      ),
    };
    return Directory('$base/remya/launcher/emoji');
  }
}
