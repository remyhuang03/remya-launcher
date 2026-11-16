import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/features/web/web_client.dart';
import 'package:remya_launcher/features/web/web_settings.dart';
import 'package:remya_ui/settings_ui.dart';

class _WebClient extends WebClient {
  Set<String> enabled = {};
  final changes = <(String, bool)>[];
  bool unavailable = true;
  int clears = 0;
  int refreshes = 0;
  String engine = 'duckduckgo';

  @override
  Future<String> defaultEngine() async => engine;

  @override
  Future<String> setDefaultEngine(String engineId) async => engine = engineId;

  @override
  Future<WebSyncStatus> syncStatus() async {
    if (unavailable) {
      throw const WebException('INTERNAL', 'Browser data unavailable');
    }
    return WebSyncStatus(
      profiles: [
        WebProfile(
          profileId: 'firefox-personal',
          browserId: 'firefox',
          browserLabel: 'Firefox',
          label: 'Personal',
          enabled: enabled.contains('firefox-personal'),
        ),
        WebProfile(
          profileId: 'chromium-work',
          browserId: 'chromium',
          browserLabel: 'Chromium',
          label: 'Work',
          status: 'UNSUPPORTED_SCHEMA',
          enabled: enabled.contains('chromium-work'),
        ),
      ],
    );
  }

  @override
  Future<void> setProfileEnabled(String profileId, bool value) async {
    changes.add((profileId, value));
    if (value) {
      enabled.add(profileId);
    } else {
      enabled.remove(profileId);
    }
  }

  @override
  Future<void> requestSync({
    List<String>? profileIds,
    bool full = false,
  }) async {
    expect(full, isTrue);
    refreshes++;
  }

  @override
  Future<void> clear() async {
    clears++;
    enabled.clear();
  }
}

void main() {
  testWidgets(
    'Web settings only enables the selected profile and confirms clear',
    (tester) async {
      final client = _WebClient();
      await tester.binding.setSurfaceSize(const Size(1100, 1000));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: RemyaSettingsScaffold(
              pages: [webSettingsPage(client)],
              selected: 'web',
              onSelected: (_) {},
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Could not load browser profiles.'), findsOne);
      expect(client.changes, isEmpty);
      client.unavailable = false;
      await tester.tap(find.text('Retry'));
      await tester.pumpAndSettle();

      expect(find.text('Browser data format is unsupported'), findsOne);
      await tester.tap(find.text('DuckDuckGo'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Google').last);
      await tester.pumpAndSettle();
      expect(client.engine, 'google');
      expect(find.text('Google'), findsOne);
      final switches = find.byType(Switch);
      expect(tester.widgetList<Switch>(switches).map((w) => w.value), [
        false,
        false,
      ]);
      await tester.tap(switches.first);
      await tester.pumpAndSettle();
      expect(client.changes, [('firefox-personal', true)]);
      expect(client.enabled, {'firefox-personal'});

      await tester.tap(find.text('Refresh'));
      await tester.pumpAndSettle();
      expect(client.refreshes, 1);
      await tester.tap(find.text('Clear…'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(client.clears, 0);
      expect(client.enabled, {'firefox-personal'});

      await tester.tap(find.text('Clear…'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Clear data'));
      await tester.pumpAndSettle();
      expect(client.clears, 1);
      expect(client.enabled, isEmpty);
      expect(tester.widgetList<Switch>(switches).map((w) => w.value), [
        false,
        false,
      ]);
    },
  );
}
