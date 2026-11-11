import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';

class ClipboardPlatform {
  const ClipboardPlatform();

  static const _window = MethodChannel('top.remya.launcher/window');

  static Future<ClipboardPlatform> capture() async {
    try {
      final result = await _run('hyprctl', ['-j', 'activewindow']);
      if (result.exitCode != 0) return const ClipboardPlatform();
      final json = jsonDecode(result.stdout as String) as Map<String, dynamic>;
      final address = json['address'];
      final pid = json['pid'];
      if (address is String &&
          RegExp(r'^0x[0-9a-fA-F]+$').hasMatch(address) &&
          address != '0x0' &&
          pid is int &&
          pid > 0) {
        return _HyprlandClipboardPlatform(address, pid);
      }
    } on Object {
      // An unavailable compositor leaves copy-only behavior available.
    }
    return const ClipboardPlatform();
  }

  Future<void> close() => _window.invokeMethod<void>('close');

  Future<bool> pasteToPreviousWindow() async => false;

  Future<void> openLink(String link) async {
    final uri = Uri.tryParse(link);
    if (uri == null ||
        !(uri.scheme == 'http' || uri.scheme == 'https') ||
        !uri.hasAuthority ||
        uri.host.isEmpty) {
      throw const FormatException('This entry is not a web URL.');
    }
    await _checked('gio', ['open', uri.toString()], 'Could not open the link.');
  }

  Future<void> revealFiles(List<String> files) async {
    final paths = files.where((path) => path.startsWith('/')).toList();
    if (paths.isEmpty) {
      throw const FileSystemException('No local paths to reveal.');
    }
    final uris = paths.map((path) => Uri.file(path).toString()).toList();
    try {
      await _checked('gdbus', [
        'call',
        '--session',
        '--dest',
        'org.freedesktop.FileManager1',
        '--object-path',
        '/org/freedesktop/FileManager1',
        '--method',
        'org.freedesktop.FileManager1.ShowItems',
        jsonEncode(uris),
        '',
      ], 'Could not reveal the files.');
    } on ProcessException {
      await _checked('gio', [
        'open',
        File(paths.first).parent.path,
      ], 'Could not open the containing folder.');
    }
  }

  Future<void> runInTerminal(String command) async {
    final terminal = Platform.environment['TERMINAL'];
    if (terminal != null && terminal.isNotEmpty) {
      await _start(terminal, ['-e', '/bin/sh', '-lc', command]);
      return;
    }
    try {
      await _start('xdg-terminal-exec', ['/bin/sh', '-lc', command]);
    } on ProcessException {
      await _start('kitty', ['/bin/sh', '-lc', command]);
    }
  }

  Future<void> saveImage(String mime, Uint8List bytes) async {
    final path = await _window.invokeMethod<String>(
      'saveFile',
      'clipboard.${_imageExtension(mime)}',
    );
    if (path == null) return;
    final parent = File(path).parent;
    final staging = await parent.createTemp('.remya-clipboard-');
    try {
      final output = await File('${staging.path}/image')
          .writeAsBytes(bytes, flush: true);
      await output.rename(path);
    } finally {
      await staging.delete(recursive: true);
    }
  }

  Future<bool> openImageInEditor(String mime, Uint8List bytes) =>
      _openWith('clipboard.${_imageExtension(mime)}', bytes);

