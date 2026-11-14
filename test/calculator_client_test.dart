import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:remya_launcher/features/calculator/calculator_client.dart';

void main() {
  group('CalculatorClient sibling lookup and real process execution', () {
    late Directory tempDir;

    setUp(() async {
      tempDir = await Directory.systemTemp.createTemp('calc-client-test-');
    });

    tearDown(() async {
      if (await tempDir.exists()) {
        await tempDir.delete(recursive: true);
      }
    });

    test('rejects missing sibling binary asynchronously', () async {
      final launcher = File('${tempDir.path}/remya_launcher');
      await launcher.writeAsString('stub');

      final client = CalculatorClient(launcherExecutablePath: launcher.path);
      addTearDown(client.close);

      await expectLater(
        () => client.request('preview', {'query': '1+1', 'revision': 1}),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.binaryUnavailable,
          ),
        ),
      );
    });

    test('rejects directory disguised as sibling binary', () async {
      final launcher = File('${tempDir.path}/remya_launcher');
      await launcher.writeAsString('stub');
      await Directory('${tempDir.path}/${CalculatorClient.sidecarBinaryName}')
          .create();

      final client = CalculatorClient(launcherExecutablePath: launcher.path);
      addTearDown(client.close);

      await expectLater(
        () => client.request('hello'),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.invalidExecutable,
          ),
        ),
      );
    });

    test('rejects non-executable sibling binary', () async {
      final launcher = File('${tempDir.path}/remya_launcher');
      await launcher.writeAsString('stub');
      final sidecar = File(
        '${tempDir.path}/${CalculatorClient.sidecarBinaryName}',
      );
      await sidecar.writeAsString('#!/bin/sh\nexit 0\n');
      await Process.run('chmod', ['0644', sidecar.path]);

      final client = CalculatorClient(launcherExecutablePath: launcher.path);
      addTearDown(client.close);

      await expectLater(
        () => client.request('hello'),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.invalidExecutable,
          ),
        ),
      );
    });

    test('rejects symlink escaping the bundle directory', () async {
      final bundleDir = await Directory('${tempDir.path}/bundle').create();
      final outsideDir = await Directory('${tempDir.path}/outside').create();
      final launcher = File('${bundleDir.path}/remya_launcher');
      await launcher.writeAsString('stub');

      final outsideBin = File(
        '${outsideDir.path}/${CalculatorClient.sidecarBinaryName}',
      );
      await outsideBin.writeAsString('#!/bin/sh\nexit 0\n');
      await Process.run('chmod', ['0755', outsideBin.path]);

      await Link('${bundleDir.path}/${CalculatorClient.sidecarBinaryName}')
          .create(outsideBin.path);

      final client = CalculatorClient(launcherExecutablePath: launcher.path);
      addTearDown(client.close);

      await expectLater(
        () => client.request('hello'),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.invalidExecutable,
          ),
        ),
      );
    });

    test(
      'runs real sibling script with controlled env and clean shutdown',
      () async {
        final bundleDir = await Directory('${tempDir.path}/bundle').create();
        final launcher = File('${bundleDir.path}/remya_launcher');
        await launcher.writeAsString('stub');

        final sidecar = File(
          '${bundleDir.path}/${CalculatorClient.sidecarBinaryName}',
        );
        await sidecar.writeAsString('''#!/usr/bin/python3
import json, os, sys
for line in sys.stdin:
    request = json.loads(line)
    operation = request["op"]
    response = {"v": 1, "id": request["id"], "ok": True, "data": {}}
    if operation == "hello":
        response["data"] = {
            "protocolVersion": 1,
            "envKeys": sorted(os.environ),
            "tz": os.environ.get("TZ"),
            "xdgState": os.environ.get("XDG_STATE_HOME"),
        }
    elif operation == "preview":
        response["revision"] = request["revision"]
        response["data"] = {"status": "ready", "query": request["query"]}
    print(json.dumps(response), flush=True)
    if operation == "shutdown":
        break
''');
        await Process.run('chmod', ['0755', sidecar.path]);

        final client = CalculatorClient(
          launcherExecutablePath: launcher.path,
          environment: {
            'HOME': '/home/tester',
            'XDG_STATE_HOME': '/tmp/state-home',
            'XDG_RUNTIME_DIR': 'relative/unsafe',
            'TZ': 'Asia/Shanghai',
            'LD_PRELOAD': '/tmp/evil.so',
            'SECRET_TOKEN': 'do-not-pass',
          },
        );
        addTearDown(client.close);

        final hello = await client.request('hello');
        expect(hello['ok'], isTrue);
        final data = hello['data']! as Map<String, Object?>;
        expect(data['tz'], 'Asia/Shanghai');
        expect(data['xdgState'], '/tmp/state-home');
        final envKeys = data['envKeys']! as List<Object?>;
        expect(envKeys, contains('HOME'));
        expect(envKeys, contains('TZ'));
        expect(envKeys, contains('XDG_STATE_HOME'));
        expect(envKeys.contains('XDG_RUNTIME_DIR'), isFalse);
        expect(envKeys.contains('LD_PRELOAD'), isFalse);
        expect(envKeys.contains('SECRET_TOKEN'), isFalse);

        final preview = await client.request('preview', {
          'query': '1+1',
          'revision': 7,
          'scope': 'calc',
        });
        expect(preview['ok'], isTrue);
        expect(preview['revision'], 7);

        await client.close();
        expect(client.state, CalculatorClientState.closed);
      },
    );
  });

  group('CalculatorClient protocol, framing, and stale ID guards', () {
    test('returns ok:false envelopes without throwing', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient();
      addTearDown(client.close);

      harness.onRequest = (proc, req) {
        if (req['op'] == 'preview') {
          proc.sendJson({
            'v': 1,
            'id': req['id'],
            'ok': false,
            'revision': req['revision'],
            'error': {'code': 'invalid_syntax'},
          });
        }
      };

      final res = await client.request('preview', {
        'query': '1+',
        'revision': 3,
      });
      expect(res['ok'], isFalse);
      expect((res['error']! as Map<String, Object?>)['code'], 'invalid_syntax');
    });

    test('serializes concurrent cold-start requests to one spawn', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient();
      addTearDown(client.close);

      harness.onRequest = (proc, req) {
        if (req['op'] == 'preview') {
          proc.sendJson({
            'v': 1,
            'id': req['id'],
            'ok': true,
            'revision': req['revision'],
            'data': {'q': req['query']},
          });
        }
      };

      final results = await Future.wait([
        client.request('hello'),
        client.request('preview', {'query': '1+1', 'revision': 1}),
        client.request('preview', {'query': '2+2', 'revision': 2}),
      ]);

      expect(harness.spawnCount, 1);
      expect(results[0]['ok'], isTrue);
      expect(results[1]['revision'], 1);
      expect(results[2]['revision'], 2);
      expect(harness.processes.single.helloCount, 1);
    });

    test(
      'assembles split UTF-8 chunks and ignores stale/duplicate IDs',
      () async {
        final harness = _FakeSidecarHarness();
        final client = harness.createClient();
        addTearDown(client.close);

        harness.onRequest = (proc, req) {
          if (req['op'] == 'preview') {
            // Emit stale/unmatched IDs before the real response.
            proc.sendJson({'v': 1, 'id': 'stale-999', 'ok': true, 'data': {}});

            final payload = utf8.encode(
              '${jsonEncode({
                'v': 1,
                'id': req['id'],
                'ok': true,
                'revision': req['revision'],
                'data': {'text': 'π ≈ 3.14159 🚀'},
              })}\n',
            );
            // Split right in the middle of multi-byte UTF-8 sequence.
            for (var i = 0; i < payload.length; i += 3) {
              final end = i + 3 > payload.length ? payload.length : i + 3;
              proc.emitStdoutBytes(payload.sublist(i, end));
            }
            // Duplicate response for the same ID should be safely ignored.
            proc.sendJson({
              'v': 1,
              'id': req['id'],
              'ok': true,
              'data': {'dup': true},
            });
          }
        };

        final res = await client.request('preview', {
          'query': 'pi',
          'revision': 1,
        });
        expect(
          (res['data']! as Map<String, Object?>)['text'],
          'π ≈ 3.14159 🚀',
        );
        expect(client.decodeErrorCount, 0);
      },
    );

    test(
      'handles out-of-order responses and rejects mismatched revision',
      () async {
        final harness = _FakeSidecarHarness();
        final client = harness.createClient();
        addTearDown(client.close);

        final held = <Map<String, Object?>>[];
        harness.onRequest = (proc, req) {
          if (req['op'] == 'preview') {
            held.add(req);
            if (held.length == 2) {
              // Reply to second request first, then first request with bad rev.
              proc.sendJson({
                'v': 1,
                'id': held[1]['id'],
                'ok': true,
                'revision': held[1]['revision'],
                'data': {'n': 2},
              });
              proc.sendJson({
                'v': 1,
                'id': held[0]['id'],
                'ok': true,
                'revision': 999,
                'data': {'n': 1},
              });
            }
          }
        };

        final f1 = client.request('preview', {'query': 'a', 'revision': 1});
        final f2 = client.request('preview', {'query': 'b', 'revision': 2});

        final r2 = await f2;
        expect((r2['data']! as Map<String, Object?>)['n'], 2);

        await expectLater(
          () => f1,
          throwsA(
            isA<CalculatorTransportException>().having(
              (e) => e.code,
              'code',
              CalculatorTransportErrorCode.protocolError,
            ),
          ),
        );
      },
    );

    test('rejects unsupported protocol version permanently', () async {
      final harness = _FakeSidecarHarness(protocolVersion: 2);
      final client = harness.createClient();
      addTearDown(client.close);

      await expectLater(
        () => client.request('preview', {'query': '1+1', 'revision': 1}),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.unsupportedProtocol,
          ),
        ),
      );

      expect(client.state, CalculatorClientState.disabled);
      expect(harness.processes.single.killSignals, isNotEmpty);

      await expectLater(
        () => client.request('preview', {'query': '1+1', 'revision': 2}),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.unsupportedProtocol,
          ),
        ),
      );
      expect(harness.spawnCount, 1);
    });

    test('enforces outbound and inbound maxFrameBytes limits', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient(maxFrameBytes: 256);
      addTearDown(client.close);

      harness.onRequest = (proc, req) {
        if (req['op'] == 'preview' && req['query'] == 'huge-reply') {
          proc.sendJson({
            'v': 1,
            'id': req['id'],
            'ok': true,
            'data': {'blob': 'x' * 400},
          });
        } else if (req['op'] == 'preview') {
          proc.sendJson({'v': 1, 'id': req['id'], 'ok': true, 'data': {}});
        }
      };

      await expectLater(
        () => client.request('preview', {'query': 'a' * 300, 'revision': 1}),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.frameTooLarge,
          ),
        ),
      );
      expect(harness.spawnCount, 0);

      await expectLater(
        () => client.request('preview', {'query': 'huge-reply', 'revision': 2}),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.frameTooLarge,
          ),
        ),
      );
      expect(client.decodeErrorCount, 1);
      expect(harness.processes.single.killSignals, isNotEmpty);
    });

    test('rejects unexpected stdout text and invalid UTF-8', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient();
      addTearDown(client.close);

      var mode = 'banner';
      harness.onRequest = (proc, req) {
        if (req['op'] == 'preview') {
          if (mode == 'banner') {
            mode = 'bad_utf8';
            proc.emitStdoutBytes(utf8.encode('Starting calculator...\n'));
          } else {
            proc.emitStdoutBytes([0xFF, 0xFE, 0x0A]);
          }
        }
      };

      await expectLater(
        () => client.request('preview', {'query': '1+1', 'revision': 1}),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.protocolError,
          ),
        ),
      );
      expect(client.decodeErrorCount, 1);

      await expectLater(
        () => client.request('preview', {'query': '1+1', 'revision': 2}),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.protocolError,
          ),
        ),
      );
      expect(client.decodeErrorCount, 2);
    });
  });

  group('CalculatorClient crash recovery, retries, timeouts, and close', () {
    test(
      'retries safe preview once on crash, including only latest preview',
      () async {
        final harness = _FakeSidecarHarness();
        final client = harness.createClient();
        addTearDown(client.close);

        var previewsSeen = 0;
        harness.onRequest = (proc, req) {
          if (req['op'] == 'preview') {
            previewsSeen++;
            if (previewsSeen <= 2 && harness.spawnCount == 1) {
              if (previewsSeen == 2) {
                proc.crash(139);
              }
              return;
            }
            proc.sendJson({
              'v': 1,
              'id': req['id'],
              'ok': true,
              'revision': req['revision'],
              'data': {'query': req['query']},
            });
          }
        };

        final f1 = client.request('preview', {'query': '1+', 'revision': 1});
        final f2 = client.request('preview', {'query': '1+2', 'revision': 2});

        await expectLater(
          () => f1,
          throwsA(isA<CalculatorTransportException>()),
        );
        final r2 = await f2;
        expect(r2['revision'], 2);
        expect((r2['data']! as Map<String, Object?>)['query'], '1+2');
        expect(harness.spawnCount, 2);
        expect(client.crashCount, 1);
        expect(client.restartCount, 1);
      },
    );

    test('never retries mutations after process crash', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient();
      addTearDown(client.close);

      var commitCount = 0;
      harness.onRequest = (proc, req) {
        if (req['op'] == 'commit') {
          commitCount++;
          proc.crash(1);
        } else if (req['op'] == 'preview') {
          proc.sendJson({'v': 1, 'id': req['id'], 'ok': true, 'data': {}});
        }
      };

      await expectLater(
        () => client.request('commit', {
          'candidateId': 'c1',
          'revision': 1,
          'idempotencyKey': 'k1',
        }),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.processCrashed,
          ),
        ),
      );
      expect(commitCount, 1);
      expect(harness.spawnCount, 1);

      // Next request lazily restarts cleanly.
      final ok = await client.request('preview', {
        'query': '1+1',
        'revision': 2,
      });
      expect(ok['ok'], isTrue);
      expect(harness.spawnCount, 2);
    });

    test(
      'kills hung process on hard timeout without retrying hung query',
      () async {
        final harness = _FakeSidecarHarness();
        final client = harness.createClient(
          requestTimeout: const Duration(milliseconds: 40),
        );
        addTearDown(client.close);

        var attempts = 0;
        harness.onRequest = (proc, req) {
          if (req['op'] == 'preview') {
            attempts++;
            if (req['query'] != 'hang') {
              proc.sendJson({'v': 1, 'id': req['id'], 'ok': true, 'data': {}});
            }
          }
        };

        await expectLater(
          () => client.request('preview', {'query': 'hang', 'revision': 1}),
          throwsA(
            isA<CalculatorTransportException>().having(
              (e) => e.code,
              'code',
              CalculatorTransportErrorCode.timeout,
            ),
          ),
        );
        expect(attempts, 1);
        expect(
          harness.processes.single.killSignals,
          contains(ProcessSignal.sigkill),
        );

        final recovered = await client.request('preview', {
          'query': '2+2',
          'revision': 2,
        });
        expect(recovered['ok'], isTrue);
        expect(harness.spawnCount, 2);
      },
    );

    test(
      'applies capped exponential backoff and disables after repeated crashes',
      () async {
        final harness = _FakeSidecarHarness();
        final recordedSleeps = <Duration>[];
        var now = DateTime.utc(2026, 1, 1);

        final client = harness.createClient(
          initialBackoff: const Duration(milliseconds: 50),
          maxBackoff: const Duration(milliseconds: 120),
          maxConsecutiveFailures: 5,
          clock: () => now,
          sleeper: (delay) async {
            recordedSleeps.add(delay);
            now = now.add(delay);
          },
        );
        addTearDown(client.close);

        harness.onRequest = (proc, req) {
          if (req['op'] == 'commit') {
            proc.crash(1);
          }
        };

        for (var i = 0; i < 4; i++) {
          await expectLater(
            () => client.request('commit', {
              'candidateId': 'c1',
              'revision': i,
              'idempotencyKey': 'k$i',
            }),
            throwsA(isA<CalculatorTransportException>()),
          );
        }

        // 5th consecutive failure transitions client to disabled.
        await expectLater(
          () => client.request('commit', {
            'candidateId': 'c1',
            'revision': 4,
            'idempotencyKey': 'k4',
          }),
          throwsA(
            isA<CalculatorTransportException>().having(
              (e) => e.code,
              'code',
              CalculatorTransportErrorCode.disabled,
            ),
          ),
        );

        expect(client.state, CalculatorClientState.disabled);
        expect(recordedSleeps, [
          const Duration(milliseconds: 50),
          const Duration(milliseconds: 100),
          const Duration(milliseconds: 120),
        ]);

        // Subsequent requests fail immediately without spawning.
        final spawnsBefore = harness.spawnCount;
        await expectLater(
          () => client.request('preview', {'query': '1+1', 'revision': 5}),
          throwsA(
            isA<CalculatorTransportException>().having(
              (e) => e.code,
              'code',
              CalculatorTransportErrorCode.disabled,
            ),
          ),
        );
        expect(harness.spawnCount, spawnsBefore);
      },
    );

    test(
      'close is permanent, idempotent, and interrupts pending requests',
      () async {
        final harness = _FakeSidecarHarness();
        final client = harness.createClient(
          shutdownTimeout: const Duration(milliseconds: 50),
        );

        harness.onRequest = (proc, req) {
          if (req['op'] == 'shutdown') {
            proc.sendJson({'v': 1, 'id': req['id'], 'ok': true, 'data': {}});
            proc.crash(0);
          }
        };

        // Trigger startup so sidecar is ready, then leave a request pending.
        await client.request('hello');
        final pending = client.request('preview', {
          'query': '1+1',
          'revision': 1,
        });

        final expectation = expectLater(
          () => pending,
          throwsA(
            isA<CalculatorTransportException>().having(
              (e) => e.code,
              'code',
              CalculatorTransportErrorCode.closed,
            ),
          ),
        );

        await client.close();
        await client.close();
        await expectation;
        expect(client.state, CalculatorClientState.closed);

        await expectLater(
          () => client.request('preview', {'query': '2+2', 'revision': 2}),
          throwsA(
            isA<CalculatorTransportException>().having(
              (e) => e.code,
              'code',
              CalculatorTransportErrorCode.closed,
            ),
          ),
        );
      },
    );

    test('handles shutdown request when stopped and when running', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient();
      addTearDown(client.close);

      harness.onRequest = (proc, req) {
        if (req['op'] == 'shutdown') {
          proc.sendJson({'v': 1, 'id': req['id'], 'ok': true, 'data': {}});
          proc.crash(0);
        } else if (req['op'] == 'preview') {
          proc.sendJson({'v': 1, 'id': req['id'], 'ok': true, 'data': {}});
        }
      };

      final coldShutdown = await client.request('shutdown');
      expect(coldShutdown['ok'], isTrue);
      expect(harness.spawnCount, 0);

      await client.request('preview', {'query': '1+1', 'revision': 1});
      expect(harness.spawnCount, 1);
      expect(client.state, CalculatorClientState.ready);

      final warmShutdown = await client.request('shutdown');
      expect(warmShutdown['ok'], isTrue);
      expect(client.state, CalculatorClientState.stopped);

      await client.request('preview', {'query': '2+2', 'revision': 2});
      expect(harness.spawnCount, 2);
    });

    test('retries preview at most once on double crash and resets failure streak on success', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient(maxConsecutiveFailures: 3);
      addTearDown(client.close);

      var shouldCrash = true;
      harness.onRequest = (proc, req) {
        if (req['op'] == 'preview') {
          if (shouldCrash) {
            proc.crash(1);
          } else {
            proc.sendJson({'v': 1, 'id': req['id'], 'ok': true, 'data': {}});
          }
        }
      };

      await expectLater(
        () => client.request('preview', {'query': 'boom', 'revision': 1}),
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.processCrashed,
          ),
        ),
      );
      expect(harness.spawnCount, 2);

      shouldCrash = false;
      final ok = await client.request('preview', {
        'query': '1+1',
        'revision': 2,
      });
      expect(ok['ok'], isTrue);
      expect(client.state, CalculatorClientState.ready);
    });

    test('close interrupts request waiting in backoff without delay', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient(
        initialBackoff: const Duration(seconds: 30),
        maxBackoff: const Duration(seconds: 30),
      );

      harness.onRequest = (proc, req) {
        if (req['op'] == 'commit') {
          proc.crash(1);
        }
      };

      for (var i = 0; i < 2; i++) {
        await expectLater(
          () => client.request('commit', {
            'candidateId': 'c1',
            'revision': i,
            'idempotencyKey': 'k$i',
          }),
          throwsA(isA<CalculatorTransportException>()),
        );
      }

      final waiting = client.request('preview', {
        'query': '1+1',
        'revision': 9,
      });
      final expectation = expectLater(
        () => waiting,
        throwsA(
          isA<CalculatorTransportException>().having(
            (e) => e.code,
            'code',
            CalculatorTransportErrorCode.closed,
          ),
        ),
      );

      await Future<void>.delayed(const Duration(milliseconds: 5));
      await client.close();
      await expectation;
    });

    test('drains large stderr output without blocking responses', () async {
      final harness = _FakeSidecarHarness();
      final client = harness.createClient();
      addTearDown(client.close);

      harness.onRequest = (proc, req) {
        if (req['op'] == 'preview') {
          proc.emitStderrBytes(List<int>.filled(32 * 1024, 65));
          proc.sendJson({'v': 1, 'id': req['id'], 'ok': true, 'data': {}});
        }
      };

      final res = await client.request('preview', {
        'query': '1+1',
        'revision': 1,
      });
      expect(res['ok'], isTrue);
    });
  });
}

