import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/app.dart';
import 'package:remya_launcher/features/apps/apps_client.dart';
import 'package:remya_launcher/features/clipboard/clipboard_platform.dart';

const _search = Duration(milliseconds: 80);
const _appId = 'test.editor.desktop';

final _editor = LauncherAppEntry.fromJson({
  'id': _appId,
  'name': 'Test Editor',
  'description': 'Edit files',
  'windows': [
    {'id': '0xabc', 'title': 'Draft.txt', 'workspace': '3'},
  ],
  'actions': [
    {'id': 'new-window', 'name': 'New Window'},
    {'id': 'new-private-window', 'name': 'New Private Window'},
  ],
});
final _package = AppPackage.fromJson({
  'id': 'obsidian',
  'name': 'Obsidian',
  'description': '1.8.10 · Markdown knowledge base',
  'source': 'extra',
  'command': 'sudo pacman -S obsidian',
});

class _Apps extends AppsClient {
  final requests = <(String, bool)>[];
  final actions = <(String, String?, String?, String?, String?)>[];
  Completer<AppsSearch>? delayed;
  bool fail = false;
  bool hide = false;

  @override
  Future<AppsSearch> search(String query, {required bool packages}) async {
    requests.add((query, packages));
    if (query == 'slow') return delayed!.future;
    if (query == 'obsidian') {
      return AppsSearch(total: 1, packages: packages ? [_package] : []);
    }
    return AppsSearch(
      total: hide ? 0 : 1,
      matched: 1,
      apps: [
        if (hide)
          LauncherAppEntry.fromJson({
            'id': _appId,
            'name': 'Test Editor',
            'hidden': true,
          })
        else
          _editor,
      ],
      frequent: query.isEmpty && !hide ? [_editor] : [],
      hidden: hide,
    );
  }

  @override
  Future<String?> perform(
    String action, {
    String? appId,
    String query = '',
    String? target,
    String? packageId,
    String? source,
  }) async {
    actions.add((action, appId, target, packageId, source));
    if (fail) throw const AppsException('The executable was not found.');
    return null;
  }
}

class _Platform extends ClipboardPlatform {
  int closes = 0;

  @override
  Future<void> close() async => closes++;
}

Future<void> _shortcut(
  WidgetTester tester,
  LogicalKeyboardKey modifier,
  LogicalKeyboardKey key,
) async {
  await tester.sendKeyDownEvent(modifier);
  await tester.sendKeyEvent(key);
  await tester.sendKeyUpEvent(modifier);
  await tester.pump();
}

Future<void> _enter(WidgetTester tester, String query) async {
  await tester.enterText(find.byType(TextFormField).first, query);
  await tester.pump(_search);
  await tester.pump();
}

void main() {
  testWidgets('All retains its query in Apps; stale responses are ignored', (
    tester,
  ) async {
    final client = _Apps()..delayed = Completer<AppsSearch>();
    await tester.pumpWidget(RemyaLauncherApp(appsClient: client));
    await tester.pump();
    await _enter(tester, 'slow');
    await _enter(tester, 'editor');
    expect(find.text('APPS'), findsOneWidget);
    await _shortcut(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.digit2,
    );
    await tester.pump(_search);
    await tester.pump();
    expect(find.text('editor'), findsOneWidget);
    expect(find.text('1 of 1'), findsOneWidget);
    expect(
      client.requests,
      containsAllInOrder([
        ('slow', false),
        ('editor', false),
        ('editor', true),
      ]),
    );

    client.delayed!.complete(const AppsSearch());
    await tester.pump();
    expect(find.text('1 of 1'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'expands and collapses, filters actions, and shows launch errors inline',
    (tester) async {
      final client = _Apps();
      final platform = _Platform();
      await tester.pumpWidget(
        RemyaLauncherApp(appsClient: client, clipboardPlatform: platform),
      );
      await tester.pump();
      await _shortcut(
        tester,
        LogicalKeyboardKey.controlLeft,
        LogicalKeyboardKey.digit2,
      );
      await _enter(tester, 'Test');
      final field = tester.widget<TextFormField>(find.byType(TextFormField));
      field.controller!.selection = const TextSelection.collapsed(offset: 4);
      await _shortcut(
        tester,
        LogicalKeyboardKey.controlLeft,
        LogicalKeyboardKey.arrowRight,
      );
      expect(find.text('OPEN WINDOWS'), findsOneWidget);
      expect(find.text('Draft.txt'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pump();
      expect(find.text('Draft.txt'), findsNothing);
      expect(field.controller!.text, 'Test');

      await _shortcut(
        tester,
        LogicalKeyboardKey.controlLeft,
        LogicalKeyboardKey.keyK,
      );
      expect(find.text('type to filter'), findsOneWidget);
      await tester.enterText(find.byType(TextField).last, 'private');
      await tester.pump();
      expect(find.text('New Private Window'), findsOneWidget);
      expect(find.text('Copy launch command'), findsNothing);

      client.fail = true;
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(client.actions, [
        ('desktopAction', _appId, 'new-private-window', null, null),
      ]);
      expect(platform.closes, 0);
      expect(find.text('The executable was not found.'), findsOneWidget);

      client.fail = false;
      await _shortcut(
        tester,
        LogicalKeyboardKey.controlLeft,
        LogicalKeyboardKey.enter,
      );
      expect(client.actions.last.$1, 'new');
      expect(platform.closes, 1);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets('shows offline install and web; hidden matches take precedence', (
    tester,
  ) async {
    final client = _Apps();
    await tester.pumpWidget(
      RemyaLauncherApp(appsClient: client, clipboardPlatform: _Platform()),
    );
    await tester.pump();
    await _shortcut(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.digit2,
    );
    await _enter(tester, 'obsidian');
    expect(find.text('INSTALL'), findsOneWidget);
    expect(find.text('ELSEWHERE'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(client.actions.last, ('install', null, null, 'obsidian', 'extra'));
    await _shortcut(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.enter,
    );
    expect(client.actions.last.$1, 'web');

    client.hide = true;
    await _enter(tester, 'editor');
    expect(find.text('HIDDEN'), findsOneWidget);
    expect(find.text('INSTALL'), findsNothing);
    await _shortcut(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.keyH,
    );
    expect(client.actions.last.$1, 'unhide');
    await tester.pumpWidget(const SizedBox());
  });
}
