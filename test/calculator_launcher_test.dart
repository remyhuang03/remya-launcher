import 'dart:async';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:material_ui/material_ui.dart';
import 'package:remya_launcher/features/calculator/calculator_client.dart';
import 'package:remya_launcher/features/calculator/calculator_controller.dart';
import 'package:remya_launcher/features/clipboard/clipboard_client.dart';
import 'package:remya_launcher/features/clipboard/clipboard_launcher.dart';
import 'package:remya_launcher/features/clipboard/clipboard_platform.dart';

typedef _RequestHandler = Future<Map<String, Object?>> Function(
  String op,
  Map<String, Object?> fields,
);

class _FakeCalculatorClient extends CalculatorClient {
  _FakeCalculatorClient(this.handler);

  final _RequestHandler handler;
  final List<(String op, Map<String, Object?> fields)> calls = [];
  bool closed = false;

  @override
  Future<Map<String, Object?>> request(
    String op, [
    Map<String, Object?> fields = const {},
  ]) {
    calls.add((op, Map<String, Object?>.from(fields)));
    return handler(op, fields);
  }

  @override
  Future<void> close() async {
    closed = true;
  }
}

class _FakeClipboardClient extends ClipboardClient {
  _FakeClipboardClient();

  final List<String> publishedTexts = [];

  @override
  Future<List<ClipboardEntry>> list({String? query}) async => const [];

  @override
  Future<bool> isEnabled() async => true;

  @override
  Stream<void> watch() => const Stream<void>.empty();

  @override
  Future<void> publishText(String text) async {
    publishedTexts.add(text);
  }
}

class _FakeClipboardPlatform extends ClipboardPlatform {
  _FakeClipboardPlatform();

  bool closed = false;

  @override
  Future<void> close() async {
    closed = true;
  }

  @override
  Future<bool> pasteToPreviousWindow() async => false;
}

Map<String, Object?> _envelope(
  Map<String, Object?> data, {
  int? revision,
  bool ok = true,
}) => {'v': 1, 'id': 'test', 'ok': ok, 'revision': ?revision, 'data': data};

Map<String, Object?> _unitPreview(int? revision) => _envelope({
  'status': 'ready',
  'primaryCandidateId': 'c1',
  'candidates': [
    {
      'id': 'c1',
      'kind': 'unit_conversion',
      'normalized': '100 Mbps → MB/s',
      'interpretation': '100 megabits per second → megabytes per second',
      'assumptions': ['SI decimal prefixes', '1 byte = 8 bits'],
      'value': {
        'tag': 'quantity',
        'decimal': '12.5',
        'unitId': 'byte_per_second_megabyte_si',
        'display': '12.5 MB/s',
      },
      'formats': [
        {'id': 'si', 'label': '12.5 MB/s', 'copyText': '12.5 MB/s'},
        {'id': 'iec', 'label': '11.92 MiB/s', 'copyText': '11.92 MiB/s'},
      ],
      'diagnostics': const [],
    },
  ],
  'diagnostics': const [],
  'completions': const [],
}, revision: revision);