class _FakeSidecarHarness {
  _FakeSidecarHarness({this.protocolVersion = 1});

  final int protocolVersion;
  final List<_FakeProcess> processes = [];
  int spawnCount = 0;

  void Function(_FakeProcess process, Map<String, Object?> request)? onRequest;

  CalculatorClient createClient({
    Duration handshakeTimeout = const Duration(seconds: 2),
    Duration requestTimeout = const Duration(seconds: 1),
    Duration shutdownTimeout = const Duration(milliseconds: 200),
    Duration initialBackoff = const Duration(milliseconds: 10),
    Duration maxBackoff = const Duration(milliseconds: 100),
    int maxConsecutiveFailures = 5,
    int maxFrameBytes = CalculatorClient.defaultMaxFrameBytes,
    DateTime Function()? clock,
    Future<void> Function(Duration)? sleeper,
  }) => CalculatorClient(
    handshakeTimeout: handshakeTimeout,
    requestTimeout: requestTimeout,
    shutdownTimeout: shutdownTimeout,
    initialBackoff: initialBackoff,
    maxBackoff: maxBackoff,
    maxConsecutiveFailures: maxConsecutiveFailures,
    maxFrameBytes: maxFrameBytes,
    clock: clock,
    sleeper: sleeper,
    processStarter:
        (
          executable,
          arguments, {
          environment,
          includeParentEnvironment = true,
          runInShell = false,
          mode = ProcessStartMode.normal,
        }) async {
          expect(includeParentEnvironment, isFalse);
          expect(runInShell, isFalse);
          expect(mode, ProcessStartMode.normal);
          spawnCount++;
          final proc = _FakeProcess(
            pid: 1000 + spawnCount,
            onLine: (proc, line) {
              final req = jsonDecode(line) as Map<String, Object?>;
              if (req['op'] == 'hello') {
                proc.helloCount++;
                proc.sendJson({
                  'v': 1,
                  'id': req['id'],
                  'ok': true,
                  'data': {
                    'protocolVersion': protocolVersion,
                    'engine': 'fake',
                    'capabilities': ['math'],
                  },
                });
              }
              onRequest?.call(proc, req);
            },
          );
          processes.add(proc);
          return proc;
        },
  );
}

