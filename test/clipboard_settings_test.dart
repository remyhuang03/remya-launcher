import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_launcher/app.dart';
import 'package:remya_launcher/features/clipboard/clipboard_client.dart';

class _Core extends SettingsClient {
  final signals = StreamController<SettingsChange>.broadcast();
  final owners = StreamController<bool>.broadcast();
  final writes = <List<String>>[];
  final values = <String, Object>{
    'clipboard-history': true,
    'clipboard-max-items': 500,
    'clipboard-retention-days': 30,
  };
  Object? failure = StateError('Unavailable: remyad is not running');

  @override
  Stream<SettingsChange> get changes => signals.stream;

  @override
  Stream<bool> get serviceAvailableChanges => owners.stream;

  @override
  Future<void> close() async {}

  @override
  Future<Map<String, dynamic>> call(
    String method, [
    List<String> arguments = const [],
  ]) async {
    if (failure case final failure?) throw failure;
    final id = arguments.first;
    final key = id.substring('remya-launcher.'.length);
    if (method == 'Set') {
      writes.add(arguments);
      values[key] = jsonDecode(arguments.last) as Object;
    }
    return {'id': id, 'availability': 'available', 'value': values[key]};
  }
}

class _Clipboard extends ClipboardClient {
  int clears = 0;
  bool available = false;

  @override
  Future<void> clear() async {
    if (!available) {
      throw const ClipboardServiceException(
        'Clipboard service is unavailable.',
      );
    }
    clears++;
  }
}

void main() {
  testWidgets('clipboard settings keep values and separate error sources', (
    tester,
  ) async {
    final core = _Core();
    final clipboard = _Clipboard();
    addTearDown(core.signals.close);
    addTearDown(core.owners.close);
    await tester.binding.setSurfaceSize(const Size(760, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      RemyaLauncherApp(
        settingsPage: 'clipboard',
        settingsClient: core,
        clipboardClient: clipboard,
      ),
    );
    await tester.pumpAndSettle();
    expect(
      find.text('Remya Core cannot provide the clipboard policy'),
      findsOne,
    );
    expect(find.byType(Switch), findsNothing);
    expect(find.text('Unavailable'), findsNWidgets(3));
    // The settings window never starts the clipboard overlay.
    expect(find.byType(TextFormField), findsNothing);

    core.failure = null;
    core.owners.add(true);
    await tester.pumpAndSettle();
    expect(
      find.text('Remya Core cannot provide the clipboard policy'),
      findsNothing,
    );

    await tester.tap(find.text('500 items'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), '6000');
    await tester.tap(find.text('Set limit'));
    await tester.pumpAndSettle();
    expect(find.text('Enter a whole number from 1 to 5000.'), findsOne);
    await tester.enterText(find.byType(TextFormField), '40');
    await tester.tap(find.text('Set limit'));
    await tester.pumpAndSettle();
    expect(core.writes.single, ['remya-launcher.clipboard-max-items', '40']);
    expect(find.text('40 items'), findsOne);

    // History off hides the limits; an external change shows them again.
    await tester.tap(find.byType(Switch));
    await tester.pumpAndSettle();
    expect(core.writes.last, ['remya-launcher.clipboard-history', 'false']);
    expect(find.text('40 items'), findsNothing);
    core.values['clipboard-history'] = true;
    core.signals.add((topic: 'remya-launcher.clipboard-history', value: {}));
    await tester.pumpAndSettle();
    expect(find.text('40 items'), findsOne);

    // Clipboard socket failures do not block policy edits.
    await tester.tap(find.text('Clear history'));
    await tester.pumpAndSettle();
    await tester.tap(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.text('Clear history'),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Clipboard history service error'), findsOne);
    expect(tester.widget<Switch>(find.byType(Switch)).onChanged, isNotNull);
    clipboard.available = true;
    await tester.tap(find.text('Clear history'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(clipboard.clears, 0);
    await tester.tap(find.text('Clear history'));
    await tester.pumpAndSettle();
    await tester.tap(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.text('Clear history'),
      ),
    );
    await tester.pumpAndSettle();
    expect(clipboard.clears, 1);
    expect(find.text('Clipboard history service error'), findsNothing);

    // Core failures and invalid replies keep the last valid policy read-only.
    core.values['clipboard-retention-days'] = 900;
    core.signals.add((
      topic: 'remya-launcher.clipboard-retention-days',
      value: {},
    ));
    await tester.pumpAndSettle();
    expect(find.textContaining('invalid clipboard-retention-days'), findsOne);
    expect(find.text('30 days'), findsOne);
    expect(tester.widget<Switch>(find.byType(Switch)).onChanged, isNull);
    await tester.pumpWidget(const SizedBox.shrink());
  });
}
