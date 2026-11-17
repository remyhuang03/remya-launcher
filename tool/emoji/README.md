# Emoji catalog

Run `python tool/emoji/build.py` to regenerate the Flutter assets. The
generator requires Python 3.10 or later and caches verified source archives
under `$XDG_CACHE_HOME/remya-emoji-sources` (default `~/.cache`). A cached
source with a mismatched hash is rejected. The app reads the checked-in
catalog and works offline.

- `python tool/emoji/build.py --check` regenerates and compares every asset.
- `python tool/emoji/build.py --offline-check` checks the tracked bundle
  against its manifest without downloading or generating anything.
- Add `--audit /tmp/remya-emoji-audit.json` to a regeneration or full check
  to inspect exact CLDR deltas, alias provenance by locale and RGI ID, and
  excluded non-RGI source entries. Ambiguous shortcodes remain searchable
  for each applicable emoji.

For an update, pin the new versions, URLs and SHA-256 values in
[sources.lock.json](sources.lock.json), regenerate and review the manifest
and audit report. Change the version-specific acceptance checks and search
fixtures when introducing a new Emoji standard version. The MIT and Unicode
license texts are regenerated from the pinned packages.

The Unicode test file defines the exact payload and order. The CLDR full
packages provide resolved base and derived annotations for seven languages,
with fallbacks from Emojibase and the Unicode test file. For 18.0 the final
CLDR data supplies all 3,963 short names in each locale. The audit separates
added keywords (including derived annotations missing tags in Emojibase)
from changed keyword sets. Project aliases are manually reviewed in
[aliases.json](aliases.json), including common Simplified and Traditional
Chinese expressions and a small set of pinyin and romaji readings.

## Search implementation decision

The released `emoji_picker_i18n 1.1.0` package exposes `EmojiSearch` with
caller-provided data. Its bundled catalog searches English, Chinese and
Japanese and resolves 👩‍💻, but lacks all 19 sequences introduced in
Emoji 18. Keep the library's search and ranking and replace its data with
the generated catalog. Each fully qualified sequence is a searchable
record, and the adapter resolves qualifiers and variant families from
the official data.

| Gate | Evidence |
| --- | --- |
| G0 — API | The release-package probe in `test/emoji_search_test.dart` exercises the public constructor, multilingual queries and reverse lookup. |
| G1 — Data | The generator checks the exact set of 3,963 fully qualified sequences and 1,272 qualification spellings. The Dart tests exercise exact lookup across the full catalog. |
| G2 — Relevance | The 170-case golden corpus achieved Recall@10 of 100% and MRR@10 of 0.985. |
| G3 — Performance | On Arch Linux with an Intel Core Ultra 7 255H and Flutter 3.50.0/Dart 3.14.0, the focused Flutter test run measured 4.36/7.47/11.34 ms warm P50/P95/P99 for 160 samples. Index load took 357 ms in the test VM, exceeding the proposed 150 ms budget. Decoding and indexing run in an isolate after opening the Emoji scope. The catalog is about 460 KB gzip-compressed. |
| G4 — Maintenance | The lockfile pins the package. The generator pins data archives and licenses by hash, and CI verifies the bundled files offline. |
