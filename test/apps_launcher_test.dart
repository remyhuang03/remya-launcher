import 'dart:async';
import 'dart:convert';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/app.dart';
import 'package:remya_launcher/features/apps/apps_client.dart';
import 'package:remya_launcher/features/apps/apps_icon.dart';
import 'package:remya_launcher/features/clipboard/clipboard_platform.dart';
import 'package:remya_launcher/features/launcher/launcher_controls.dart';
import 'package:remya_launcher/features/windows/windows_controller.dart';

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

const _windows = [
  {
    'id': '0xabc',
    'title': 'Draft.txt',
    'class': 'test-editor',
    'workspace': '3',
    'appId': _appId,
    'appName': 'Test Editor',
  },
  {'id': '0xdef', 'title': 'htop', 'class': 'kitty', 'workspace': '1'},
];

class _Apps extends AppsClient {
  final requests = <(String, bool)>[];
  final actions = <(String, String?, String?, String?, String?)>[];
  Completer<AppsSearch>? delayed;
  bool fail = false;
  bool hide = false;
  List<LauncherAppEntry>? matches;

  @override
  Future<AppsSearch> search(String query, {required bool packages}) async {
    requests.add((query, packages));
    if (query == 'slow') return delayed!.future;
    if (query == 'obsidian') {
      return AppsSearch(total: 1, packages: packages ? [_package] : []);
    }
    if (matches case final apps?) {
      return AppsSearch(total: apps.length, matched: apps.length, apps: apps);
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
  Future<List<LauncherWindow>> windows(String query) async => [
    for (final json in _windows)
      if ('${json['title']} ${json['class']}'.toLowerCase().contains(
        query.toLowerCase(),
      ))
        LauncherWindow.fromJson(json),
  ];

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

class _PendingWindows extends _Apps {
  final reads = <Completer<List<LauncherWindow>>>[];

  @override
  Future<List<LauncherWindow>> windows(String query) {
    final read = Completer<List<LauncherWindow>>();
    reads.add(read);
    return read.future;
  }
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
  testWidgets('Apps icon retries a failed native lookup after the miss TTL', (
    tester,
  ) async {
    const channel = MethodChannel('top.remya.launcher/window');
    final messenger = tester.binding.defaultBinaryMessenger;
    var requests = 0;
    messenger.setMockMethodCallHandler(channel, (call) async {
      expect(call.method, 'resolveAppIcon');
      if (++requests == 1) throw PlatformException(code: 'unavailable');
      return base64Decode(
        'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8'
        'AAwMCAO+aNp8AAAAASUVORK5CYII=',
      );
    });
    addTearDown(() => messenger.setMockMethodCallHandler(channel, null));

    Future<void> showIcon(String name) async {
      await tester.pumpWidget(
        MaterialApp(
          home: AppsIcon(id: name, name: name, icon: 'missing-for-retry-test'),
        ),
      );
      await tester.pump();
    }

    await showIcon('Alpha');
    expect(find.text('A'), findsOneWidget);
    await showIcon('Beta');
    expect(requests, 1);
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 2050)),
    );
    await showIcon('Gamma');
    expect(requests, 2);
    expect(find.byType(Image), findsOneWidget);
  });

  testWidgets('Windows polling waits and a late read cannot undo retry', (
    tester,
  ) async {
    final client = _PendingWindows();
    final controller = WindowsController(client: client, onClose: () async {});
    controller.update(active: true);
    await tester.pump(const Duration(milliseconds: 30));
    expect(client.reads, hasLength(1));
    await tester.pump(const Duration(seconds: 4));
    expect(client.reads, hasLength(1));

    final retry = controller.retry();
    final fresh = LauncherWindow.fromJson(_windows[1]);
    client.reads[1].complete([fresh]);
    await retry;
    client.reads[0].completeError(const AppsException('Stale'));
    await tester.pump();
    expect(controller.windows.single, fresh);
    expect(controller.error, isNull);
    controller.dispose();
  });

  testWidgets('The narrow search field fits the entire scope strip', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 600));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(RemyaLauncherApp(appsClient: _Apps()));
    await tester.pumpAndSettle();

    final chips = find.byType(LauncherScopeChip);
    expect(chips, findsNWidgets(8));
    final left = tester.getTopLeft(chips.first).dx;
    final right = tester.getTopRight(chips.last).dx;
    final search = tester.getRect(find.byType(LauncherSearchField));
    expect(left - search.left, closeTo(4, 1));
    expect(search.right - right, closeTo(4, 1));
  });

  testWidgets('All shows eight apps; Apps builds rows as they scroll in', (
    tester,
  ) async {
    final client = _Apps()
      ..matches = [
        for (var i = 1; i <= 60; i++)
          LauncherAppEntry.fromJson({
            'id': 'test.$i.desktop',
            'name': 'Test App $i',
          }),
      ];
    await tester.pumpWidget(RemyaLauncherApp(appsClient: client));
    await _enter(tester, 'test');
    expect(find.text('Test App 9'), findsNothing);
    final eighth = find.text('Test App 8');
    expect(eighth.hitTestable(), findsNothing);
    for (var i = 0; i < 7; i++) {
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pump();
    }
    expect(eighth.hitTestable(), findsOneWidget);

    await _shortcut(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.digit2,
    );
    await tester.pump(_search);
    await tester.pump();
    final last = find.text('Test App 60');
    expect(last, findsNothing);
    await tester.scrollUntilVisible(
      last,
      200,
      scrollable: find.descendant(
        of: find.byType(ListView),
        matching: find.byType(Scrollable),
      ),
    );
    expect(last.hitTestable(), findsOneWidget);
  });

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

  testWidgets('Right Arrow completes an app query and Ctrl+K opens actions', (
    tester,
  ) async {
    await tester.pumpWidget(RemyaLauncherApp(appsClient: _Apps()));
    await _enter(tester, 'Test');
    final field = tester.widget<TextFormField>(find.byType(TextFormField));
    field.controller!.selection = const TextSelection.collapsed(offset: 4);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
    await tester.pump(_search);
    expect(field.controller!.text, 'Test Editor');
    expect(find.text('type to filter'), findsNothing);
    await _shortcut(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.keyK,
    );
    expect(find.text('type to filter'), findsOneWidget);
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
      expect(find.text('Focus window'), findsNothing);
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
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(client.actions.last, ('open', _appId, null, null, null));
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

  testWidgets('# enters Windows; Enter focuses and Ctrl+Enter moves here', (
    tester,
  ) async {
    final client = _Apps();
    final platform = _Platform();
    await tester.pumpWidget(
      RemyaLauncherApp(appsClient: client, clipboardPlatform: platform),
    );
    await tester.pump();
    await _enter(tester, '#');
    final field = tester.widget<TextFormField>(find.byType(TextFormField));
    expect(field.controller!.text, isEmpty);
    expect(find.text('2 windows'), findsOneWidget);
    expect(find.text('Workspace 3'), findsOneWidget);

    await _enter(tester, 'kitty');
    expect(find.text('Draft.txt'), findsNothing);
    expect(find.text('htop'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pump();
    expect(client.actions.last, ('window', null, '0xdef', null, null));
    expect(platform.closes, 1);

    await _enter(tester, '');
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
    await _shortcut(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.enter,
    );
    expect(client.actions.last, ('windowHere', _appId, '0xabc', null, null));
    expect(platform.closes, 2);

    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pump();
    expect(find.text('2 windows'), findsNothing);
    await _shortcut(
      tester,
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.digit7,
    );
    await tester.pump(_search);
    expect(find.text('2 windows'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });
}