class _FakeProcess implements Process {
  _FakeProcess({required this.pid, required this.onLine}) {
    _stdin = _FakeStdin((bytes) {
      _stdinBuffer.addAll(bytes);
      while (true) {
        final idx = _stdinBuffer.indexOf(10);
        if (idx < 0) break;
        final lineBytes = _stdinBuffer.sublist(0, idx);
        _stdinBuffer.removeRange(0, idx + 1);
        onLine(this, utf8.decode(lineBytes));
      }
    });
  }

  @override
  final int pid;

  final void Function(_FakeProcess process, String line) onLine;
  final List<int> _stdinBuffer = [];
  final StreamController<List<int>> _stdoutController = StreamController();
  final StreamController<List<int>> _stderrController = StreamController();
  final Completer<int> _exitCompleter = Completer<int>();
  late final _FakeStdin _stdin;

  final List<ProcessSignal> killSignals = [];
  int helloCount = 0;

  @override
  IOSink get stdin => _stdin;

  @override
  Stream<List<int>> get stdout => _stdoutController.stream;

  @override
  Stream<List<int>> get stderr => _stderrController.stream;

  @override
  Future<int> get exitCode => _exitCompleter.future;

  void sendJson(Map<String, Object?> envelope) {
    emitStdoutBytes(utf8.encode('${jsonEncode(envelope)}\n'));
  }

