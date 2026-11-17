import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/app.dart';
import 'package:remya_launcher/features/clipboard/clipboard_client.dart';
import 'package:remya_launcher/features/clipboard/clipboard_launcher.dart';
import 'package:remya_launcher/features/clipboard/clipboard_platform.dart';
import 'package:remya_launcher/features/emoji/emoji_results.dart';
import 'package:remya_launcher/features/emoji/emoji_search.dart';

final _field = find.byType(TextFormField);

class _Platform extends ClipboardPlatform {
  const _Platform();

  @override
  Future<void> close() async {}
}

class _Clipboard extends ClipboardClient {
  bool available = false;
  final payloads = <String>[];

  @override
  Future<void> publishText(String text) async {
    if (!available) {
      throw const ClipboardServiceException('Clipboard service is unavailable');
    }
    payloads.add(text);
  }
}

Future<void> _render(
  WidgetTester tester,
  Future<EmojiCatalog> Function() load, {
  ClipboardClient client = const ClipboardClient(),
}) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: ClipboardLauncher(
          startInClipboard: false,
          client: client,
          platform: const _Platform(),
          loadEmojiCatalog: load,
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _waitForEmoji(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 75));
  await tester.pumpAndSettle();
}

Future<void> _ctrl(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  try {
    await tester.sendKeyEvent(key);
  } finally {
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late EmojiCatalog catalog;
  setUpAll(() async => catalog = await EmojiCatalog.load());

  testWidgets('A composing prefix is left intact until the IME commits', (
    tester,
  ) async {
    await _render(tester, () async => catalog);
    await tester.showKeyboard(_field);
    const composing = TextEditingValue(
      text: ':猫',
      selection: TextSelection.collapsed(offset: 2),
      composing: TextRange(start: 0, end: 2),
    );
    tester.testTextInput.updateEditingValue(composing);
    await tester.pump();
    expect(find.byType(EmojiResults), findsNothing);
    expect(tester.widget<TextFormField>(_field).controller!.text, ':猫');

    tester.testTextInput.updateEditingValue(
      composing.copyWith(composing: TextRange.empty),
    );
    await _waitForEmoji(tester);
    expect(tester.widget<TextFormField>(_field).controller!.text, '猫');
    expect(
      tester.widget<EmojiResults>(find.byType(EmojiResults)).results,
      contains(predicate<EmojiResult>((item) => item.emoji == '🐱')),
    );

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();
    expect(tester.widget<TextFormField>(_field).controller!.text, isEmpty);
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.byType(EmojiResults), findsNothing);
  });

  testWidgets('Rapid queries copy only the last selected complete sequence', (
    tester,
  ) async {
    final load = Completer<EmojiCatalog>();
    final clipboard = _Clipboard()..available = true;
    await _render(tester, () => load.future, client: clipboard);
    final payloads = <String>[];
    var failPlatformCopy = false;
    final messenger = tester.binding.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') {
        if (failPlatformCopy) throw PlatformException(code: 'unavailable');
        payloads.add((call.arguments as Map)['text'] as String);
      }
      return null;
    });
    addTearDown(
      () => messenger.setMockMethodCallHandler(SystemChannels.platform, null),
    );

    await tester.enterText(_field, ':cat');
    expect(tester.widget<TextFormField>(_field).controller!.text, 'cat');
    await tester.pump(const Duration(milliseconds: 75));
    await tester.enterText(_field, ':joy:');
    await tester.enterText(_field, '👩‍💻');
    load.complete(catalog);
    await _waitForEmoji(tester);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(clipboard.payloads, ['👩‍💻']);
    expect(payloads, isEmpty);
    expect(find.text('Copied. Paste it in the target window.'), findsOneWidget);

    await tester.enterText(_field, '🫹');
    await _waitForEmoji(tester);
    await _ctrl(tester, LogicalKeyboardKey.arrowRight);
    await tester.pumpAndSettle();
    expect(find.text('Emoji variants'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.text('Emoji variants'), findsNothing);
    expect(tester.widget<TextFormField>(_field).controller!.text, '🫹');

    await _ctrl(tester, LogicalKeyboardKey.arrowRight);
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pumpAndSettle();
    clipboard.available = false;
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(payloads.last, '🫹🏻');

    failPlatformCopy = true;
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(find.text('Could not copy the emoji.'), findsOneWidget);
    expect(payloads, ['🫹🏻']);

    await _ctrl(tester, LogicalKeyboardKey.tab);
    await tester.pump();
    expect(find.byType(EmojiResults), findsNothing);
  });

  testWidgets('Uses Traditional Chinese on a zh-TW desktop', (tester) async {
    final dispatcher = tester.binding.platformDispatcher;
    dispatcher.localesTestValue = const [Locale('zh', 'TW')];
    addTearDown(dispatcher.clearLocalesTestValue);
    await tester.pumpWidget(
      const RemyaLauncherApp(clipboardPlatform: _Platform()),
    );
    await tester.pumpAndSettle();
    await tester.enterText(_field, ':');
    await tester.pump();
    expect(find.text('按名稱、情緒或短碼搜尋表情…'), findsOneWidget);
  });
}