void main() {
  group('CalculatorController', () {
    test('All-scope high-confidence gate filters non-math queries', () {
      const cases = {
        '1920*1080': true,
        '1+1': true,
        '100 Mbps to MB/s': true,
        '0xDEAD_BEEF >> 4': true,
        '0.1 as bf16': true,
        '3pm PST in beijing': true,
        '¥2999 in usd': true,
        '3pi sqrt 2': true,
        '': false,
        'wallpaper-1920x1080.png': false,
        'wallpaper-1920x1080': false,
        'lib/app.dart': false,
        'https://example.com/1+1': false,
        'speed of light': false,
        '1 kilo': false,
        '2026': false,
      };
      for (final MapEntry(key: query, value: expected) in cases.entries) {
        expect(
          CalculatorController.isHighConfidenceAllQuery(query),
          expected,
          reason: query,
        );
      }
    });

    test('ignores stale out-of-order preview responses', () async {
      final firstCompleter = Completer<Map<String, Object?>>();
      final secondCompleter = Completer<Map<String, Object?>>();
      final client = _FakeCalculatorClient((op, fields) {
        if (op == 'preview' && fields['query'] == '1+1') {
          return firstCompleter.future;
        }
        if (op == 'preview' && fields['query'] == '1+2') {
          return secondCompleter.future;
        }
        return Future.value(_envelope({'entries': const []}));
      });

      final controller = CalculatorController(
        client: client,
        debounceDelay: Duration.zero,
      );
      addTearDown(controller.dispose);

      controller.setScope(.calculator);
      controller.updateQuery('1+1');
      final rev1 = controller.revision;
      controller.updateQuery('1+2');
      final rev2 = controller.revision;

      secondCompleter.complete(
        _envelope({
          'status': 'ready',
          'primaryCandidateId': 'c2',
          'candidates': [
            {
              'id': 'c2',
              'kind': 'math',
              'normalized': '1 + 2',
              'interpretation': '1 + 2',
              'value': {'tag': 'integer', 'display': '3'},
              'formats': const [],
            },
          ],
          'diagnostics': const [],
          'completions': const [],
        }, revision: rev2),
      );
      await Future<void>.delayed(Duration.zero);
      expect(controller.selectedCandidate?.primaryDisplay(), '3');

      firstCompleter.complete(
        _envelope({
          'status': 'ready',
          'primaryCandidateId': 'c1',
          'candidates': [
            {
              'id': 'c1',
              'kind': 'math',
              'normalized': '1 + 1',
              'interpretation': '1 + 1',
              'value': {'tag': 'integer', 'display': '2'},
              'formats': const [],
            },
          ],
          'diagnostics': const [],
          'completions': const [],
        }, revision: rev1),
      );
      await Future<void>.delayed(Duration.zero);
      expect(controller.selectedCandidate?.primaryDisplay(), '3');
    });

    test('a history read cannot restore an entry after Clear', () async {
      final pending = Completer<Map<String, Object?>>();
      final client = _FakeCalculatorClient((op, _) {
        return op == 'historyList'
            ? pending.future
            : Future.value(_envelope(const {}));
      });
      final controller = CalculatorController(client: client);
      addTearDown(controller.dispose);
      controller.setScope(.calculator);
      expect(controller.loadingHistory, isTrue);
      await controller.clearHistory(includePinned: true);
      pending.complete(
        _envelope({
          'entries': [
            {'historyId': 'old', 'query': '1+1'},
          ],
        }),
      );
      await Future<void>.delayed(Duration.zero);
      expect(controller.historyEntries, isEmpty);
      expect(controller.loadingHistory, isFalse);
    });

    test('calculator retries incomplete programmer preferences', () async {
      var reads = 0;
      final widths = <Object?>[];
      final client = _FakeCalculatorClient((op, fields) async {
        if (op == 'status' && ++reads == 2) {
          return _envelope({
            'programmer': {'bitWidth': 16},
          });
        }
        if (op == 'preview') {
          widths.add((fields['settings'] as Map)['bitWidth']);
          return _unitPreview(fields['revision'] as int?);
        }
        return _envelope(const {});
      });
      final controller = CalculatorController(
        client: client,
        debounceDelay: Duration.zero,
      );
      addTearDown(controller.dispose);
      controller.setScope(.calculator);
      controller.updateQuery('1+1');
      await Future<void>.delayed(Duration.zero);
      controller.updateQuery('2+2');
      await Future<void>.delayed(Duration.zero);
      expect(widths, [32, 16]);
    });

    test('delays error display until 600ms or immediate reveal', () async {
      final client = _FakeCalculatorClient((op, fields) async {
        if (op == 'preview') {
          return _envelope({
            'status': 'error',
            'candidates': const [],
            'diagnostics': [
              {
                'code': 'dimension_mismatch',
                'severity': 'error',
                'parameters': {
                  'sourceDimension': 'mass',
                  'targetDimension': 'length',
                },
                'suggestions': [
                  {'id': 's1', 'label': '5 kg in lb'},
                ],
              },
            ],
            'completions': const [],
          }, revision: fields['revision'] as int?);
        }
        return _envelope({'entries': const []});
      });

      final controller = CalculatorController(
        client: client,
        debounceDelay: Duration.zero,
        errorDelay: const Duration(milliseconds: 50),
      );
      addTearDown(controller.dispose);

      controller.setScope(.calculator, query: '5 kg in m');
      await Future<void>.delayed(Duration.zero);
      expect(controller.hasPendingError, isTrue);
      expect(controller.shouldShowError, isFalse);

      await Future<void>.delayed(const Duration(milliseconds: 60));
      expect(controller.shouldShowError, isTrue);
      expect(
        controller.errorDiagnostics.first.displayMessage,
        'Cannot convert mass to length',
      );
    });

    test('UTF-16 completion edit preserves CJK/emoji prefix', () {
      const edit = CalculatorTextEdit(
        startUtf16: 5,
        endUtf16: 9,
        text: 'kilogram',
      );
      final updated = edit.applyTo(
        const TextEditingValue(
          text: '🎉 1 kilo',
          selection: TextSelection.collapsed(offset: 9),
        ),
      );
      expect(updated?.text, '🎉 1 kilogram');
      expect(updated?.selection.extentOffset, 13);
    });
  });

  group('ClipboardLauncher calculator integration', () {
    Widget buildApp({
      required _FakeCalculatorClient calcClient,
      required _FakeClipboardClient clipClient,
      required _FakeClipboardPlatform platform,
    }) => MaterialApp(
      home: Scaffold(
        body: ClipboardLauncher(
          client: clipClient,
          platform: platform,
          calculatorClient: calcClient,
        ),
      ),
    );

    testWidgets(
      'enters Calc with Ctrl+5, cycles formats with Tab, copies with Enter',
      (tester) async {
        final calcClient = _FakeCalculatorClient((op, fields) async {
          if (op == 'historyList') {
            return _envelope({'entries': const []});
          }
          if (op == 'preview') {
            return _unitPreview(fields['revision'] as int?);
          }
          if (op == 'commit') {
            return _envelope({'historyId': 'h1'});
          }
          return _envelope(const {});
        });
        final clipClient = _FakeClipboardClient();
        final platform = _FakeClipboardPlatform();

        await tester.pumpWidget(
          buildApp(
            calcClient: calcClient,
            clipClient: clipClient,
            platform: platform,
          ),
        );

        await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        await tester.sendKeyDownEvent(LogicalKeyboardKey.digit5);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.digit5);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        await tester.pumpAndSettle();

        await tester.enterText(find.byType(TextFormField), '100 Mbps to MB/s');
        await tester.pump(const Duration(milliseconds: 40));
        await tester.pumpAndSettle();

        expect(find.text('12.5 MB/s', findRichText: true), findsWidgets);

        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pumpAndSettle();
        expect(find.text('11.92 MiB/s', findRichText: true), findsWidgets);

        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await tester.pumpAndSettle();

        expect(clipClient.publishedTexts, ['11.92 MiB/s']);
        final commitCall = calcClient.calls
            .where((c) => c.$1 == 'commit')
            .single;
        expect(commitCall.$2['formatId'], 'iec');
      },
    );

    testWidgets('=expr in All enters Calc with expr preserved', (tester) async {
      final calcClient = _FakeCalculatorClient((op, fields) async {
        if (op == 'preview') {
          return _unitPreview(fields['revision'] as int?);
        }
        return _envelope({'entries': const []});
      });
      await tester.pumpWidget(
        buildApp(
          calcClient: calcClient,
          clipClient: _FakeClipboardClient(),
          platform: _FakeClipboardPlatform(),
        ),
      );

      await tester.enterText(find.byType(TextFormField), '=100 Mbps to MB/s');
      await tester.pump(const Duration(milliseconds: 40));
      await tester.pumpAndSettle();

      expect(find.text('12.5 MB/s', findRichText: true), findsWidgets);
      expect(
        calcClient.calls.firstWhere((call) => call.$1 == 'preview').$2['query'],
        '100 Mbps to MB/s',
      );
      expect(
        tester
            .widget<TextFormField>(find.byType(TextFormField))
            .controller
            ?.text,
        '100 Mbps to MB/s',
      );
    });

    testWidgets('chemistry prompts for molar mass and copies the conversion', (
      tester,
    ) async {
      const query = '372.45 pmol/L to pg/ml';
      const hint = 'Add a chemical formula or molar mass';
      const answer = '67.0991022 pg/mL';
      const molarMass = 'C6H12O6 · 180.156 g/mol';
      final calcClient = _FakeCalculatorClient((op, fields) async {
        if (op != 'preview') return _envelope({'entries': const []});
        final revision = fields['revision'] as int?;
        if (fields['query'] == '$query using C6H12O6') {
          return _envelope({
            'status': 'ready',
            'primaryCandidateId': 'chemistry',
            'candidates': [
              {
                'id': 'chemistry',
                'kind': 'chemistry_conversion',
                'normalized': '$query using C6H12O6',
                'assumptions': [molarMass],
                'value': {
                  'tag': 'quantity',
                  'display': 'approx. $answer',
                  'exact': false,
                },
                'formats': [
                  {
                    'id': 'primary',
                    'label': 'approx. $answer',
                    'copyText': answer,
                  },
                ],
              },
            ],
          }, revision: revision);
        }
        if (fields['scope'] == 'all') {
          return _envelope({'status': 'empty'}, revision: revision);
        }
        return _envelope({
          'status': 'invalid',
          'diagnostics': [
            {
              'code': 'molar_mass_required',
              'severity': 'error',
              'parameters': {'message': 'A molar mass is required.'},
              'suggestions': [
                {
                  'id': 'using',
                  'label': hint,
                  'replacement': {
                    'startUtf16': query.length,
                    'endUtf16': query.length,
                    'text': ' using ',
                  },
                },
              ],
            },
          ],
        }, revision: revision);
      });
      final clipboard = _FakeClipboardClient();
      await tester.pumpWidget(
        buildApp(
          calcClient: calcClient,
          clipClient: clipboard,
          platform: _FakeClipboardPlatform(),
        ),
      );

      final field = find.byType(TextFormField);
      await tester.enterText(field, query);
      await tester.pumpAndSettle();
      expect(find.text(hint, findRichText: true), findsNothing);
      expect(find.text('approx. $answer'), findsNothing);

      await tester.enterText(field, '$query using C6H12O6');
      await tester.pumpAndSettle();
      expect(find.text('approx. $answer'), findsOneWidget);

      await tester.enterText(field, '=$query');
      await tester.pumpAndSettle();
      await tester.pump(const Duration(milliseconds: 600));
      expect(find.text('A molar mass is required.'), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();
      expect(
        tester.widget<TextFormField>(field).controller?.text,
        '$query using ',
      );

      await tester.enterText(field, '$query using C6H12O6');
      await tester.pumpAndSettle();
      expect(find.text('Approximate'), findsOneWidget);
      expect(find.text(molarMass), findsOneWidget);
      expect(find.text(answer, findRichText: true), findsOneWidget);
      await tester.sendKeyEvent(LogicalKeyboardKey.enter);
      await tester.pumpAndSettle();
      expect(clipboard.publishedTexts, [answer]);
    });

    testWidgets('All inline shows 1920*1080 and Tab switches to Calc', (
      tester,
    ) async {
      final calcClient = _FakeCalculatorClient((op, fields) async {
        if (op == 'preview') {
          return _envelope({
            'status': 'ready',
            'primaryCandidateId': 'c1',
            'candidates': [
              {
                'id': 'c1',
                'kind': 'math',
                'normalized': '1920 × 1080',
                'interpretation': '1920 × 1080',
                'value': {'tag': 'integer', 'display': '2073600'},
                'formats': [
                  {'id': 'dec', 'label': '2073600', 'copyText': '2073600'},
                ],
              },
            ],
            'diagnostics': const [],
            'completions': const [],
          }, revision: fields['revision'] as int?);
        }
        return _envelope({'entries': const []});
      });

      await tester.pumpWidget(
        buildApp(
          calcClient: calcClient,
          clipClient: _FakeClipboardClient(),
          platform: _FakeClipboardPlatform(),
        ),
      );

      await tester.enterText(
        find.byType(TextFormField),
        'wallpaper-1920x1080.png',
      );
      await tester.pump(const Duration(milliseconds: 40));
      expect(calcClient.calls.where((c) => c.$1 == 'preview'), isEmpty);

      await tester.enterText(find.byType(TextFormField), '1920*1080');
      await tester.pump(const Duration(milliseconds: 40));
      await tester.pumpAndSettle();

      expect(find.text('Open in Calc'), findsWidgets);
      expect(find.text('2,073,600', findRichText: true), findsOneWidget);

      await tester.sendKeyEvent(LogicalKeyboardKey.tab);
      await tester.pumpAndSettle();

      expect(find.text('Open in Calc'), findsNothing);
      expect(find.text('2,073,600', findRichText: true), findsWidgets);
    });

    testWidgets(
      'float format selection updates bits and copies the selected value',
      (tester) async {
        final calcClient = _FakeCalculatorClient((op, fields) async {
          if (op != 'preview') return _envelope({'entries': const []});
          return _envelope({
            'status': 'ready',
            'candidates': [
              {
                'id': 'float',
                'kind': 'float_format',
                'normalized': '0.1 as bf16',
                'value': {
                  'tag': 'float_bits',
                  'format': 'bf16',
                  'inputDecimal': '0.1',
                  'bitsHex': '0x3DCD',
                  'storedDecimal': '0.10009765625',
                  'signBit': '0',
                  'exponentBits': '01111011',
                  'fractionBits': '1001101',
                  'alternatives': [
                    {
                      'format': 'fp32',
                      'bitsHex': '0x3DCCCCCD',
                      'storedDecimal': '0.10000000149',
                      'signBit': '0',
                      'exponentBits': '01111011',
                      'fractionBits': '10011001100110011001101',
                    },
                  ],
                },
                'formats': [
                  {'id': 'bf16', 'label': 'BF16', 'copyText': '0x3DCD'},
                  {'id': 'fp32', 'label': 'FP32', 'copyText': '0x3DCCCCCD'},
                ],
              },
            ],
          }, revision: fields['revision'] as int?);
        });
        final clipboard = _FakeClipboardClient();
        await tester.pumpWidget(
          buildApp(
            calcClient: calcClient,
            clipClient: clipboard,
            platform: _FakeClipboardPlatform(),
          ),
        );
        await tester.enterText(find.byType(TextFormField), '=0.1 as bf16');
        await tester.pumpAndSettle();
        expect(find.text('1 · 8 · 7 bits'), findsOneWidget);
        await tester.sendKeyEvent(LogicalKeyboardKey.tab);
        await tester.pumpAndSettle();
        expect(find.text('1 · 8 · 23 bits'), findsOneWidget);
        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await tester.pumpAndSettle();
        expect(clipboard.publishedTexts, ['0.10000000149']);
      },
    );

    testWidgets('Ctrl+B cycles programmer bit width and sends settings', (
      tester,
    ) async {
      final widths = <int>[];
      final calcClient = _FakeCalculatorClient((op, fields) async {
        if (op == 'preview') {
          final settings = fields['settings'] as Map<String, Object?>;
          widths.add(settings['bitWidth'] as int);
          return _envelope({
            'status': 'ready',
            'primaryCandidateId': 'c1',
            'candidates': [
              {
                'id': 'c1',
                'kind': 'programmer',
                'normalized': '0xDBEE',
                'interpretation': 'unsigned ${settings['bitWidth']}-bit',
                'value': {'tag': 'integer', 'display': '0xDBEE'},
                'formats': [
                  {'id': 'hex', 'label': '0xDBEE', 'copyText': '0xDBEE'},
                  {'id': 'dec', 'label': '56,302', 'copyText': '56302'},
                ],
              },
            ],
            'diagnostics': const [],
            'completions': const [],
          }, revision: fields['revision'] as int?);
        }
        return _envelope({'entries': const []});
      });

      await tester.pumpWidget(
        buildApp(
          calcClient: calcClient,
          clipClient: _FakeClipboardClient(),
          platform: _FakeClipboardPlatform(),
        ),
      );

      await tester.enterText(find.byType(TextFormField), '=0xDEAD_BEEF >> 4');
      await tester.pumpAndSettle();
      expect(widths, [32]);

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.keyB);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.keyB);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();

      expect(widths, [32, 64]);
      expect(find.text('bitwise · u64'), findsOneWidget);
    });
  });
}
