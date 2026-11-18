import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_launcher/features/clipboard/clipboard_client.dart';
import 'package:remya_launcher/features/clipboard/clipboard_settings.dart';
import 'package:remya_launcher/features/files/files_client.dart';
import 'package:remya_launcher/features/files/files_preferences.dart';
import 'package:remya_launcher/features/files/files_settings.dart';
import 'package:remya_ui/settings_ui.dart';

class _MockCore extends SettingsClient {
  _MockCore({bool available = true}) {
    for (final entry in FilesPreferences.defaults.entries) {
      values[entry.key] = FilesPreferences.copySection(entry.value);
      revisions[entry.key] = 'rev-${entry.key}-1';
    }
    if (!available) {
      readFailure = StateError('Unavailable: remyad is not running');
    }
  }

  final signals = StreamController<SettingsChange>.broadcast();
  final owners = StreamController<bool>.broadcast();
  final calls = <(String method, List<String> arguments)>[];
  final values = <String, Map<String, dynamic>>{
    'clipboard-history': {'__scalar__': true},
    'clipboard-max-items': {'__scalar__': 500},
    'clipboard-retention-days': {'__scalar__': 30},
  };
  final revisions = <String, String>{};
  int _revCounter = 1;
  int closeCount = 0;
  Object? readFailure;
  Object? writeFailure;
  bool omitRevisionOnGet = false;

  @override
  Stream<SettingsChange> get changes => signals.stream;

  @override
  Stream<bool> get serviceAvailableChanges => owners.stream;

  @override
  Future<void> close() async {
    closeCount++;
  }

  @override
  Future<Map<String, dynamic>> call(
    String method, [
    List<String> arguments = const [],
  ]) async {
    calls.add((method, List<String>.from(arguments)));
    final id = arguments.first;
    final key = id.startsWith('remya-launcher.')
        ? id.substring('remya-launcher.'.length)
        : id;

    if (method == 'Get') {
      if (readFailure case final failure?) throw failure;
      final stored = values[key];
      if (stored == null) throw StateError('Unknown key $id');
      final payloadValue = stored.containsKey('__scalar__')
          ? stored['__scalar__']
          : FilesPreferences.copySection(stored);
      return {
        'id': id,
        'availability': 'available',
        'value': payloadValue,
        if (!omitRevisionOnGet && revisions.containsKey(key))
          'revision': revisions[key],
      };
    }

    if (method == 'Replace') {
      if (writeFailure case final failure?) throw failure;
      final expectedRevision = arguments[2];
      final currentRevision = revisions[key];
      if (currentRevision != expectedRevision) {
        throw StateError('Changed in another view. Reopen the editor.');
      }
      final next = Map<String, dynamic>.from(jsonDecode(arguments[1]) as Map);
      values[key] = FilesPreferences.copySection(next);
      final nextRev = 'rev-$key-${++_revCounter}';
      revisions[key] = nextRev;
      return {
        'id': id,
        'availability': 'available',
        'value': FilesPreferences.copySection(next),
        'revision': nextRev,
      };
    }

    throw StateError('Unexpected Core method: $method');
  }

  void externalUpdate(String id, Map<String, dynamic> nextValue) {
    values[id] = FilesPreferences.copySection(nextValue);
    revisions[id] = 'rev-$id-${++_revCounter}';
  }
}

class _FakeClipboard extends ClipboardClient {
  @override
  Future<void> clear() async {}
}

class _MockFilesClient extends FilesClient {
  final calls = <(String op, Map<String, Object?> arguments)>[];
  Object? statusError;
  Object? clearError;
  Map<String, String>? fileOverride;
  Map<String, String>? directoryOverride;
  int closeCount = 0;

  @override
  Future<Map<String, dynamic>> request(
    String op, [
    Map<String, Object?> arguments = const {},
    void Function(Map<String, dynamic>)? onPartial,
  ]) async {
    calls.add((op, Map<String, Object?>.from(arguments)));
    if (op == 'history.status' || op == 'history.refresh') {
      if (statusError case final error?) throw error;
      final prefs = arguments['preferences'] as Map<String, dynamic>? ?? {};
      final history = prefs['files-history'] as Map<String, dynamic>? ?? {};
      final enabled = history['enabled'] == true;
      final external = history['external_enabled'] == true;
      final fileProvider = history['file_provider'] as String? ?? 'memy';
      final dirProvider = history['directory_provider'] as String? ?? 'zoxide';
      return {
        'file':
            fileOverride ??
            _defaultStatus(
              selected: fileProvider,
              enabled: enabled,
              externalEnabled: external,
              refresh: op == 'history.refresh',
            ),
        'directory':
            directoryOverride ??
            _defaultStatus(
              selected: dirProvider,
              enabled: enabled,
              externalEnabled: external,
              refresh: op == 'history.refresh',
            ),
      };
    }
    if (op == 'history.clear') {
      if (clearError case final error?) throw error;
      return {'cleared': true};
    }
    throw StateError('Unexpected Files op: $op');
  }

