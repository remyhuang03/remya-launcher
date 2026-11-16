import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/features/web/web_client.dart';
import 'package:remya_launcher/features/web/web_results.dart';

const _github = WebEngine(
  engineId: 'github',
  label: 'GitHub',
  keyword: 'gh',
  hostname: 'github.com',
);
const _search = WebResult(
  title: 'Search GitHub',
  url: 'https://github.com/search?q=hypr',
  engineId: 'github',
);
const _alternative = WebResult(
  title: 'Search DuckDuckGo',
  url: 'https://duckduckgo.com/?q=hypr',
  engineId: 'duckduckgo',
);
const _bookmark = WebResult(
  entryId: 'bookmark',
  title: 'Hyprland wiki',
  url: 'https://example.org/wiki',
);
const _history = WebResult(
  entryId: 'history',
  title: 'Hyprland docs',
  url: 'https://example.org/wiki',
);

void main() {
  testWidgets(
    'a typed query explains disabled history and keeps engine search',
    (tester) async {
      final result = WebSearchResult.fromJson({
        'sync': {'enabledProfileCount': 0},
        'groups': [
          {
            'kind': 'search',
            'items': [
              {
                'title': 'Search GitHub',
                'url': _search.url,
                'engineId': 'github',
              },
            ],
          },
        ],
      });
      var settingsOpened = false;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: WebResults(
              layout: WebResultLayout(result, query: 'remya-bar'),
              selectedKey: null,
              onOpen: (_) {},
              onSelect: (_) {},
              favicon: (_) async => null,
              tileColumns: 6,
              onSettings: () => settingsOpened = true,
              scrollable: false,
            ),
          ),
        ),
      );
      expect(find.text('Browser history is off.'), findsOneWidget);
      expect(find.text('Search GitHub'), findsOneWidget);
      await tester.tap(find.text('Web settings'));
      expect(settingsOpened, isTrue);
    },
  );

  test(
    'All scope prioritizes the exact typed URL and removes duplicate hits',
    () {
      final layout = WebResultLayout(
        const WebSearchResult(
          groups: [
            WebGroup(kind: 'bookmarks', items: [_bookmark]),
            WebGroup(kind: 'history', items: [_history]),
            WebGroup(kind: 'search', items: [_search, _alternative]),
          ],
          input: WebInput(
            directNavigationTarget: 'https://example.org/w',
            completion: WebCompletion(suggestedUrl: 'https://example.org/wiki'),
          ),
        ),
        query: 'example.org/w',
        compact: true,
      );
      expect(layout.choices.map((choice) => choice.url), [
        'https://example.org/w',
        'https://example.org/wiki',
      ]);
      expect(layout.choices.first.entryId, isNull);
    },
  );

  test('engine search moves its primary choice above local groups', () {
    final layout = WebResultLayout(
      const WebSearchResult(
        groups: [
          WebGroup(kind: 'history', items: [_history]),
          WebGroup(kind: 'search', items: [_alternative, _search]),
        ],
      ),
      query: 'hypr',
      engine: _github,
    );
    expect(layout.choices.map((choice) => choice.item), [
      _search,
      _history,
      _alternative,
    ]);
  });

  testWidgets('selection scrolls from first item to the bottom result', (
    tester,
  ) async {
    final layout = WebResultLayout(
      WebSearchResult(
        groups: [
          WebGroup(
            kind: 'history',
            items: List.generate(
              12,
              (i) => WebResult(
                title: 'History result $i',
                url: 'https://example.org/$i',
              ),
            ),
          ),
          const WebGroup(kind: 'search', items: [_search]),
        ],
      ),
      query: 'hypr',
    );
    Widget view(WebChoice choice) => MaterialApp(
      home: Center(
        child: SizedBox(
          width: 500,
          height: 180,
          child: WebResults(
            layout: layout,
            selectedKey: choice.key,
            onOpen: (_) {},
            onSelect: (_) {},
            favicon: (_) async => null,
            tileColumns: 6,
          ),
        ),
      ),
    );
    await tester.pumpWidget(view(layout.choices.first));
    await tester.pumpAndSettle();
    expect(find.text('Search GitHub'), findsNothing);
    await tester.pumpWidget(view(layout.choices.last));
    await tester.pumpAndSettle();
    expect(find.text('Search GitHub').hitTestable(), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  testWidgets('compact result remains actionable at a narrow width', (
    tester,
  ) async {
    const target = 'https://example.org/a/long/path?q=literal-typed-text';
    final layout = WebResultLayout(
      const WebSearchResult(input: WebInput(directNavigationTarget: target)),
      query: target,
      compact: true,
    );
    final choice = layout.choices.single;
    WebChoice? opened;
    await tester.pumpWidget(
      MaterialApp(
        home: Center(
          child: SizedBox(
            width: 264,
            height: 100,
            child: WebResults(
              layout: layout,
              selectedKey: choice.key,
              onOpen: (choice) => opened = choice,
              onSelect: (_) {},
              favicon: (_) async => null,
              tileColumns: 3,
              scrollable: false,
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open'));
    expect(opened?.url, target);
    expect(tester.takeException(), isNull);
  });
}
