import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/features/clipboard/clipboard_client.dart';
import 'package:remya_launcher/features/clipboard/clipboard_launcher.dart';
import 'package:remya_launcher/features/clipboard/clipboard_platform.dart';
import 'package:remya_launcher/features/files/files_client.dart';
import 'package:remya_launcher/features/files/files_preferences.dart';
import 'package:remya_launcher/features/files/files_query.dart';

const _folder = {
  'id': 'folder',
  'name': 'Work',
  'path': '/home/test/Work',
  'displayPath': '~/Work',
  'parent': '~',
  'completion': '/home/test/Work/',
  'kind': 'directory',
  'group': 'results',
  'version': 'folder-version',
};

const _file = {
  'id': 'file',
  'name': 'todo.md',
  'path': '/home/test/Work/todo.md',
  'displayPath': '~/Work/todo.md',
  'parent': '~/Work',
  'completion': '/home/test/Work/todo.md',
  'kind': 'file',
  'group': 'results',
  'version': 'file-version',
};

class _Files extends FilesClient {
  final calls = <(String, Map<String, Object?>)>[];

  @override
  Future<Map<String, dynamic>> request(
    String op, [
    Map<String, Object?> arguments = const {},
    void Function(Map<String, dynamic>)? onPartial,
  ]) async {
    calls.add((op, {...arguments}));
    if (op == 'action') {
      return switch (arguments['action']) {
        'applications' => {
          'applications': [
            {'id': 'editor.desktop', 'name': 'Test Editor'},
          ],
          'editable': true,
        },
        'createDirectory' => {
          'path': '/home/test/Work/new',
          'entryId': 'new',
          'undoId': 'undo',
        },
        'undo' => {},
        _ => {'dispatched': true},
      };
    }
    if (op != 'search') return {};
    final query = arguments['query'] as String;
    return switch ((arguments['mode'], query)) {
      ('contents', '') => {'mode': 'contents', 'entries': []},
      ('contents', _) => {
        'mode': 'contents',
        'entries': [
          {
            ..._file,
            'hitCount': 4,
            'hits': [
              for (var line = 41; line <= 44; line++)
                {'line': line, 'snippet': 'todo $line'},
            ],
          },
        ],
      },
      (_, '~/Wo') => {
        'mode': 'path',
        'entries': [_folder],
        'createName': 'Wo',
        'createRelative': 'Wo',
        'createPath': '/home/test/Wo',
      },
      (_, '~/Work/') => {
        'mode': 'path',
        'directoryId': 'folder',
        'entries': [_file],
      },
      (_, '~/Work/new/') => {
        'mode': 'path',
        'entries': [],
        'directoryId': 'folder',
        'directoryVersion': 'folder-version',
        'createName': 'new',
        'createRelative': 'new',
        'createPath': '/home/test/Work/new',
        'createDirectoryOnly': true,
      },
      (_, final text) when text.startsWith('~/') || text.startsWith('/') => {
        'mode': 'path',
        'entries': [_file],
      },
      (_, '') => {'mode': 'empty', 'entries': []},
      _ => {
        'mode': 'names',
        'entries': [_file],
      },
    };
  }
}

class _Preferences extends FilesPreferences {
  @override
  Stream<Map<String, dynamic>> watch(String id) => const Stream.empty();

  @override
  Future<Map<String, Map<String, dynamic>>> read() async => {
    for (final entry in FilesPreferences.defaults.entries)
      entry.key: {...entry.value},
  };
}

class _Platform extends ClipboardPlatform {
  int closes = 0;

  @override
  Future<void> close() async => closes++;
}

Finder get _search => find.byType(TextFormField).first;
String _query(WidgetTester tester) =>
    tester.widget<TextFormField>(_search).controller!.text;

