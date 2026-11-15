import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/features/apps/apps_client.dart';
import 'package:remya_launcher/features/apps/apps_settings.dart';
import 'package:remya_ui/settings_ui.dart';

class _AppsClient extends AppsClient {
  final values = <String, Object>{
    'frequentDays': 30,
    'pinyin': true,
    'terminal': '',
    'showPackages': true,
    'official': true,
    'flatpak': true,
    'aur': true,
  };
  final writes = <Map<String, Object?>>[];
  final restoreAttempts = <String?>[];
  bool unavailable = true;
  bool restoreFailed = true;

  @override
  Future<Map<String, dynamic>> preferences([
    Map<String, Object?>? updates,
  ]) async {
    if (unavailable) throw const AppsException('Unavailable.');
    if (updates != null) {
      writes.add(updates);
      values.addAll(updates.cast<String, Object>());
    }
    return {...values};
  }

  @override
  Future<List<LauncherAppEntry>> hidden() async => [
    LauncherAppEntry.fromJson({'id': 'example.desktop', 'name': 'An app'}),
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
    expect(action, 'unhide');
    restoreAttempts.add(appId);
    if (restoreFailed) throw const AppsException('Could not save.');
    return null;
  }
}

void main() {
  testWidgets('Apps settings retry reads, save preferences and restore apps', (
    tester,
  ) async {
    final apps = _AppsClient();
    var selected = 'apps';
    await tester.binding.setSurfaceSize(const Size(1100, 1000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: StatefulBuilder(
            builder: (_, setState) => RemyaSettingsScaffold(
              pages: [appsSettingsPage(apps)],
              selected: selected,
              onSelected: (page) => setState(() => selected = page),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.textContaining('Could not load Apps preferences'), findsOne);
    apps.unavailable = false;
    await tester.tap(find.text('Retry'));
    await tester.pumpAndSettle();

    await tester.tap(find.text('30 days'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byType(TextFormField), '0');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(apps.writes, isEmpty);
    expect(find.text('Enter a whole number from 1 to 365.'), findsOne);
    await tester.enterText(find.byType(TextFormField), '60');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(apps.writes.single, {'frequentDays': 60});
    expect(find.text('60 days'), findsOne);

    await tester.tap(find.text('Hidden apps'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Restore'));
    await tester.pumpAndSettle();
    expect(find.text('An app'), findsOne);
    expect(find.textContaining('Could not restore An app.'), findsOne);
    apps.restoreFailed = false;
    await tester.tap(find.text('Restore'));
    await tester.pumpAndSettle();
    expect(apps.restoreAttempts, ['example.desktop', 'example.desktop']);
    expect(find.text('No hidden apps.'), findsOne);
  });
}
