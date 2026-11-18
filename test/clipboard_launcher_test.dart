import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/app.dart';
import 'package:remya_launcher/features/clipboard/clipboard_client.dart';
import 'package:remya_launcher/features/clipboard/clipboard_color.dart';
import 'package:remya_launcher/features/clipboard/clipboard_platform.dart';
import 'package:remya_launcher/features/files/file_icon_tile.dart';
import 'package:remya_launcher/features/launcher/launcher_panel.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test(
    'save as replaces the chosen file without leaving staging files',
    () async {
      final folder = await Directory.systemTemp.createTemp(
        'clipboard-save-test-',
      );
      addTearDown(() => folder.delete(recursive: true));
      final destination = await File('${folder.path}/clipboard.png')
          .writeAsBytes([0]);
      const channel = MethodChannel('top.remya.launcher/window');
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      messenger.setMockMethodCallHandler(channel, (call) async {
        expect(call.method, 'saveFile');
        expect(call.arguments, 'clipboard.png');
        return destination.path;
      });
      addTearDown(() => messenger.setMockMethodCallHandler(channel, null));

      final image = Uint8List.fromList([137, 80, 78, 71]);
      await const ClipboardPlatform().saveImage('image/png', image);

      expect(await destination.readAsBytes(), image);
      expect(await folder.list().map((file) => file.path).toList(), [
        destination.path,
      ]);
    },
  );

  testWidgets('clipboard starts on demand and supports filters and actions', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(400, 520));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final clipboard = _TestClipboard([
      _entry('link', 'link', 'https://example.com', age: 2),
      _entry('text', 'text', 'A note', age: 1),
    ]);
    final platform = _TestPlatform();
    await tester.pumpWidget(
      RemyaLauncherApp(clipboardClient: clipboard, clipboardPlatform: platform),
    );
    expect(clipboard.requests, isEmpty);
    await tester.enterText(find.byType(TextFormField), ';');
    await tester.pumpAndSettle();
    expect(find.text('A note'), findsWidgets);
    expect(find.text('https://example.com'), findsOneWidget);
    expect(tester.takeException(), isNull);

    await _modifiedKey(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.keyF,
    );
    await tester.pumpAndSettle();
    expect(find.text('https://example.com'), findsNothing);
    await _modifiedKey(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.keyC,
    );
    await tester.pumpAndSettle();
    expect(clipboard.requests.last, {
      'op': 'publish',
      'id': 'text',
      'mode': 'original',
    });
    expect(platform.pasteCalls, 0);
    expect(find.text('Copied to the clipboard.'), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(platform.pasteCalls, 1);
    expect(find.text('Copied. Paste it in the target window.'), findsOneWidget);

    await _modifiedKey(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.keyD,
    );
    await tester.pumpAndSettle();
    expect(find.text('PINNED'), findsOneWidget);

    await _modifiedKey(
      tester,
      LogicalKeyboardKey.shiftLeft,
      LogicalKeyboardKey.delete,
    );
    await tester.pumpAndSettle();
    expect(find.text('A note'), findsNothing);
    expect(find.text('No matching clipboard entries.'), findsOneWidget);
  });

  testWidgets('clipboard clear requires confirmation', (tester) async {
    final clipboard = _TestClipboard([_entry('text', 'text', 'Keep me')]);
    await tester.pumpWidget(RemyaLauncherApp(clipboardClient: clipboard));
    await tester.enterText(find.byType(TextFormField), '>');
    await tester.pumpAndSettle();
    final search = find.byType(TextFormField);
    await tester.enterText(search, 'echo example');
    await tester.pumpAndSettle();
    expect(find.text('No matching actions.'), findsOneWidget);
    await tester.enterText(search, '');
    await tester.pumpAndSettle();
    await tester.tap(find.text('Clear clipboard history'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(
      clipboard.requests.any((request) => request['op'] == 'clear'),
      isFalse,
    );

    await tester.tap(find.text('Clear clipboard history'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Clear history'));
    await tester.pumpAndSettle();
    expect(
      clipboard.requests.where((request) => request['op'] == 'clear'),
      hasLength(1),
    );
    expect(find.text('Clipboard history cleared.'), findsOneWidget);
  });

  testWidgets('compact overlay leaves space for results and preview', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(246, 380));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final clipboard = _TestClipboard([_entry('text', 'text', 'Compact note')]);
    await tester.pumpWidget(
      RemyaLauncherApp(startInClipboard: true, clipboardClient: clipboard),
    );
    await tester.pumpAndSettle();
    expect(find.byTooltip('Paste').hitTestable(), findsOneWidget);
    final note = find.text('Compact note').hitTestable();
    expect(note, findsNWidgets(2));
    expect(find.byType(TextFormField).hitTestable(), findsOneWidget);
  });

  testWidgets('clipboard searches preview text and keeps keyboard selection', (
    tester,
  ) async {
    final clipboard = _TestClipboard([
      _entry('first', 'text', 'First', age: 1),
      {
        ..._entry('second', 'text', 'Second', age: 2),
        'searchable': 'needle in the second line',
      },
    ]);
    await tester.pumpWidget(RemyaLauncherApp(clipboardClient: clipboard));
    await _modifiedKey(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.digit6,
    );
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pumpAndSettle();
    expect(clipboard.requests.last, {'op': 'detail', 'id': 'second'});

    await tester.enterText(find.byType(TextFormField), 'needle');
    await tester.pump(const Duration(milliseconds: 80));
    await tester.pumpAndSettle();
    expect(find.text('First'), findsNothing);
    expect(find.text('Second'), findsWidgets);
    expect(
      clipboard.requests.any((request) => request['query'] == 'needle'),
      isTrue,
    );
  });

  testWidgets('hidden clipboard data is only shown while keys are held', (
    tester,
  ) async {
    final clipboard = _TestClipboard([
      _entry('hidden', 'hidden', 'Hidden', expires: true),
    ]);
    await tester.pumpWidget(
      RemyaLauncherApp(startInClipboard: true, clipboardClient: clipboard),
    );
    await tester.pumpAndSettle();
    expect(find.text('test secret'), findsNothing);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.space);
    await tester.pump();
    expect(find.text('test secret'), findsOneWidget);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.space);
    await tester.pump();
    expect(find.text('test secret'), findsNothing);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  });

  testWidgets('unavailable clipboard shows a retry action', (tester) async {
    final clipboard = _TestClipboard([]);
    clipboard.available = false;
    await tester.pumpWidget(
      RemyaLauncherApp(startInClipboard: true, clipboardClient: clipboard),
    );
    await tester.pumpAndSettle();
    expect(find.text('Clipboard service is unavailable.'), findsOneWidget);
    clipboard.available = true;
    await tester.tap(find.text('Retry'));
    await tester.pumpAndSettle();
    expect(find.text('Clipboard history is empty.'), findsOneWidget);
  });

  testWidgets('clipboard listens again after the service returns', (
    tester,
  ) async {
    final clipboard = _TestClipboard([
      _entry('old', 'text', 'Original', age: 1),
    ]);
    await tester.pumpWidget(
      RemyaLauncherApp(startInClipboard: true, clipboardClient: clipboard),
    );
    await tester.pumpAndSettle();
    expect(find.text('Original'), findsWidgets);
    await clipboard.changes!.close();
    await tester.pump();
    expect(find.text('Clipboard service is unavailable.'), findsOneWidget);

    clipboard.entries.add(_entry('new', 'text', 'Reconnected', age: 1));
    await tester.pump(const Duration(seconds: 1));
    await tester.pumpAndSettle();
    expect(clipboard.watchCalls, 2);
    expect(find.text('Reconnected'), findsWidgets);

    clipboard.entries.add(_entry('latest', 'text', 'New signal', age: 1));
    clipboard.changes!.add(null);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 50));
    await tester.pumpAndSettle();
    expect(find.text('New signal'), findsWidgets);
  });

  test('clipboard converts colors without losing alpha', () {
    final color = clipboardColor('#FF000080')!;
    final formats = clipboardColorFormats('#FF000080', color);
    expect(formats, [
      '#FF000080',
      'rgb(255 0 0 / 0.502)',
      'hsl(0 100% 50% / 0.502)',
      'oklch(0.628 0.258 29.234 / 0.502)',
    ]);
  });

  testWidgets('launcher panel stays fixed at 70px from top', (tester) async {
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final clipboard = _TestClipboard([_entry('text', 'text', 'Note')]);

    for (final size in const [
      Size(1280, 720),
      Size(1600, 900),
      Size(1920, 1080),
      Size(2560, 1440),
    ]) {
      await tester.binding.setSurfaceSize(size);
      await tester.pumpWidget(RemyaLauncherApp(clipboardClient: clipboard));
      await tester.pumpAndSettle();
      expect(tester.getTopLeft(find.byType(LauncherPanel)).dy, 70.0);
    }
  });

  testWidgets('clipboard file entries render Material Icon Theme tiles', (
    tester,
  ) async {
    await FileIconAssets.ensureLoaded();
    await tester.binding.setSurfaceSize(const Size(1000, 720));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final clipboard = _TestClipboard(
      [
        {..._entry('file-1', 'files', 'demo.mp4'), 'summary': '1 file'},
      ],
      filesById: {
        'file-1': ['/home/remy/demo.mp4'],
      },
    );
    await tester.pumpWidget(
      RemyaLauncherApp(startInClipboard: true, clipboardClient: clipboard),
    );
    await tester.pumpAndSettle();

    expect(find.byType(FileIconTile), findsNWidgets(2));
    final videoSvgs = tester
        .widgetList<SvgPicture>(find.byType(SvgPicture))
        .where(
          (svg) =>
              svg.bytesLoader is SvgAssetLoader &&
              (svg.bytesLoader as SvgAssetLoader).assetName ==
                  'assets/material_icon_theme/icons/video.svg',
        );
    expect(videoSvgs, hasLength(2));
  });
}