  static Map<String, String> _defaultStatus({
    required String selected,
    required bool enabled,
    required bool externalEnabled,
    required bool refresh,
  }) {
    if (!enabled) {
      return {
        'selected': selected,
        'effective': 'none',
        'health': 'disabled',
        'reason': 'history_disabled',
      };
    }
    if (selected == 'xbel') {
      return {
        'selected': selected,
        'effective': 'xbel',
        'health': 'ready',
        'reason': '',
      };
    }
    if (!externalEnabled) {
      return {
        'selected': selected,
        'effective': 'xbel',
        'health': 'disabled',
        'reason': 'consent_required',
      };
    }
    return {
      'selected': selected,
      'effective': selected,
      'health': refresh ? 'ready' : 'available',
      'reason': '',
    };
  }

  @override
  void close() {
    closeCount++;
  }
}

void main() {
  test(
    'linux registration JSON matches FilesPreferences defaults and schemas',
    () {
      final jsonText = File('linux/packaging/remya-launcher.json')
          .readAsStringSync();
      final manifest = jsonDecode(jsonText) as Map<String, dynamic>;
      final pages = {
        for (final page
            in (manifest['pages'] as List).cast<Map<String, dynamic>>())
          page['id'] as String: page,
      };
      final settings = manifest['settings'] as Map<String, dynamic>;

      for (final id in FilesPreferences.ids) {
        expect(pages.containsKey(id), isTrue, reason: 'Missing page $id');
        expect(settings.containsKey(id), isTrue, reason: 'Missing setting $id');

        final setting = settings[id] as Map<String, dynamic>;
        expect(setting['page'], id);
        expect(setting['default'], equals(FilesPreferences.defaults[id]));

        final schema = setting['schema'] as Map<String, dynamic>;
        expect(
          schema['\$schema'],
          'https://json-schema.org/draft/2020-12/schema',
        );
        expect(schema['type'], 'object');
        expect(schema['additionalProperties'], isFalse);

        final properties = schema['properties'] as Map<String, dynamic>;
        final required = (schema['required'] as List).cast<String>();
        final expectedKeys = FilesPreferences.defaults[id]!.keys.toSet();
        expect(properties.keys.toSet(), expectedKeys);
        expect(required.toSet(), expectedKeys);
      }

      final searchProps =
          ((settings['files-search'] as Map)['schema'] as Map)['properties']
              as Map<String, dynamic>;
      expect((searchProps['roots'] as Map)['minItems'], 1);
      expect(
        (searchProps['roots'] as Map)['maxItems'],
        FilesPreferences.maxRoots,
      );
      expect(
        (searchProps['exclusions'] as Map)['maxItems'],
        FilesPreferences.maxExclusions,
      );
      expect(
        ((searchProps['exclusions'] as Map)['items'] as Map)['pattern'],
        r'^(/.*|~(/.*)?)$',
      );
      expect(
        (searchProps['contents_max_bytes'] as Map)['minimum'],
        FilesPreferences.minBytesLimit,
      );
      expect(
        (searchProps['contents_max_bytes'] as Map)['maximum'],
        FilesPreferences.maxContentsBytes,
      );
      expect(
        (searchProps['result_limit'] as Map)['minimum'],
        FilesPreferences.minResultLimit,
      );
      expect(
        (searchProps['result_limit'] as Map)['maximum'],
        FilesPreferences.maxResultLimit,
      );

      final historyProps =
          ((settings['files-history'] as Map)['schema'] as Map)['properties']
              as Map<String, dynamic>;
      expect(
        (historyProps['file_provider'] as Map)['enum'],
        FilesPreferences.fileProviders,
      );
      expect(
        (historyProps['directory_provider'] as Map)['enum'],
        FilesPreferences.directoryProviders,
      );
      expect(
        (historyProps['external_write_mode'] as Map)['enum'],
        FilesPreferences.externalWriteModes,
      );
      expect(
        (historyProps['retention_days'] as Map)['maximum'],
        FilesPreferences.maxRetentionDays,
      );
      expect((historyProps['max_records_per_kind'] as Map)['minimum'], 1);
      expect(
        (historyProps['max_records_per_kind'] as Map)['maximum'],
        FilesPreferences.maxHistoryRecords,
      );

      final actionsProps =
          ((settings['files-actions'] as Map)['schema'] as Map)['properties']
              as Map<String, dynamic>;
      for (final key in ['editor', 'terminal', 'file_manager']) {
        final pattern = RegExp((actionsProps[key] as Map)['pattern'] as String);
        expect(pattern.hasMatch(''), isTrue);
        expect(pattern.hasMatch('nvim'), isTrue);
        expect(pattern.hasMatch('/opt/My Editor/bin/edit'), isTrue);
        expect(pattern.hasMatch('nvim -u NONE'), isFalse);
        expect(pattern.hasMatch('bin/nvim'), isFalse);
        expect(pattern.hasMatch('/'), isFalse);
      }

      final previewProps =
          ((settings['files-preview'] as Map)['schema'] as Map)['properties']
              as Map<String, dynamic>;
      expect(
        (previewProps['text_max_bytes'] as Map)['maximum'],
        FilesPreferences.maxTextPreviewBytes,
      );
      expect(
        (previewProps['image_max_bytes'] as Map)['maximum'],
        FilesPreferences.maxImagePreviewBytes,
      );
    },
  );

  group('FilesPreferences service', () {
    test(
      'read() falls back to defaults without writing on Core failure',
      () async {
        final core = _MockCore(available: false);
        addTearDown(core.signals.close);
        addTearDown(core.owners.close);
        final prefs = FilesPreferences(client: core);

        final result = await prefs.read();
        expect(result.keys.toSet(), FilesPreferences.ids.toSet());
        expect(result, equals(FilesPreferences.defaults));
        expect(core.calls.where((c) => c.$1 != 'Get'), isEmpty);

        // Mutating returned values must not alter static defaults.
        (result['files-search']!['roots'] as List).add('/tmp');
        expect(FilesPreferences.defaults['files-search']!['roots'], ['~']);

        await prefs.close();
        expect(core.closeCount, 0);
      },
    );

    test('setValue() uses Get baseline and CAS Replace without overwriting conflicts', () async {
      final core = _MockCore();
      addTearDown(core.signals.close);
      addTearDown(core.owners.close);
      final prefs = FilesPreferences(client: core);

      await prefs.setValue('files-search', 'result_limit', 350);
      expect(core.calls.map((c) => c.$1).toList(), ['Get', 'Replace']);
      final replaceArgs = core.calls.last.$2;
      expect(replaceArgs[0], 'remya-launcher.files-search');
      expect((jsonDecode(replaceArgs[1]) as Map)['result_limit'], 350);
      expect(replaceArgs[2], 'rev-files-search-1');

      // Validate literal exclusion paths (including metacharacters) and action targets.
      await prefs.setValue('files-search', 'exclusions', <String>[
        '/tmp/a*b[1]',
        '~/Projects/build',
      ]);
      await prefs.setValue(
        'files-actions',
        'editor',
        '/opt/My Editor/bin/edit',
      );
      expect(core.values['files-search']!['exclusions'], [
        '/tmp/a*b[1]',
        '~/Projects/build',
      ]);
      expect(
        core.values['files-actions']!['editor'],
        '/opt/My Editor/bin/edit',
      );

      // Validate that invalid values fail before touching Core.
      final callsBefore = core.calls.length;
      await expectLater(
        prefs.setValue('files-search', 'roots', <String>['relative/path']),
        throwsFormatException,
      );
      await expectLater(
        prefs.setValue('files-search', 'exclusions', <String>[
          '**/node_modules',
        ]),
        throwsFormatException,
      );
      await expectLater(
        prefs.setValue('files-search', 'exclusions', <String>['relative/path']),
        throwsFormatException,
      );
      await expectLater(
        prefs.setValue('files-search', 'result_limit', 19),
        throwsFormatException,
      );
      await expectLater(
        prefs.setValue('files-search', 'result_limit', 501),
        throwsFormatException,
      );
      await expectLater(
        prefs.setValue('files-search', 'contents_max_bytes', 10485761),
        throwsFormatException,
      );
      await expectLater(
        prefs.setValue('files-history', 'max_records_per_kind', 10001),
        throwsFormatException,
      );
      await expectLater(
        prefs.setValue('files-actions', 'editor', 'nvim -u NONE'),
        throwsFormatException,
      );
      await expectLater(
        prefs.setValue('files-actions', 'terminal', 'bin/kitty'),
        throwsFormatException,
      );
      expect(core.calls.length, callsBefore);
    });

    test(
      'setValue() surfaces CAS conflict without falling back to Set',
      () async {
        final core = _MockCore()
          ..writeFailure = StateError(
            'Changed in another view. Reopen the editor.',
          );
        addTearDown(core.signals.close);
        addTearDown(core.owners.close);
        final prefs = FilesPreferences(client: core);

        await expectLater(
          prefs.setValue('files-preview', 'show_preview', false),
          throwsStateError,
        );
        expect(core.calls.where((c) => c.$1 == 'Set'), isEmpty);
        expect(core.values['files-preview']!['show_preview'], isTrue);
      },
    );
  });

  group('Files settings UI', () {
    Future<void> pumpWindow(
      WidgetTester tester,
      _MockCore core, {
      _MockFilesClient? files,
      String initialPage = filesSettingsPageId,
    }) async {
      await tester.binding.setSurfaceSize(const Size(1100, 950));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ClipboardSettingsWindow(
              initialPage: initialPage,
              client: core,
              clipboard: _FakeClipboard(),
              files: files ?? _MockFilesClient(),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    testWidgets('shows Core errors, retries, and renders all four Files tabs', (
      tester,
    ) async {
      final core = _MockCore(available: false);
      addTearDown(core.signals.close);
      addTearDown(core.owners.close);

      await pumpWindow(tester, core);

      // Sidebar entry under system category and four tabs.
      final filesIcon = tester.widget<RemyaSidebarIcon>(
        find.descendant(
          of: find.byKey(const ValueKey('nav-files')),
          matching: find.byType(RemyaSidebarIcon),
        ),
      );
      expect(filesIcon.category, RemyaSettingsCategory.system);
      expect(find.byKey(const ValueKey('nav-page-files-search')), findsOne);
      expect(find.byKey(const ValueKey('nav-page-files-history')), findsOne);
      expect(find.byKey(const ValueKey('nav-page-files-preview')), findsOne);
      expect(find.byKey(const ValueKey('nav-page-files-actions')), findsOne);

      expect(
        find.textContaining('Remy\'a Core cannot provide Files preferences'),
        findsOne,
      );
      expect(find.text('Unavailable'), findsNWidgets(7));

      // Recover via Retry.
      core.readFailure = null;
      await tester.tap(find.text('Retry'));
      await tester.pumpAndSettle();
      expect(
        find.textContaining('Remy\'a Core cannot provide Files preferences'),
        findsNothing,
      );
      expect(find.text('~'), findsOne);
      expect(find.text('2 MiB (2097152 B)'), findsOne);
      expect(find.text('200 results'), findsOne);

      // Switch to History tab.
      await tester.tap(find.byKey(const ValueKey('nav-page-files-history')));
      await tester.pumpAndSettle();
      expect(find.textContaining('off by default for privacy'), findsWidgets);
      expect(find.text('90 days'), findsOne);
      expect(find.text('10000 records'), findsOne);

      // Switch to Preview tab.
      await tester.tap(find.byKey(const ValueKey('nav-page-files-preview')));
      await tester.pumpAndSettle();
      expect(find.text('10 MiB (10485760 B)'), findsOne);
      expect(find.text('50 MiB (52428800 B)'), findsOne);

      // Switch to Actions tab.
      await tester.tap(find.byKey(const ValueKey('nav-page-files-actions')));
      await tester.pumpAndSettle();
      expect(find.text('Automatic'), findsNWidgets(3));

      await tester.pumpWidget(const SizedBox.shrink());
    });

    testWidgets(
      'edits Search, History, Preview, and Actions preferences via Replace',
      (tester) async {
        final core = _MockCore();
        addTearDown(core.signals.close);
        addTearDown(core.owners.close);

        await pumpWindow(tester, core, initialPage: 'files-search');

        // Edit roots via line-separated text dialog with validation.
        await tester.tap(find.text('~'));
        await tester.pumpAndSettle();
        await tester.enterText(find.byType(TextFormField), 'relative/path');
        await tester.tap(find.text('Save'));
        await tester.pumpAndSettle();
        expect(
          find.textContaining('absolute path (/...) or ~/...'),
          findsWidgets,
        );

        await tester.enterText(find.byType(TextFormField), '~/Projects\n/tmp');
        await tester.tap(find.text('Save'));
        await tester.pumpAndSettle();
        expect(core.values['files-search']!['roots'], ['~/Projects', '/tmp']);
        expect(find.text('2 folders'), findsOne);

        // Toggle show_hidden.
        await tester.tap(
          find.descendant(
            of: find.byKey(const ValueKey('show_hidden')),
            matching: find.byType(Switch),
          ),
        );
        await tester.pumpAndSettle();
        expect(core.values['files-search']!['show_hidden'], isTrue);

        // Edit result_limit with range check.
        await tester.ensureVisible(find.text('200 results'));
        await tester.tap(find.text('200 results'));
        await tester.pumpAndSettle();
        await tester.enterText(find.byType(TextFormField), '99999');
        await tester.tap(find.text('Save'));
        await tester.pumpAndSettle();
        expect(find.text('Enter a whole number from 20 to 500.'), findsOne);
        await tester.enterText(find.byType(TextFormField), '120');
        await tester.tap(find.text('Save'));
        await tester.pumpAndSettle();
        expect(core.values['files-search']!['result_limit'], 120);
        expect(find.text('120 results'), findsOne);

        // Watch external change signal.
        core.externalUpdate('files-search', {
          ...core.values['files-search']!,
          'result_limit': 42,
        });
        core.signals.add((topic: 'remya-launcher.files-search', value: {}));
        await tester.pumpAndSettle();
        expect(find.text('42 results'), findsOne);

        // History dropdowns.
        await tester.ensureVisible(
          find.byKey(const ValueKey('nav-page-files-history')),
        );
        await tester.tap(find.byKey(const ValueKey('nav-page-files-history')));
        await tester.pumpAndSettle();
        await tester.ensureVisible(find.text('Zoxide'));
        await tester.tap(find.text('Zoxide'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Memy').last);
        await tester.pumpAndSettle();
        expect(core.values['files-history']!['directory_provider'], 'memy');

        await tester.ensureVisible(find.text('Read only'));
        await tester.tap(find.text('Read only'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Launcher writes').last);
        await tester.pumpAndSettle();
        expect(
          core.values['files-history']!['external_write_mode'],
          'launcher',
        );

        // Actions validation and save.
        await tester.ensureVisible(
          find.byKey(const ValueKey('nav-page-files-actions')),
        );
        await tester.tap(find.byKey(const ValueKey('nav-page-files-actions')));
        await tester.pumpAndSettle();
        await tester.tap(
          find.descendant(
            of: find.byKey(const ValueKey('editor')),
            matching: find.byType(OutlinedButton),
          ),
        );
        await tester.pumpAndSettle();
        await tester.enterText(find.byType(TextFormField), 'nvim -u NONE');
        await tester.tap(find.text('Save'));
        await tester.pumpAndSettle();
        expect(
          find.textContaining('without shell operators or control characters'),
          findsOne,
        );
        await tester.enterText(
          find.byType(TextFormField),
          '/opt/My Editor/bin/edit',
        );
        await tester.tap(find.text('Save'));
        await tester.pumpAndSettle();
        expect(
          core.values['files-actions']!['editor'],
          '/opt/My Editor/bin/edit',
        );
        expect(find.text('/opt/My Editor/bin/edit'), findsOne);

        await tester.pumpWidget(const SizedBox.shrink());
      },
    );

    testWidgets(
      'captures displayed revision and refuses to overwrite CAS conflicts',
      (tester) async {
        final core = _MockCore();
        addTearDown(core.signals.close);
        addTearDown(core.owners.close);

        await pumpWindow(tester, core, initialPage: 'files-preview');

        // Another client updates Core behind our back (no signal delivered).
        core.externalUpdate('files-preview', {
          'show_preview': true,
          'text_max_bytes': 4096,
          'image_max_bytes': 52428800,
        });

        // User toggles show_preview in UI; UI must send the captured displayed
        // revision (rev-files-preview-1) and surface the conflict instead of overwriting.
        await tester.tap(
          find.descendant(
            of: find.byKey(const ValueKey('show_preview')),
            matching: find.byType(Switch),
          ),
        );
        await tester.pumpAndSettle();

        expect(
          find.textContaining('Changed in another view. Reopen the editor.'),
          findsOne,
        );
        expect(core.values['files-preview']!['show_preview'], isTrue);
        expect(core.values['files-preview']!['text_max_bytes'], 4096);

        // Reload brings the new Core value and revision into the UI.
        await tester.tap(find.text('Reload'));
        await tester.pumpAndSettle();
        expect(
          find.textContaining('Changed in another view. Reopen the editor.'),
          findsNothing,
        );
        expect(find.text('4 KiB (4096 B)'), findsOne);

        // Now toggling succeeds against the reloaded revision.
        await tester.tap(
          find.descendant(
            of: find.byKey(const ValueKey('show_preview')),
            matching: find.byType(Switch),
          ),
        );
        await tester.pumpAndSettle();
        expect(core.values['files-preview']!['show_preview'], isFalse);

        await tester.pumpWidget(const SizedBox.shrink());
      },
    );

    testWidgets(
      'displays Files history provider status, refreshes explicitly, and confirms clear',
      (tester) async {
        final core = _MockCore();
        final files = _MockFilesClient()
          ..statusError = const FilesException(
            'Files service could not start.',
          );
        addTearDown(core.signals.close);
        addTearDown(core.owners.close);

        await pumpWindow(
          tester,
          core,
          files: files,
          initialPage: 'files-history',
        );

        // Initial load uses history.status (never history.refresh) and permits retry.
        expect(files.calls.map((c) => c.$1).toList(), ['history.status']);
        expect(
          find.textContaining('Could not check Files history status.'),
          findsOne,
        );

        files.statusError = null;
        await tester.ensureVisible(find.text('Retry'));
        await tester.tap(find.text('Retry'));
        await tester.pumpAndSettle();
        expect(files.calls.map((c) => c.$1).toList(), [
          'history.status',
          'history.status',
        ]);
        expect(find.text('History is off'), findsNWidgets(2));
        expect(find.text('Selected: Memy · Effective: None'), findsOne);
        expect(find.text('Selected: Zoxide · Effective: None'), findsOne);

        // Enabling history and external providers reports actual effective status via history.status.
        await tester.ensureVisible(find.byKey(const ValueKey('enabled')));
        await tester.tap(
          find.descendant(
            of: find.byKey(const ValueKey('enabled')),
            matching: find.byType(Switch),
          ),
        );
        await tester.pumpAndSettle();
        await tester.ensureVisible(
          find.byKey(const ValueKey('external_enabled')),
        );
        await tester.tap(
          find.descendant(
            of: find.byKey(const ValueKey('external_enabled')),
            matching: find.byType(Switch),
          ),
        );
        await tester.pumpAndSettle();
        expect(files.calls.where((c) => c.$1 == 'history.refresh'), isEmpty);
        expect(find.text('Available'), findsNWidgets(2));

        // Explicit Refresh invokes history.refresh and updates health/reason.
        files.directoryOverride = {
          'selected': 'zoxide',
          'effective': 'xbel',
          'health': 'unavailable',
          'reason': 'not_installed',
        };
        await tester.ensureVisible(find.text('Refresh'));
        await tester.tap(find.text('Refresh'));
        await tester.pumpAndSettle();
        expect(files.calls.last.$1, 'history.refresh');
        final passedPrefs =
            files.calls.last.$2['preferences'] as Map<String, dynamic>;
        expect((passedPrefs['files-history'] as Map)['enabled'], isTrue);
        expect(
          (passedPrefs['files-history'] as Map)['external_enabled'],
          isTrue,
        );
        expect(find.text('Ready'), findsOne);
        expect(find.text('Unavailable'), findsOne);
        expect(find.textContaining('Provider is not installed'), findsOne);

        // Clear Launcher history requires explicit confirmation.
        await tester.ensureVisible(find.text('Clear history'));
        await tester.tap(find.text('Clear history'));
        await tester.pumpAndSettle();
        expect(find.text('Clear Launcher history?'), findsOne);
        await tester.tap(find.text('Cancel'));
        await tester.pumpAndSettle();
        expect(files.calls.where((c) => c.$1 == 'history.clear'), isEmpty);

        await tester.tap(find.text('Clear history'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Clear history').last);
        await tester.pumpAndSettle();
        final clearCalls = files.calls
            .where((c) => c.$1 == 'history.clear')
            .toList();
        expect(clearCalls, hasLength(1));
        expect(clearCalls.single.$2, {'scope': 'launcher'});
        expect(find.text('Launcher files history cleared'), findsOne);

        await tester.pumpWidget(const SizedBox.shrink());
        expect(files.closeCount, 0);
      },
    );
  });
}