  Future<bool> openCodeInEditor(String text, String? language) => _openWith(
    'clipboard.${switch (language?.toLowerCase()) {
      'python' => 'py',
      'javascript' => 'js',
      'typescript' => 'ts',
      'rust' => 'rs',
      'dart' => 'dart',
      'shell' || 'bash' => 'sh',
      'json' => 'json',
      'html' => 'html',
      _ => 'txt',
    }}',
    utf8.encode(text),
  );

  Future<bool> _openWith(String name, List<int> bytes) async {
    final runtime = Platform.environment['XDG_RUNTIME_DIR'];
    if (runtime == null || !runtime.startsWith('/')) {
      throw const FileSystemException(
        'The user runtime directory is unavailable.',
      );
    }
    final directory = await Directory('$runtime/remya-launcher')
        .createTemp('editor-');
    final file = File('${directory.path}/$name');
    var opened = false;
    try {
      await file.writeAsBytes(bytes);
      opened = await _window.invokeMethod<bool>('openWith', file.path) ?? false;
      return opened;
    } finally {
      if (!opened) await directory.delete(recursive: true);
    }
  }

  static String _imageExtension(String mime) => switch (mime) {
    'image/jpeg' => 'jpg',
    'image/png' => 'png',
    'image/webp' => 'webp',
    'image/gif' => 'gif',
    'image/bmp' => 'bmp',
    'image/tiff' => 'tiff',
    _ => 'image',
  };

  static Future<void> _start(String executable, List<String> arguments) async {
    await Process.start(executable, arguments, mode: .detached);
  }

  static Future<void> _checked(
    String executable,
    List<String> arguments,
    String message,
  ) async {
    final result = await _run(executable, arguments);
    if (result.exitCode != 0) {
      throw ProcessException(executable, const [], message, result.exitCode);
    }
  }

  static Future<ProcessResult> _run(
    String executable,
    List<String> arguments,
  ) async {
    final process = await Process.start(executable, arguments);
    final stdout = process.stdout.transform(utf8.decoder).join();
    final stderr = process.stderr.transform(utf8.decoder).join();
    try {
      final (exitCode, output, errors) = await (
        process.exitCode,
        stdout,
        stderr,
      ).wait.timeout(const Duration(seconds: 2));
      return ProcessResult(process.pid, exitCode, output, errors);
    } on TimeoutException {
      process.kill(ProcessSignal.sigkill);
      throw ProcessException(
        executable,
        const [],
        'The desktop did not respond.',
      );
    }
  }
}

class _HyprlandClipboardPlatform extends ClipboardPlatform {
  const _HyprlandClipboardPlatform(this.address, this.pid);

  final String address;
  final int pid;

  static const _terminalClasses = {
    'alacritty',
    'com.mitchellh.ghostty',
    'com.raggesilver.blackbox',
    'foot',
    'footclient',
    'gnome-terminal',
    'gnome-terminal-server',
    'io.elementary.terminal',
    'kitty',
    'konsole',
    'org.gnome.console',
    'org.gnome.ptyxis',
    'org.kde.konsole',
    'org.wezfurlong.wezterm',
    'terminator',
    'tilix',
    'wezterm',
    'xfce4-terminal',
  };

  @override
  Future<bool> pasteToPreviousWindow() async {
    try {
      final result = await ClipboardPlatform._run('hyprctl', ['-j', 'clients']);
      if (result.exitCode != 0) return false;
      final clients = (jsonDecode(result.stdout as String) as List<dynamic>)
          .whereType<Map<String, dynamic>>();
      final target = clients
          .where(
            (client) =>
                client['address'] == address &&
                client['pid'] == pid &&
                client['mapped'] == true,
          )
          .firstOrNull;
      if (target == null) return false;
      final terminal = ['class', 'initialClass'].any((field) {
        final name = target[field];
        return name is String && _terminalClasses.contains(name.toLowerCase());
      });
      await ClipboardPlatform._window.invokeMethod<void>('hide');
      final modifiers = terminal ? 'CTRL + SHIFT' : 'CTRL';
      final sent = await ClipboardPlatform._run('hyprctl', [
        'dispatch',
        'hl.dsp.send_shortcut({mods = "$modifiers", key = "V", window = "address:$address"})',
      ]);
      if (sent.exitCode == 0 && (sent.stdout as String).trim() == 'ok') {
        await close();
        return true;
      }
    } on Object {
      // Copy succeeded; keep the launcher available to report manual paste.
    }
    await ClipboardPlatform._window.invokeMethod<void>('show');
    return false;
  }
}
