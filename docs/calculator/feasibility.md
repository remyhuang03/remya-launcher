# Calculator backend feasibility

Remya Launcher starts `remya-launcher-calculator` from the same bundle directory when Calc opens or an All query qualifies. The sidecar uses version 1 NDJSON and owns evaluation and SQLite state. Debug and release builds use the same protocol and carry their own sidecar and Rust notices. `hello` reports the protocol and enabled capabilities without waiting for SQLite or a network request.

## Engine probes

| Input or API | Observed with fend-core 1.5.9 | Integration decision |
| --- | --- | --- |
| `3pi sqrt 2` | Produces a function expression | Insert explicit multiplication before the adjacent square-root call. `3pi * sqrt 2` yields `approx. 13.3286488145`. |
| `100 Mbps to MB/s` | Formats `12.5 MB / s` | Compute the quantity, dimension and alternatives independently using exact rational scales. |
| `0xDEAD_BEEF >> 4 & 0xFFFF` | Formats `0xdbee` | Define precedence, width and signedness in the integer adapter. |
| `0xff+1` | Formats `0x100` | Apply explicit selected-width overflow policy in the integer adapter. |
| `20% of 80` | Formats `16` | Use the original engine display. |
| `2(3+4)` | Formats `14` | Independently type the bounded arithmetic subset. |
| `x = 2` | Returns `2` even in preview | Run previews with an isolated context. Save persistent definitions via `saveVariable`. |
| `roll d6` | Preview is empty and no random generator is available | Leave RNG unconfigured in the sidecar. |
| `get_completions_for_prefix("1 kilo")` | Returns the suffix `gram` | Use the curated catalog for ranked entries and original UTF-16 editor ranges. |
| `get_main_result` | Integer and symbolic results are exposed as presentation text | Use `display_only` unless a separate typed path is available. |
| Interrupt-aware evaluation | `2^2^100000` returned an exponent error in about 3 ms in the API probe | Apply query bounds, a 100 ms cooperative deadline and a client-side hard timeout. The probe does not establish a universal upper bound. |

The pinned fend release is suitable as the broad expression engine. Fractional, approximate and symbolic output from fend is treated as presentation text unless a typed adapter independently establishes the value. Time uses the bundled IANA 2026e database. PST is fixed UTC−08:00. The source-local calendar date is used when omitted. DST folds return both instants and gaps require a new, valid civil time.

## State and external data

- The separate calculator database lives under `$XDG_STATE_HOME/remya/launcher/calculator.sqlite3`, with the standard home fallback. Preview never inserts history or updates variables, alias choices or rate snapshots. History writes couple the selected result and idempotency receipt in one immediate transaction.
- The default programmer mode is unsigned 32-bit with checked overflow. For ambiguous lower-case data units the backend returns distinct interpretations. Explicitly choosing and committing a candidate remembers that spelling. For ¥ the default is CNY and JPY remains selectable.
- Currency preview reads the cached ECB daily reference snapshot. Explicit refresh uses the ECB HTTPS feed without an API key or query text. The feed publishes on working days. Cached values are marked stale after 24 hours and blocked after seven days. Results include provider, reference time, fetch time and the derivation of the cross-rate. The ECB publishes the data for information purposes and requires attribution for reuse. [ECB reference rates](https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html), [ECB copyright and disclaimer](https://www.ecb.europa.eu/services/using-our-site/disclaimer/html/index.en.html)
- The locked Linux dependency tree includes MIT fend and quick-xml, MIT or Unlicense jiff and jiff-tzdb, MIT or Apache-2.0 half, and the rustls validation path. The WebPKI trust-anchor data carries CDLA-Permissive-2.0. The bundle notice generator collects workspace dependency licenses and notices. The Linux dependency metadata and bundled notice contents were inspected. Cargo-deny 0.20.2 returned `licenses ok` for the locked x86_64 Linux calculator graph with build dependencies included and a crate-specific WebPKI data license allowance.

## Validation

Measured on Arch Linux 7.3.4, Intel Core Ultra 7 255H, installed release sidecar. Cold startup used 60 separate processes. Warm samples used 80 requests per query with an initialized SQLite store and measured complete NDJSON request and response latency. Values are milliseconds.

| Operation | p50 | p95 | p99 |
| --- | ---: | ---: | ---: |
| Cold process and hello | 0.717 | 1.012 | 1.081 |
| Warm `1+1` | 0.049 | 0.113 | 0.189 |
| Warm `3pi sqrt 2` | 0.119 | 0.143 | 0.279 |
| Warm `100 Mbps to MB/s` | 0.054 | 0.058 | 0.073 |
| Warm `1 kilo` | 0.138 | 0.170 | 0.199 |
| Warm `speed of light` | 0.143 | 0.409 | 0.424 |

A 2,000-revision burst returned the final exact result, kept history empty and reached a process RSS high-water mark of 9,852 KiB. These measurements cover the sidecar and IPC. Flutter input-to-render timing is pending.

| Spec state | Evidence |
| --- | --- |
| A | Process tests cover durable history, pinning, deletion, variable dependencies and cycle rejection. Calc renders history and saved definitions. |
| B–D | Numerical golden tests cover fend normalization, exact typed unit conversion, and integer widths and formats. The installed UI showed 12.5 MB/s, then 11.920928955 MiB/s after Tab. |
| E–F | Deterministic FP32, FP16, BF16, fixed-offset, IANA, and DST vectors. FP8 and general calendar arithmetic are deferred. |
| G | Deterministic policy fixtures and a real ECB refresh, math during refresh, and cache re-use across a process restart. |
| H–L | Catalog rank and UTF-16 edit tests, typo and ambiguity vectors, a dimension-mismatch fixture, and the Flutter error delay test. |
| M–N | The installed All scope displayed `1920*1080` inline. Widget tests exercise Calc entry and format selection. The full Actions menu and keyboard matrix remain pending manual acceptance. |

Remaining P7 checks include continuous fuzzing, screen-reader verification, and visual acceptance of every Figma state. The installed probe verified sidecar lazy startup and exit and that the clipboard service stayed active.
