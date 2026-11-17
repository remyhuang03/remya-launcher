import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/features/emoji/emoji_results.dart';
import 'package:remya_launcher/features/emoji/emoji_search.dart';

void main() {
  testWidgets('emoji selection scrolls into view with large text', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(420, 220);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);

    final selection = ValueNotifier(0);
    addTearDown(selection.dispose);
    final results = List.generate(
      12,
      (index) => EmojiResult(
        emoji: String.fromCharCode(0x1F600 + index),
        name: 'Face ${index + 1}',
        group: 'Smileys',
        codePoints: 'U+${(0x1F600 + index).toRadixString(16).toUpperCase()}',
      ),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: MediaQuery(
            data: const MediaQueryData(textScaler: TextScaler.linear(2.5)),
            child: ValueListenableBuilder(
              valueListenable: selection,
              builder: (context, index, child) => EmojiResults(
                results: results,
                selectedIndex: index,
                onCopy: (_) {},
                onVariants: (_) {},
                onSelected: (index) => selection.value = index,
                hasVariants: (_) => true,
              ),
            ),
          ),
        ),
      ),
    );
    expect(tester.takeException(), isNull);

    selection.value = results.length - 1;
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    final label = find.text('Face 12');
    expect(label, findsOneWidget);
    expect(tester.getCenter(label).dy, inInclusiveRange(0, 220));
  });
}