Future<void> _modifiedKey(
  WidgetTester tester,
  LogicalKeyboardKey modifier,
  LogicalKeyboardKey key,
) async {
  await tester.sendKeyDownEvent(modifier);
  await tester.sendKeyEvent(key);
  await tester.sendKeyUpEvent(modifier);
}

Map<String, dynamic> _entry(
  String id,
  String kind,
  String title, {
  int age = 0,
  bool expires = false,
}) {
  final now = DateTime.now().millisecondsSinceEpoch;
  return {
    'id': id,
    'kind': kind,
    'title': title,
    'summary': kind,
    'copiedAt': now - age,
    'timesCopied': 1,
    'pinned': false,
    'mimeTypes': ['text/plain'],
    'thumbnailOnly': false,
    if (expires) 'expiresAt': now + 45000,
  };
}

class _TestClipboard extends ClipboardClient {
  _TestClipboard(this.entries, {this.filesById = const {}});

  final List<Map<String, dynamic>> entries;
  final Map<String, List<String>> filesById;
  final requests = <Map<String, Object?>>[];
  bool available = true;
  int watchCalls = 0;
  StreamController<void>? changes;

  @override
  Future<bool> isEnabled() async => true;

  @override
  Stream<void> watch() {
    watchCalls++;
    return available
        ? (changes = StreamController<void>()).stream
        : Stream<void>.error(const ClipboardServiceException('Unavailable'));
  }

