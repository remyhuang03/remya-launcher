import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/app.dart';
import 'package:remya_launcher/features/clipboard/clipboard_platform.dart';
import 'package:remya_launcher/features/web/web_client.dart';

const _site = WebResult(
  entryId: 'history:1',
  title: 'Launcher documentation',
  url: 'https://docs.example.org/launcher',
  hostname: 'docs.example.org',
);
const _github = WebEngine(
  engineId: 'github',
  label: 'GitHub',
  keyword: 'gh',
  hostname: 'github.com',
);
const _youtube = WebEngine(
  engineId: 'youtube',
  label: 'YouTube',
  keyword: 'yt',
  hostname: 'youtube.com',
);

class _Web extends WebClient {
  final calls = <(String, String?)>[];
  final actionsRun = <(String, String?, String?)>[];
  final slow = Completer<WebSearchResult>();

  @override
  bool get available => true;

  @override
  Future<List<WebEngine>> engines() async => const [_github, _youtube];

  @override
  Future<WebSearchResult> search(
    String query, {
    String? engineId,
    List<String>? profileIds,
    int? limit,
  }) async {
    calls.add((query, engineId));
    if (query == 'slow') return slow.future;
    final target = query.startsWith('docs.example.org/')
        ? 'https://$query'
        : null;
    return WebSearchResult(
      recent: query.isEmpty && engineId == null ? const [_site] : const [],
      groups: [
        if (target == null && query.isNotEmpty && engineId == null)
          const WebGroup(kind: 'history', items: [_site]),
        if (query.isNotEmpty || engineId != null)
          WebGroup(
            kind: 'search',
            items: [
              WebResult(
                title: 'Search ${engineId ?? 'DuckDuckGo'} for $query',
                url:
                    'https://duckduckgo.com/?q=${Uri.encodeQueryComponent(query)}',
                engineId: engineId ?? 'duckduckgo',
              ),
            ],
          ),
      ],
      input: WebInput(
        kind: target == null ? 'search' : 'url',
        directNavigationTarget: target,
        completion: query == 'docs.example.org/la'
            ? const WebCompletion(
                suggestedUrl: 'https://docs.example.org/launcher',
                sourceEntryId: 'history:1',
              )
            : null,
      ),
    );
  }

  @override
  Future<List<WebAction>> actions(String entryId) async => const [
    WebAction(action: 'open.default', label: 'Open'),
    WebAction(action: 'copy.url', label: 'Copy URL'),
    WebAction(action: 'forget.localPage', label: 'Hide this page in Launcher'),
  ];

  @override
  Future<void> perform(
    String action, {
    String? entryId,
    String? url,
    String? engineId,
    String? browserId,
    String? profileId,
    String? eventId,
  }) async {
    actionsRun.add((action, entryId, url));
  }
}

class _Platform extends ClipboardPlatform {
  int closed = 0;

  @override
  Future<void> close() async => closed++;
}

Finder get _field => find.byType(TextFormField).first;

Future<void> _render(WidgetTester tester, _Web web, _Platform platform) async {
  await tester.binding.setSurfaceSize(const Size(900, 740));
  addTearDown(() => tester.binding.setSurfaceSize(null));
  await tester.pumpWidget(
    RemyaLauncherApp(webClient: web, clipboardPlatform: platform),
  );
  await tester.pump();
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

Future<void> _type(WidgetTester tester, String text) async {
  await tester.enterText(_field, text);
  await tester.pump(const Duration(milliseconds: 80));
  await tester.pump();
}

void main() {
  testWidgets(
    'All direct URL opens the typed URL; Right Arrow accepts completion',
    (tester) async {
      final web = _Web();
      final platform = _Platform();
      await _render(tester, web, platform);
      await _type(tester, 'docs.example.org/la');
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pump();
      expect(web.actionsRun.single, (
        'open.default',
        null,
        'https://docs.example.org/la',
      ));

      await _ctrl(tester, LogicalKeyboardKey.digit4);
      expect(
        tester.widget<TextFormField>(_field).controller!.text,
        'docs.example.org/la',
      );
      expect(find.text('accept'), findsOneWidget);
      final editable = tester.state<EditableTextState>(
        find.byType(EditableText),
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pumpAndSettle();
      expect(editable.widget.controller.text, 'docs.example.org/la');
      await tester.showKeyboard(_field);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextFormField>(_field).controller!.text,
        'docs.example.org/launcher',
      );
      expect(
        tester.state<EditableTextState>(find.byType(EditableText)),
        same(editable),
      );
      expect(editable.widget.focusNode.hasFocus, isTrue);
      expect(platform.closed, 1);
    },
  );

  testWidgets(
    'Engine keyword enters Web, Tab cycles, Backspace removes token',
    (tester) async {
      final web = _Web();
      await _render(tester, web, _Platform());
      await _type(tester, 'gh ');
      expect(find.text('GitHub'), findsOneWidget);
      expect(web.calls.last, ('', 'github'));
      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pumpAndSettle();
      expect(web.calls.last, ('', 'youtube'));
      expect(find.text('YouTube'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pumpAndSettle();
      expect(web.calls.last, ('', null));
      expect(find.text('Launcher documentation'), findsOneWidget);
    },
  );

  testWidgets(
    'Outdated search reply is ignored and hiding requires confirmation',
    (tester) async {
      final web = _Web();
      await _render(tester, web, _Platform());
      await _type(tester, '?');
      await _type(tester, 'slow');
      await _type(tester, 'docs');
      web.slow.complete(const WebSearchResult());
      await tester.pump();
      expect(
        find.textContaining('Launcher documentation', findRichText: true),
        findsOneWidget,
      );

      await _ctrl(tester, LogicalKeyboardKey.keyK);
      expect(find.text('Copy as Markdown link'), findsOneWidget);
      await tester.tap(find.text('Hide this page in Launcher'));
      await tester.pumpAndSettle();
      expect(find.text('Hide this page?'), findsOneWidget);
      expect(web.actionsRun, isEmpty);
      await tester.tap(find.widgetWithText(FilledButton, 'Hide'));
      await tester.pumpAndSettle();
      expect(web.actionsRun.single, ('forget.localPage', 'history:1', null));
    },
  );
}