  void emitStdoutBytes(List<int> bytes) {
    if (!_stdoutController.isClosed) {
      _stdoutController.add(bytes);
    }
  }

  void emitStderrBytes(List<int> bytes) {
    if (!_stderrController.isClosed) {
      _stderrController.add(bytes);
    }
  }

  void crash(int code) {
    if (!_stdoutController.isClosed) {
      unawaited(_stdoutController.close());
    }
    if (!_stderrController.isClosed) {
      unawaited(_stderrController.close());
    }
    if (!_exitCompleter.isCompleted) {
      _exitCompleter.complete(code);
    }
  }

  @override
  bool kill([ProcessSignal signal = ProcessSignal.sigterm]) {
    killSignals.add(signal);
    crash(-1);
    return true;
  }
}

class _FakeStdin implements IOSink {
  _FakeStdin(this.onBytes);

  final void Function(List<int> data) onBytes;
  final Completer<void> _doneCompleter = Completer<void>();

  @override
  Encoding encoding = utf8;

  @override
  void add(List<int> data) {
    if (!_doneCompleter.isCompleted) {
      onBytes(data);
    }
  }

  @override
  void addError(Object error, [StackTrace? stackTrace]) {}

  @override
  Future<void> addStream(Stream<List<int>> stream) async {
    await for (final chunk in stream) {
      add(chunk);
    }
  }

  @override
  Future<void> close() async {
    if (!_doneCompleter.isCompleted) {
      _doneCompleter.complete();
    }
  }

  @override
  Future<void> get done => _doneCompleter.future;

  @override
  Future<void> flush() async {}

  @override
  void write(Object? object) => add(encoding.encode(object.toString()));

  @override
  void writeAll(Iterable<dynamic> objects, [String separator = '']) =>
      write(objects.join(separator));

  @override
  void writeCharCode(int charCode) => add([charCode]);

  @override
  void writeln([Object? object = '']) => write('${object ?? ''}\n');
}
