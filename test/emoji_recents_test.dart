import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:remya_launcher/features/emoji/emoji_recents.dart';

void main() {
  test(
    'recents persist in use order, deduplicate and can be cleared',
    () async {
      final temp = await Directory.systemTemp.createTemp('emoji-recents-test-');
      addTearDown(() => temp.delete(recursive: true));
      final directory = Directory('${temp.path}/private');
      final first = EmojiRecents(directory: directory);
      final second = EmojiRecents(directory: directory);
      expect(await second.read(), isEmpty);

      await Future.wait([
        first.record('U+1F469 U+200D U+1F4BB'),
        second.record('U+1FAF9 U+1F3FD'),
        first.record('U+1F469 U+200D U+1F4BB'),
      ]);
      const expected = ['U+1F469 U+200D U+1F4BB', 'U+1FAF9 U+1F3FD'];
      expect(await second.read(), expected);
      final file = File('${directory.path}/emoji-recents.json');
      expect(jsonDecode(await file.readAsString()), expected);
      expect(() => first.record('private typed query'), throwsArgumentError);

      for (var i = 0; i < 31; i++) {
        await first.record(
          'U+1F6${i.toRadixString(16).padLeft(2, '0').toUpperCase()}',
        );
      }
      expect(await second.read(), hasLength(30));
      expect((await second.read()).first, 'U+1F61E');
      await second.clear();
      expect(await first.read(), isEmpty);
      expect(await file.exists(), isFalse);
    },
  );
}
