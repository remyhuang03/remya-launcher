use rusqlite::Connection;
use tempfile::tempdir;
use url::Url;

use super::{make_entry, make_profile, open_test_index};
use crate::index::dto::EntryKind;
use crate::index::store::{MAX_LAUNCHER_OPENS, OPEN_RETENTION_MS, compute_entry_id};
use crate::index::{Index, Search};

#[test]
fn deletion_reconciliation_and_disable_race() {
    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-1", "firefox", "Firefox", "Default");
    idx.set_enabled(&fx.id, true).unwrap();

    idx.replace(
        &fx,
        vec![
            make_entry(
                "1",
                false,
                "https://a.example/",
                "Alpha",
                2,
                Some(1000),
                &[],
            ),
            make_entry("2", false, "https://b.example/", "Beta", 2, Some(2000), &[]),
        ],
    )
    .unwrap();

    assert!(idx.has_url(&fx.id, "https://b.example/").unwrap());

    // Reconciliation removes deleted row "2"
    idx.replace(
        &fx,
        vec![make_entry(
            "1",
            false,
            "https://a.example/",
            "Alpha Updated",
            3,
            Some(3000),
            &[],
        )],
    )
    .unwrap();

    assert!(!idx.has_url(&fx.id, "https://b.example/").unwrap());
    assert!(idx.has_url(&fx.id, "https://a.example/").unwrap());
    let search_b = idx
        .search_at(
            Search {
                query: "Beta",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            5000,
        )
        .unwrap();
    assert_eq!(search_b["matchedCount"], 0);

    // Record non-private open, then disable profile -> purges rows + scoped usage.
    let a_url = Url::parse("https://a.example/").unwrap();
    idx.record_open_at(&a_url, Some(&fx.id), "ev-1", 4000)
        .unwrap();

    idx.set_enabled(&fx.id, false).unwrap();
    assert!(!idx.enabled_ids().unwrap().contains(&fx.id));
    assert!(!idx.has_url(&fx.id, "https://a.example/").unwrap());

    // Simulated disable race: stale background replace MUST NOT resurrect disabled profile.
    idx.replace(
        &fx,
        vec![make_entry(
            "1",
            false,
            "https://a.example/",
            "Resurrect Attempt",
            99,
            Some(9999),
            &[],
        )],
    )
    .unwrap();
    assert!(!idx.has_url(&fx.id, "https://a.example/").unwrap());

    // Recording open on disabled profile is ignored.
    idx.record_open_at(&a_url, Some(&fx.id), "ev-2", 6000)
        .unwrap();
    let open_count: i64 = idx
        .conn
        .query_row("SELECT COUNT(*) FROM launcher_opens", [], |r| r.get(0))
        .unwrap();
    assert_eq!(open_count, 0);
}

#[test]
fn hide_page_and_site_across_enabled_profiles_and_clear() {
    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-1", "firefox", "Firefox", "Default");
    let cr = make_profile("cr-1", "chromium", "Chromium", "Default");
    idx.set_enabled(&fx.id, true).unwrap();
    idx.set_enabled(&cr.id, true).unwrap();

    let secret1 = "https://secret.example/p1";
    let secret2 = "https://secret.example/p2";
    let lookalike = "https://secret.example.evil.org/";

    idx.replace(
        &fx,
        vec![
            make_entry("1", false, secret1, "Secret One", 5, Some(10_000), &[]),
            make_entry("2", false, secret2, "Secret Two", 5, Some(10_000), &[]),
            make_entry("3", false, lookalike, "Lookalike", 5, Some(10_000), &[]),
        ],
    )
    .unwrap();
    idx.replace(
        &cr,
        vec![make_entry(
            "10",
            false,
            secret1,
            "Secret One CR",
            3,
            Some(9_000),
            &[],
        )],
    )
    .unwrap();

    let open_u = Url::parse(secret1).unwrap();
    idx.record_open_at(&open_u, Some(&fx.id), "open-1", 10_500)
        .unwrap();

    assert_eq!(idx.hide("non-existent-id", false).unwrap(), 0);

    let id_p1 = compute_entry_id(&fx.id, EntryKind::History, "1");
    assert_eq!(
        idx.entry(&id_p1).unwrap(),
        Some((secret1.to_owned(), fx.id.clone()))
    );

    // Hide page across enabled profiles (fx + cr = 2)
    let affected = idx.hide(&id_p1, false).unwrap();
    assert_eq!(affected, 2);
    assert!(!idx.has_url(&fx.id, secret1).unwrap());
    assert!(!idx.has_url(&cr.id, secret1).unwrap());
    assert_eq!(idx.entry(&id_p1).unwrap(), None);
    assert!(idx.has_url(&fx.id, secret2).unwrap());

    // Re-sync must not unhide secret1
    idx.replace(
        &fx,
        vec![
            make_entry("1", false, secret1, "Secret One", 9, Some(12_000), &[]),
            make_entry("2", false, secret2, "Secret Two", 5, Some(10_000), &[]),
            make_entry("3", false, lookalike, "Lookalike", 5, Some(10_000), &[]),
        ],
    )
    .unwrap();
    assert!(!idx.has_url(&fx.id, secret1).unwrap());

    // Hide entire site for secret.example; lookalike host stays visible
    let id_p2 = compute_entry_id(&fx.id, EntryKind::History, "2");
    assert_eq!(idx.hide(&id_p2, true).unwrap(), 1);
    assert!(!idx.has_url(&fx.id, secret2).unwrap());
    assert!(idx.has_url(&fx.id, lookalike).unwrap());

    // Clear purges all web data + opt-ins
    idx.clear().unwrap();
    assert!(idx.enabled_ids().unwrap().is_empty());
    assert!(!idx.has_url(&fx.id, lookalike).unwrap());
}

#[test]
fn record_open_age_and_count_capping() {
    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-1", "firefox", "Firefox", "Default");
    idx.set_enabled(&fx.id, true).unwrap();

    let url = Url::parse("https://example.org/app").unwrap();
    let base_ms: i64 = 1_700_000_000_000;

    // Aged event older than OPEN_RETENTION_MS is pruned when newer event arrives.
    idx.record_open_at(&url, Some(&fx.id), "old-evt", base_ms)
        .unwrap();
    let after_retention_ms = base_ms + OPEN_RETENTION_MS + 1_000;
    idx.record_open_at(&url, Some(&fx.id), "new-evt", after_retention_ms)
        .unwrap();

    let remaining_old: i64 = idx
        .conn
        .query_row(
            "SELECT COUNT(*) FROM launcher_opens WHERE event_id = 'old-evt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(remaining_old, 0);

    // Cap total events at MAX_LAUNCHER_OPENS.
    for i in 0..(MAX_LAUNCHER_OPENS + 10) {
        idx.record_open_at(
            &url,
            Some(&fx.id),
            &format!("cap-{i}"),
            after_retention_ms + i + 1,
        )
        .unwrap();
    }
    let total: i64 = idx
        .conn
        .query_row("SELECT COUNT(*) FROM launcher_opens", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, MAX_LAUNCHER_OPENS);
}

#[test]
fn pin_to_launcher_lifecycle_dedup_profile_filter_limit_and_cascade() {
    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-1", "firefox", "Firefox", "Default");
    let cr = make_profile("cr-1", "chromium", "Chromium", "Work");
    idx.set_enabled(&fx.id, true).unwrap();
    idx.set_enabled(&cr.id, true).unwrap();

    let mut fx_entries = vec![
        make_entry(
            "unvisited-bkm",
            true,
            "https://unvisited.example/",
            "Unvisited Bookmark",
            0,
            None,
            &["Pinned"],
        ),
        make_entry(
            "shared",
            true,
            "https://shared.example/",
            "Shared FX",
            1,
            Some(1_000),
            &[],
        ),
        make_entry(
            "frequent-unpinned",
            false,
            "https://veryfrequent.example/",
            "Very Frequent",
            500,
            Some(99_000),
            &[],
        ),
    ];
    for i in 1..=6 {
        fx_entries.push(make_entry(
            &format!("extra-{i}"),
            false,
            &format!("https://extra{i}.example/"),
            &format!("Extra {i}"),
            1,
            Some(1_000 + i),
            &[],
        ));
    }
    idx.replace(&fx, fx_entries).unwrap();
    idx.replace(
        &cr,
        vec![make_entry(
            "cr-shared",
            false,
            "https://shared.example/",
            "Shared CR",
            20,
            Some(50_000),
            &[],
        )],
    )
    .unwrap();

    // Unknown entry cannot be pinned
    assert_eq!(idx.toggle_pin("web:missing").unwrap(), None);
    assert!(!idx.is_pinned("web:missing").unwrap());

    let unvisited_id = compute_entry_id(&fx.id, EntryKind::Bookmark, "unvisited-bkm");
    let fx_shared_id = compute_entry_id(&fx.id, EntryKind::Bookmark, "shared");
    let cr_shared_id = compute_entry_id(&cr.id, EntryKind::History, "cr-shared");

    // Pin unvisited bookmark + shared URL
    assert_eq!(idx.toggle_pin(&unvisited_id).unwrap(), Some(true));
    assert!(idx.is_pinned(&unvisited_id).unwrap());
    assert_eq!(idx.toggle_pin(&fx_shared_id).unwrap(), Some(true));
    // Dedup by URL across profiles
    assert!(idx.is_pinned(&cr_shared_id).unwrap());

    // Empty query prioritizes pinned entries (including unvisited bookmark) before unpinned frequent sites
    let empty = idx
        .search_at(
            Search {
                query: "",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            100_000,
        )
        .unwrap();
    let freq = empty["frequentSites"].as_array().unwrap();
    assert_eq!(freq.len(), 6);
    assert_eq!(freq[0]["url"], "https://unvisited.example/");
    assert_eq!(freq[1]["url"], "https://shared.example/");
    assert_eq!(freq[1]["sources"].as_array().unwrap().len(), 2);
    assert_eq!(freq[2]["url"], "https://veryfrequent.example/");

    // Profile filter respects pin ownership: cr-1 only does not inherit fx-1's pin
    let cr_only = idx
        .search_at(
            Search {
                query: "",
                engine_id: None,
                profile_ids: Some(std::slice::from_ref(&cr.id)),
                limit: 10,
            },
            false,
            100_000,
        )
        .unwrap();
    let cr_freq = cr_only["frequentSites"].as_array().unwrap();
    assert_eq!(cr_freq.len(), 1);
    assert_eq!(cr_freq[0]["url"], "https://shared.example/");
    assert_eq!(cr_freq[0]["sources"].as_array().unwrap().len(), 1);

    // Pin limit 6: fill remaining 4 slots, 7th pin is rejected (Some(false))
    for i in 1..=4 {
        let eid = compute_entry_id(&fx.id, EntryKind::History, &format!("extra-{i}"));
        assert_eq!(idx.toggle_pin(&eid).unwrap(), Some(true));
    }
    let fifth_extra = compute_entry_id(&fx.id, EntryKind::History, "extra-5");
    assert_eq!(idx.toggle_pin(&fifth_extra).unwrap(), Some(false));
    assert!(!idx.is_pinned(&fifth_extra).unwrap());

    // Toggling via cr_shared_id unpins shared.example (dedup by URL)
    assert_eq!(idx.toggle_pin(&cr_shared_id).unwrap(), Some(false));
    assert!(!idx.is_pinned(&fx_shared_id).unwrap());

    // Hiding unvisited-bkm purges its pin and makes toggle_pin return None
    assert_eq!(idx.hide(&unvisited_id, false).unwrap(), 1);
    assert!(!idx.is_pinned(&unvisited_id).unwrap());
    assert_eq!(idx.toggle_pin(&unvisited_id).unwrap(), None);

    // Disabling profile purges remaining pins via ON DELETE CASCADE
    idx.set_enabled(&fx.id, false).unwrap();
    let remaining_pins: i64 = idx
        .conn
        .query_row("SELECT COUNT(*) FROM pins", [], |r| r.get(0))
        .unwrap();
    assert_eq!(remaining_pins, 0);
}

#[test]
fn consent_token_and_cross_process_replace_race_guard() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("index.sqlite");

    // Simulate v1 schema upgrade
    {
        let legacy = Connection::open(&db_path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE opt_ins (
                   profile_id TEXT PRIMARY KEY,
                   browser_id TEXT NOT NULL DEFAULT '',
                   browser_label TEXT NOT NULL DEFAULT '',
                   profile_label TEXT NOT NULL DEFAULT '',
                   enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
                   updated_ms INTEGER NOT NULL
                 );
                 INSERT INTO opt_ins (profile_id, enabled, updated_ms)
                 VALUES ('legacy-1', 1, 1000);
                 PRAGMA user_version = 1;",
            )
            .unwrap();
    }

    let mut idx_a = Index::open(&db_path).unwrap();
    let mut idx_b = Index::open(&db_path).unwrap();
    let legacy_tok = idx_a.consent_token("legacy-1").unwrap();
    assert!(legacy_tok.is_some());

    let fx = make_profile("fx-race", "firefox", "Firefox", "Default");
    assert_eq!(idx_a.consent_token(&fx.id).unwrap(), None);

    // Enabling creates unique consent token; repeated enable preserves it
    idx_a.set_enabled(&fx.id, true).unwrap();
    let token1 = idx_a.consent_token(&fx.id).unwrap().expect("token1");
    idx_a.set_enabled(&fx.id, true).unwrap();
    assert_eq!(
        idx_a.consent_token(&fx.id).unwrap().as_deref(),
        Some(token1.as_str())
    );

    // Sidecar B disables and re-enables before Sidecar A commits stale entries
    idx_b.set_enabled(&fx.id, false).unwrap();
    assert_eq!(idx_b.consent_token(&fx.id).unwrap(), None);
    idx_b.set_enabled(&fx.id, true).unwrap();
    let token2 = idx_b.consent_token(&fx.id).unwrap().expect("token2");
    assert_ne!(token1, token2);

    // Stale Sidecar A attempt fails atomically without importing old data
    let stale_ok = idx_a
        .replace_if_current(
            &fx,
            vec![make_entry(
                "old",
                false,
                "https://stale.example/",
                "Stale",
                1,
                Some(1000),
                &[],
            )],
            &token1,
        )
        .unwrap();
    assert!(!stale_ok);
    assert!(!idx_b.has_url(&fx.id, "https://stale.example/").unwrap());

    // Current Sidecar B succeeds and preserves token2
    let fresh_ok = idx_b
        .replace_if_current(
            &fx,
            vec![make_entry(
                "new",
                false,
                "https://fresh.example/",
                "Fresh",
                1,
                Some(2000),
                &[],
            )],
            &token2,
        )
        .unwrap();
    assert!(fresh_ok);
    assert!(idx_b.has_url(&fx.id, "https://fresh.example/").unwrap());
    assert_eq!(
        idx_b.consent_token(&fx.id).unwrap().as_deref(),
        Some(token2.as_str())
    );
}