Future<void> _render(
  WidgetTester tester,
  _Files files,
  _Platform platform,
) async {
  await tester.binding.setSurfaceSize(const Size(1000, 780));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: ClipboardLauncher(
          startInClipboard: false,
          client: const ClipboardClient(),
          platform: platform,
          filesClient: files,
          filesPreferences: _Preferences(),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

Future<void> _ctrl(WidgetTester tester, LogicalKeyboardKey key) async {
  await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
  try {
    await tester.sendKeyEvent(key);
  } finally {
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  }
  await tester.pumpAndSettle();
}

Future<void> _type(WidgetTester tester, String value) async {
  await tester.enterText(_search, value);
  await tester.pump(const Duration(milliseconds: 220));
  await tester.pumpAndSettle();
}

void main() {
  test(
    'filter tokens leave exact path whitespace and invalid filters editable',
    () {
      expect(
        filesCommitFilters('~/folder with  two spaces/ ext:md mod:7d '),
        isA<(String, List<String>)>()
            .having((value) => value.$1, 'query', '~/folder with  two spaces/')
            .having((value) => value.$2, 'filters', ['ext:md', 'mod:7d']),
      );
      expect(
        filesCommitFilters('size:10M mod:>2026-02-31 '),
        isA<(String, List<String>)>()
            .having((value) => value.$1, 'query', 'size:10M mod:>2026-02-31 ')
            .having((value) => value.$2, 'filters', isEmpty),
      );
    },
  );

  testWidgets('Tab commits the focused filter candidate', (tester) async {
    final files = _Files();
    await _render(tester, files, _Platform());
    await _ctrl(tester, LogicalKeyboardKey.digit3);
    await _type(tester, 'type:');
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pumpAndSettle();
    final args = files.calls.lastWhere((call) => call.$1 == 'search').$2;
    expect(args['filters'], ['type:dir']);
  });

  testWidgets('Ctrl+3 keeps the query and filters travel as separate tokens', (
    tester,
  ) async {
    final files = _Files();
    await _render(tester, files, _Platform());
    await _type(tester, 'todo');
    expect(find.text('FILES'), findsOneWidget);
    await _ctrl(tester, LogicalKeyboardKey.digit3);
    expect(_query(tester), 'todo');
    await _type(tester, 'todo ext:md ');
    expect(_query(tester).trim(), 'todo');
    final args = files.calls.lastWhere((call) => call.$1 == 'search').$2;
    expect(args['filters'], ['ext:md']);
    expect((args['query'] as String).trim(), 'todo');
  });

  testWidgets('path prefix, Tab, create and Undo use verified sidecar IDs', (
    tester,
  ) async {
    final files = _Files();
    await _render(tester, files, _Platform());
    await _type(tester, '~/Wo');
    expect(find.text('Tab'), findsWidgets);
    await tester.sendKeyEvent(LogicalKeyboardKey.tab);
    await tester.pumpAndSettle();
    expect(_query(tester), '~/Work/');
    await _type(tester, '~/Work/new/');
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    expect(_query(tester), '/home/test/Work/new/');
    final create = files.calls
        .lastWhere((call) => call.$2['action'] == 'createDirectory')
        .$2;
    expect(create['entryId'], 'folder');
    expect(create['version'], 'folder-version');
    expect(create['name'], 'new');
    await _ctrl(tester, LogicalKeyboardKey.keyZ);
    expect(
      files.calls.lastWhere((call) => call.$2['action'] == 'undo').$2['undoId'],
      'undo',
    );
  });

  testWidgets(
    'contents exposes file and hit rows and opens a hit at its line',
    (tester) async {
      final files = _Files();
      final platform = _Platform();
      await _render(tester, files, platform);
      await _ctrl(tester, LogicalKeyboardKey.digit3);
      await _type(tester, 'rg todo');
      expect(_query(tester), 'todo');
      expect(find.text('+1 more'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pumpAndSettle();
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();
      final action = files.calls.lastWhere((call) => call.$1 == 'action').$2;
      expect(action['action'], 'editor');
      expect(action['entryId'], 'file');
      expect(action['line'], 41);
      expect(action['operationId'], isA<String>());
      expect(platform.closes, 1);
    },
  );
}
