import 'dart:async';
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_client/remya_client.dart';
import 'package:remya_launcher/app.dart';
import 'package:remya_launcher/features/clipboard/clipboard_client.dart';
import 'package:remya_launcher/features/emoji/emoji_settings.dart';
import 'package:remya_launcher/features/launcher/startup_mode.dart';
import 'package:remya_launcher/features/launcher/startup_settings.dart';
import 'package:remya_launcher/main.dart';
import 'package:remya_ui/settings_ui.dart';

class _Core extends SettingsClient {
  final signals = StreamController<SettingsChange>.broadcast();
  final owners = StreamController<bool>.broadcast();
  final writes = <List<String>>[];
  final values = <String, Object>{
    'clipboard-history': true,
    'clipboard-max-items': 500,
    'clipboard-retention-days': 30,
    emojiAutoPasteKey: false,
    emojiRecentsKey: false,
    residentStartupKey: false,
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
    if (method == 'Get' && id == 'theme') {
      return {
        'version': 1,
        'theme': {
          'style_variant': 'tonal_spot',
          'theme_mode': 'light',
          'contrast': 'standard',
        },
      };
    }
    if (method == 'Get' && id == 'accent') {
      return {'source': 'custom', 'color': '#8054C9'};
    }
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
      find.textContaining('Remy\'a Core cannot provide the clipboard policy'),
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
      find.textContaining('Remy\'a Core cannot provide the clipboard policy'),
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
    expect(find.textContaining('Clipboard history service error'), findsOne);
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
    expect(
      find.textContaining('Clipboard history service error'),
      findsNothing,
    );

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

  test('parseSettingsPage accepts registered pages and an empty page', () {
    expect(parseSettingsPage(const []), isNull);
    expect(parseSettingsPage(const ['--clipboard']), isNull);
    expect(parseSettingsPage(const ['--settings']), '');
    expect(parseSettingsPage(const ['--settings', '--clipboard']), '');
    expect(parseSettingsPage(const ['--settings', 'clipboard']), 'clipboard');
    expect(
      parseSettingsPage(const ['--settings', 'startup']),
      startupSettingsPageId,
    );
    expect(
      parseSettingsPage(const ['--settings', emojiSettingsPageId]),
      emojiSettingsPageId,
    );
    expect(
      parseSettingsPage(const ['--settings', remyaMaterialThemePageId]),
      remyaMaterialThemePageId,
    );
    expect(
      () => parseSettingsPage(const ['--settings', 'unknown']),
      throwsArgumentError,
    );
  });

  testWidgets('Startup settings use the Core preference and follow changes', (
    tester,
  ) async {
    final core = _Core()..failure = null;
    addTearDown(core.signals.close);
    addTearDown(core.owners.close);
    await tester.binding.setSurfaceSize(const Size(760, 640));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      RemyaLauncherApp(
        settingsPage: startupSettingsPageId,
        settingsClient: core,
      ),
    );
    await tester.pumpAndSettle();
    bool? selection() => tester
        .widget<SettingsSegmentedButton<bool>>(
          find.byType(SettingsSegmentedButton<bool>),
        )
        .selected;
    expect(selection(), isFalse);

    core.values[residentStartupKey] = true;
    core.signals.add((topic: 'remya-launcher.$residentStartupKey', value: {}));
    await tester.pumpAndSettle();
    expect(selection(), isTrue);

    await tester.tap(find.text('On demand'));
    await tester.pumpAndSettle();
    expect(core.writes.single, ['remya-launcher.$residentStartupKey', 'false']);
    expect(selection(), isFalse);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets('Emoji settings use Core for both opt-in preferences', (
    tester,
  ) async {
    final core = _Core()..failure = null;
    addTearDown(core.signals.close);
    addTearDown(core.owners.close);
    await tester.binding.setSurfaceSize(const Size(960, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      RemyaLauncherApp(
        settingsPage: emojiSettingsPageId,
        settingsClient: core,
        clipboardClient: _Clipboard(),
      ),
    );
    await tester.pumpAndSettle();

    Finder toggle(String key) => find.descendant(
      of: find.byKey(ValueKey(key)),
      matching: find.byType(Switch),
    );
    expect(tester.widget<Switch>(toggle(emojiAutoPasteKey)).value, isFalse);
    expect(tester.widget<Switch>(toggle(emojiRecentsKey)).value, isFalse);

    await tester.tap(toggle(emojiAutoPasteKey));
    await tester.pumpAndSettle();
    expect(core.writes.single, ['remya-launcher.$emojiAutoPasteKey', 'true']);
    expect(tester.widget<Switch>(toggle(emojiAutoPasteKey)).value, isTrue);

    core.values[emojiRecentsKey] = true;
    core.signals.add((topic: 'remya-launcher.$emojiRecentsKey', value: {}));
    await tester.pumpAndSettle();
    expect(tester.widget<Switch>(toggle(emojiRecentsKey)).value, isTrue);
    await tester.tap(toggle(emojiRecentsKey));
    await tester.pumpAndSettle();
    expect(core.writes.last, ['remya-launcher.$emojiRecentsKey', 'false']);
    expect(tester.widget<Switch>(toggle(emojiRecentsKey)).value, isFalse);

    core.values[emojiRecentsKey] = 'invalid';
    core.signals.add((topic: 'remya-launcher.$emojiRecentsKey', value: {}));
    await tester.pumpAndSettle();
    expect(find.textContaining('invalid $emojiRecentsKey'), findsOne);
    expect(tester.widget<Switch>(toggle(emojiRecentsKey)).value, isFalse);
    expect(tester.widget<Switch>(toggle(emojiRecentsKey)).onChanged, isNull);

    await tester.tap(find.text('Clear recents'));
    await tester.pumpAndSettle();
    expect(find.text('Clear recent emoji?'), findsOne);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
    await tester.pumpWidget(const SizedBox.shrink());
  });

  testWidgets(
    'settings routing preserves Clipboard first and opens Material theme',
    (tester) async {
      final core = _Core()..failure = null;
      final clipboard = _Clipboard();
      addTearDown(core.signals.close);
      addTearDown(core.owners.close);
      await tester.binding.setSurfaceSize(const Size(1100, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(
        RemyaLauncherApp(
          settingsPage: '',
          settingsClient: core,
          clipboardClient: clipboard,
        ),
      );
      await tester.pumpAndSettle();

      expect(find.byKey(const ValueKey('content-clipboard')), findsOne);
      final clipboardIcon = tester.widget<RemyaSidebarIcon>(
        find.descendant(
          of: find.byKey(const ValueKey('nav-clipboard')),
          matching: find.byType(RemyaSidebarIcon),
        ),
      );
      expect(clipboardIcon.category, RemyaSettingsCategory.system);
      final themeIcon = tester.widget<RemyaSidebarIcon>(
        find.descendant(
          of: find.byKey(const ValueKey('nav-$remyaMaterialThemePageId')),
          matching: find.byType(RemyaSidebarIcon),
        ),
      );
      expect(themeIcon.category, RemyaSettingsCategory.personalization);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('nav-about')),
          matching: find.byType(RemyaSidebarIcon),
        ),
        findsNothing,
      );

      await tester.tap(
        find.byKey(const ValueKey('nav-$remyaMaterialThemePageId')),
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('content-$remyaMaterialThemePageId')),
        findsOne,
      );
      expect(find.text('Changes apply to all Remy\'a apps.'), findsOne);
      expect(find.text('Theme mode'), findsOne);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpWidget(
        RemyaLauncherApp(
          settingsPage: remyaMaterialThemePageId,
          settingsClient: core,
          clipboardClient: clipboard,
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.byKey(const ValueKey('content-$remyaMaterialThemePageId')),
        findsOne,
      );
      expect(find.text('Changes apply to all Remy\'a apps.'), findsOne);

      await tester.tap(find.byKey(const ValueKey('nav-clipboard')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('content-clipboard')), findsOne);
      await tester.pumpWidget(const SizedBox.shrink());
    },
  );
}
