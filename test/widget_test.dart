import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/app.dart';
import 'package:remya_launcher/features/clipboard/clipboard_preview.dart';
import 'package:remya_launcher/features/launcher/launcher_controls.dart';
import 'package:remya_launcher/features/launcher/launcher_details.dart';
import 'package:remya_launcher/features/launcher/launcher_panel.dart';
import 'package:remya_launcher/features/launcher/launcher_results.dart';
import 'package:remya_launcher/features/preview/launcher_examples.dart';

void main() {
  for (final example in LauncherExample.values) {
    testWidgets('${example.name} fits the desktop preview', (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 820));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(RemyaLauncherApp(example: example));
      await tester.pump();

      expect(find.byType(LauncherSearchField), findsOneWidget);
      expect(find.byType(LauncherScopeChip), findsNWidgets(8));
      expect(find.byType(LauncherPanel), findsOneWidget);
      expect(tester.takeException(), isNull);
      switch (example) {
        case .empty:
          expect(find.byType(LauncherPinnedTile), findsNWidgets(6));
        case .results:
        case .quickPick:
          expect(find.byType(LauncherResultRow), findsNWidgets(6));
        case .calculator:
          expect(find.byType(LauncherAnswerCard), findsOneWidget);
        case .files:
          expect(find.byType(LauncherFilePreview), findsOneWidget);
        case .web:
          expect(find.byType(LauncherResultRow), findsNWidgets(4));
        case .actions:
          expect(find.byType(LauncherActionsMenu), findsOneWidget);
        case .clipboard:
          expect(find.byType(ClipboardPreview), findsOneWidget);
      }
    });
  }

  for (final type in [
    'text',
    'code',
    'image',
    'link',
    'color',
    'files',
    'hidden',
  ]) {
    testWidgets('clipboard renders the $type detail in a narrow window', (
      tester,
    ) async {
      await tester.binding.setSurfaceSize(const Size(400, 520));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(
        RemyaLauncherApp(example: .clipboard, clipboardType: type),
      );
      await tester.pump();

      expect(find.byType(ClipboardPreview), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('search accepts text and emits UI callbacks', (tester) async {
    String? query;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: LauncherSearchField(onChanged: (text) => query = text),
        ),
      ),
    );

    await tester.enterText(find.byType(TextFormField), 'code');
    expect(query, 'code');
  });
}
