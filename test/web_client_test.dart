import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:remya_launcher/features/web/web_client.dart';

void main() {
  late Directory directory;
  late WebClient client;

  setUp(() async {
    directory = await Directory.systemTemp.createTemp('remya-web-client-');
    final helper = File('${directory.path}/remya-launcher-web');
    await helper.writeAsString(_sidecar);
    final chmod = await Process.run('chmod', ['700', helper.path]);
    expect(chmod.exitCode, 0);
    client = WebClient(executablePath: helper.path);
  });

  tearDown(() async {
    await client.close();
    await directory.delete(recursive: true);
  });

  test(
    'matches out-of-order replies to the original search requests',
    () async {
      expect(client.available, isTrue);
      final first = client.search('first');
      final second = client.search('second', engineId: 'github');
      final (firstResult, secondResult) = await (first, second).wait;

      expect(firstResult.input.query, 'first');
      expect(secondResult.input.engineId, 'github');
      expect(secondResult.groups.single.kind, 'search');
      expect(secondResult.groups.single.items.single.hostname, 'github.com');
      expect(secondResult.stale, isTrue);
    },
  );

  test('maps structured request errors without disconnecting', () async {
    await expectLater(
      client.setProfileEnabled('other', true),
      throwsA(
        isA<WebException>()
            .having((e) => e.code, 'code', 'PROFILE_NOT_FOUND')
            .having((e) => e.retryable, 'retryable', isFalse),
      ),
    );
    final profiles = await client.profiles();
    expect(profiles.single.profileId, 'sample');
    expect(profiles.single.enabled, isTrue);
  });

  test('rejects malformed frames and reconnects on a new request', () async {
    await expectLater(
      client.perform('open.default', url: 'https://example.com'),
      throwsA(
        isA<WebException>().having((e) => e.code, 'code', 'INVALID_RESPONSE'),
      ),
    );
    expect(await client.profiles(), hasLength(1));
    final attempts = File('${directory.path}/opens');
    expect(await attempts.readAsLines(), hasLength(1));
  });
}

const _sidecar = r'''#!/usr/bin/env python3
import json
from pathlib import Path
import sys
import uuid

assert sys.argv[1:] == ["--standalone"]
profile = {
    "profileId": "sample", "browserId": "firefox", "browserLabel": "Firefox",
    "label": "Example", "enabled": True, "status": "ready"
}

def reply(request, **fields):
    print(json.dumps({"id": request["id"], **fields}), flush=True)

def search(request):
    return {
        "input": {"query": request["query"], "engineId": request.get("engineId")},
        "groups": [{"kind": "search", "items": [
            {"title": "Search GitHub", "url": "https://github.com/search"}
        ]}],
        "sync": {"stale": True}
    }

first = None
for line in sys.stdin:
    request = json.loads(line)
    match request["op"]:
        case "web.search":
            if request["query"] == "first":
                first = request
                continue
            reply(request, data=search(request))
            reply(first, data=search(first))
        case "web.profiles.setEnabled":
            assert request["profileId"] == "other" and request["enabled"]
            reply(request, error={"code": "PROFILE_NOT_FOUND",
                                  "message": "Profile unavailable.",
                                  "retryable": False})
        case "web.profiles.list":
            reply(request, data=[profile])
        case "web.perform":
            assert request["action"] == "open.default"
            assert request["target"] == {"url": "https://example.com"}
            assert uuid.UUID(request["eventId"]).version == 4
            with Path(__file__).with_name("opens").open("a") as log:
                log.write("open\n")
            print("invalid frame", flush=True)
''';
