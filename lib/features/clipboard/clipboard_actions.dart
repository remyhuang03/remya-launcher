import 'package:flutter/services.dart';

import '../launcher/launcher_controls.dart';
import 'clipboard_client.dart';
import 'clipboard_color.dart';
import 'clipboard_platform.dart';

class ClipboardActions {
  const ClipboardActions({
    required this.client,
    required this.platform,
    required this.onPasted,
    required this.onCopied,
    required this.onChanged,
    required this.editText,
  });

  final ClipboardClient client;
  final ClipboardPlatform platform;
  final Future<void> Function() onPasted;
  final void Function() onCopied;
  final Future<void> Function() onChanged;
  final Future<String?> Function(String) editText;

  Future<void> paste(ClipboardEntry entry, {String mode = 'original'}) async {
    await client.publish(entry.id, mode: mode);
    await onPasted();
  }

  Future<void> pasteText(String text) async {
    await client.publishText(text);
    await onPasted();
  }

  Future<void> _copy(ClipboardEntry entry) async {
    await client.publish(entry.id);
    onCopied();
  }

  Future<void> _delete(ClipboardEntry entry) async {
    await client.delete(entry.id);
    await onChanged();
  }

  Future<void> _pin(ClipboardEntry entry) async {
    await client.setPinned(entry.id, !entry.pinned);
    await onChanged();
  }

  Future<void> _edit(ClipboardEntry entry) async {
    final edited = await editText(await client.text(entry.id));
    if (edited != null) await pasteText(edited);
  }

  Future<void> _openCode(ClipboardEntry entry) async {
    final (text, detail) = await (
      client.text(entry.id),
      client.detail(entry.id),
    ).wait;
    if (await platform.openCodeInEditor(text, detail.language)) {
      await platform.close();
    }
  }

  Future<void> _openLink(ClipboardEntry entry) async {
    await platform.openLink((await client.text(entry.id)).trim());
    await platform.close();
  }

  Future<void> _pasteMarkdown(ClipboardEntry entry) async {
    final text = (await client.text(entry.id)).trim();
    final uri = Uri.parse(text);
    final url = text.replaceAll('<', '%3C').replaceAll('>', '%3E');
    final label = uri.host.replaceAll('[', r'\[').replaceAll(']', r'\]');
    await pasteText('[$label](<$url>)');
  }

  Future<void> _imageAction(ClipboardEntry entry, {required bool save}) async {
    final image = await client.image(entry.id);
    if (save) {
      await platform.saveImage(image.mime, image.bytes);
    } else if (await platform.openImageInEditor(image.mime, image.bytes)) {
      await platform.close();
    }
  }

  List<ClipboardAction> forEntry(ClipboardEntry entry) {
    final color = clipboardColor(entry.color);
    final formats = color == null
        ? const <String>[]
        : clipboardColorFormats(entry.color!, color);
    return [
      if (entry.canPaste)
        ClipboardAction(LogicalKeyboardKey.enter, '↵', switch (entry.kind) {
          .text when entry.mimeTypes.contains('text/html') =>
            'Paste (keeps formatting)',
          .image => 'Paste image',
          .link => 'Paste URL',
          .files => 'Paste files',
          .color => 'Paste as copied',
          _ => 'Paste',
        }, () => paste(entry)),
      ...switch (entry.kind) {
        .text => [
          ClipboardAction(
            LogicalKeyboardKey.enter,
            '↵',
            'Paste as plain text',
            () => paste(entry, mode: 'plain'),
            ctrl: true,
          ),
        ],
        .command => [
          ClipboardAction(
            LogicalKeyboardKey.enter,
            '↵',
            'Run in terminal',
            () async {
              await platform.runInTerminal(await client.text(entry.id));
              await platform.close();
            },
            ctrl: true,
          ),
        ],
        .code => [
          ClipboardAction(
            LogicalKeyboardKey.enter,
            '↵',
            'Open in editor',
            () => _openCode(entry),
            ctrl: true,
          ),
        ],
        .image when entry.canPaste => [
          ClipboardAction(
            LogicalKeyboardKey.enter,
            '↵',
            'Save as…',
            () => _imageAction(entry, save: true),
            ctrl: true,
          ),
          ClipboardAction(
            LogicalKeyboardKey.keyE,
            'E',
            'Open in editor',
            () => _imageAction(entry, save: false),
            ctrl: true,
          ),
        ],
        .link => [
          ClipboardAction(
            LogicalKeyboardKey.enter,
            '↵',
            'Open in browser',
            () => _openLink(entry),
            ctrl: true,
          ),
          ClipboardAction(
            LogicalKeyboardKey.keyE,
            'E',
            'Paste as Markdown link',
            () => _pasteMarkdown(entry),
            ctrl: true,
          ),
        ],
        .files => [
          ClipboardAction(
            LogicalKeyboardKey.enter,
            '↵',
            'Paste paths as text',
            () => paste(entry, mode: 'paths'),
            ctrl: true,
          ),
          ClipboardAction(
            LogicalKeyboardKey.keyO,
            'O',
            'Reveal in file manager',
            () async {
              await platform.revealFiles((await client.detail(entry.id)).files);
              await platform.close();
            },
            ctrl: true,
          ),
        ],
        _ => const <ClipboardAction>[],
      },
      if (const {
        ClipboardKind.text,
        ClipboardKind.command,
        ClipboardKind.code,
      }.contains(entry.kind))
        ClipboardAction(
          LogicalKeyboardKey.keyE,
          'E',
          'Edit before pasting',
          () => _edit(entry),
          ctrl: true,
        ),
      for (final (index, format) in formats.indexed)
        ClipboardAction(
          const [
            LogicalKeyboardKey.digit1,
            LogicalKeyboardKey.digit2,
            LogicalKeyboardKey.digit3,
            LogicalKeyboardKey.digit4,
          ][index],
          '${index + 1}',
          'Paste in that format',
          () => pasteText(format),
          alt: true,
        ),
      if (entry.canPaste)
        ClipboardAction(
          LogicalKeyboardKey.keyC,
          'C',
          'Copy only',
          () => _copy(entry),
          ctrl: true,
        ),
      if (entry.kind != .hidden)
        ClipboardAction(
          LogicalKeyboardKey.keyD,
          'D',
          entry.pinned ? 'Unpin' : 'Pin',
          () => _pin(entry),
          ctrl: true,
        ),
      ClipboardAction(
        LogicalKeyboardKey.delete,
        'Del',
        entry.kind == .hidden ? 'Clear now' : 'Delete',
        () => _delete(entry),
        shift: true,
      ),
    ];
  }
}

class ClipboardAction {
  const ClipboardAction(
    this.key,
    this.keyLabel,
    this.label,
    this.run, {
    this.ctrl = false,
    this.shift = false,
    this.alt = false,
  });

  final LogicalKeyboardKey key;
  final String keyLabel;
  final String label;
  final Future<void> Function() run;
  final bool ctrl;
  final bool shift;
  final bool alt;

  LauncherKeyHint get hint => LauncherKeyHint([
    if (ctrl) 'Ctrl',
    if (shift) '⇧',
    if (alt) 'Alt',
    keyLabel,
  ], label: label);
}