  @override
  Future<Object?> request(
    String op, [
    Map<String, Object?> arguments = const {},
  ]) async {
    requests.add({'op': op, ...arguments});
    if (!available) throw const ClipboardServiceException('Unavailable');
    switch (op) {
      case 'list':
        final query = arguments['query'] as String? ?? '';
        return entries
            .where(
              (entry) => '${entry['title']} ${entry['searchable'] ?? ''}'
                  .contains(query),
            )
            .toList();
      case 'detail':
        final id = arguments['id'] as String;
        return {
          'text': entries.firstWhere((entry) => entry['id'] == id)['title'],
          if (filesById.containsKey(id)) 'files': filesById[id],
        };
      case 'reveal':
        return 'test secret';
      case 'setPinned':
        entries.firstWhere(
          (entry) => entry['id'] == arguments['id'],
        )['pinned'] = arguments['pinned'];
      case 'delete':
        entries.removeWhere((entry) => entry['id'] == arguments['id']);
      case 'clear':
        entries.clear();
      case 'publish':
        return null;
      default:
        throw StateError('Unexpected request $op');
    }
    return null;
  }
}

class _TestPlatform extends ClipboardPlatform {
  int pasteCalls = 0;

  @override
  Future<bool> pasteToPreviousWindow() async {
    pasteCalls++;
    return false;
  }
}
