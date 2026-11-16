use serde_json::Value;
use url::Url;

use super::{make_entry, make_profile, open_test_index};
use crate::index::Search;

#[test]
fn broad_search_retains_bookmarks_and_supports_explicit_profile_scope() {
    let (_dir, mut idx) = open_test_index();
    let history = make_profile("history", "firefox", "Firefox", "History");
    let bookmarks = make_profile("bookmarks", "chromium", "Chromium", "Bookmarks");
    idx.set_enabled(&history.id, true).unwrap();
    idx.set_enabled(&bookmarks.id, true).unwrap();
    idx.replace(
        &history,
        (0..4100)
            .map(|i| {
                make_entry(
                    &i.to_string(),
                    false,
                    &format!("https://history.example.org/docs/{i}"),
                    "Reference for documentation",
                    1,
                    Some(1_700_000_000_000 + i),
                    &[],
                )
            })
            .collect(),
    )
    .unwrap();
    idx.replace(
        &bookmarks,
        vec![make_entry(
            "1",
            true,
            "https://saved.example.org/",
            "Saved documentation",
            0,
            None,
            &[],
        )],
    )
    .unwrap();

    let request = Search {
        query: "documentation",
        engine_id: None,
        profile_ids: None,
        limit: 10,
    };
    let all = idx.search(request, false).unwrap();
    assert_eq!(
        all["groups"][0]["items"][0]["url"],
        "https://saved.example.org/"
    );
    assert!(!all["groups"][1]["items"].as_array().unwrap().is_empty());
    let selected_profiles = [bookmarks.id];
    let scoped = idx
        .search(
            Search {
                profile_ids: Some(&selected_profiles),
                ..request
            },
            false,
        )
        .unwrap();
    assert_eq!(scoped["matchedCount"], 1);
    assert_eq!(
        scoped["groups"][0]["items"][0]["url"],
        "https://saved.example.org/"
    );
}

#[test]
fn cross_profile_grouped_dedup_and_prelimit_counts() {
    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-work", "firefox", "Firefox", "Work");
    let cr = make_profile("cr-main", "chromium", "Chromium", "Main");
    idx.set_enabled(&fx.id, true).unwrap();
    idx.set_enabled(&cr.id, true).unwrap();

    idx.replace(
        &fx,
        vec![
            make_entry(
                "b1",
                true,
                "https://wiki.hypr.land:443/",
                "Hyprland Wiki",
                0,
                None,
                &["Linux", "WM"],
            ),
            make_entry(
                "h1",
                false,
                "https://wiki.hypr.land/?utm_source=fx",
                "Hyprland Query Variant",
                5,
                Some(1_700_000_000_000),
                &[],
            ),
        ],
    )
    .unwrap();

    idx.replace(
        &cr,
        vec![
            make_entry(
                "b9",
                true,
                "https://wiki.hypr.land",
                "Hyprland Wiki",
                0,
                None,
                &["Docs"],
            ),
            make_entry(
                "h9",
                false,
                "https://wiki.hypr.land/",
                "Hyprland Wiki",
                12,
                Some(1_700_000_010_000),
                &[],
            ),
            make_entry(
                "h10",
                false,
                "https://wiki.hypr.land/Configuring/",
                "Hyprland Configuring",
                3,
                Some(1_699_999_000_000),
                &[],
            ),
        ],
    )
    .unwrap();

    let res = idx
        .search_at(
            Search {
                query: "hypr",
                engine_id: None,
                profile_ids: None,
                limit: 2,
            },
            true,
            1_700_000_020_000,
        )
        .unwrap();

    // 1 deduped bookmark + 3 distinct normalized history URLs = 4 before limit=2
    assert_eq!(res["matchedCount"], 4);
    assert_eq!(res["sync"]["stale"], true);

    let groups = res["groups"].as_array().unwrap();
    assert_eq!(groups[0]["kind"], "bookmarks");
    assert_eq!(groups[1]["kind"], "history");
    assert_eq!(groups[2]["kind"], "search");

    let bkm_items = groups[0]["items"].as_array().unwrap();
    assert_eq!(bkm_items.len(), 1);
    assert_eq!(bkm_items[0]["url"], "https://wiki.hypr.land/");
    let bkm_sources = bkm_items[0]["sources"].as_array().unwrap();
    assert_eq!(bkm_sources.len(), 2);

    // Tracking query variant was NOT collapsed into https://wiki.hypr.land/
    let hist_items = groups[1]["items"].as_array().unwrap();
    assert_eq!(hist_items.len(), 1); // limited to 2 total (1 bookmark + 1 history)

    let summary = res["sourceSummary"].as_array().unwrap();
    let cr_count = summary
        .iter()
        .find(|s| s["browserId"] == "chromium")
        .unwrap()["count"]
        .as_u64()
        .unwrap();
    let fx_count = summary
        .iter()
        .find(|s| s["browserId"] == "firefox")
        .unwrap()["count"]
        .as_u64()
        .unwrap();
    assert_eq!(cr_count, 3);
    assert_eq!(fx_count, 2);
}

#[test]
fn fts5_trigram_unicode_small_query_ranking_and_empty_frequent_six() {
    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-1", "firefox", "Firefox", "Default");
    idx.set_enabled(&fx.id, true).unwrap();

    let mut entries = vec![
        make_entry(
            "1",
            false,
            "https://hypr.land/",
            "Hyprland Home",
            2,
            Some(1000),
            &[],
        ),
        make_entry(
            "2",
            false,
            "https://docs.example.org/hypr",
            "Hypr Docs",
            5,
            Some(2000),
            &[],
        ),
        make_entry(
            "3",
            false,
            "https://other.example.org/page",
            "Something about hyprwm",
            50,
            Some(3000),
            &[],
        ),
        make_entry(
            "4",
            false,
            "https://jp.example.org/",
            "日本語ドキュメント",
            4,
            Some(4000),
            &[],
        ),
        make_entry(
            "5",
            false,
            "https://de.example.org/",
            "Überblick Seite",
            4,
            Some(4100),
            &[],
        ),
        make_entry(
            "6",
            false,
            "https://quote.example.org/",
            "Test \"quoted\" AND OR * syntax",
            4,
            Some(4200),
            &[],
        ),
    ];

    for i in 1..=8 {
        entries.push(make_entry(
            &format!("host-{i}"),
            false,
            &format!("https://site{i}.example.net/"),
            &format!("Site {i}"),
            i * 10,
            Some(10_000 + i * 100),
            &[],
        ));
    }

    idx.replace(&fx, entries).unwrap();

    // Event dedup + ranking boost from non-private opens
    let docs_url = Url::parse("https://docs.example.org/hypr").unwrap();
    idx.record_open_at(&docs_url, Some(&fx.id), "same-evt", 15_000)
        .unwrap();
    idx.record_open_at(&docs_url, Some(&fx.id), "same-evt", 15_500)
        .unwrap();
    let open_rows: i64 = idx
        .conn
        .query_row("SELECT COUNT(*) FROM launcher_opens", [], |r| r.get(0))
        .unwrap();
    assert_eq!(open_rows, 1);

    // Tier ordering: ExactUrl > HostPrefix > TitlePrefix > Substring
    let ranked = idx
        .search_at(
            Search {
                query: "hypr",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            16_000,
        )
        .unwrap();
    let hist = ranked["groups"][1]["items"].as_array().unwrap();
    assert_eq!(hist[0]["hostname"], "hypr.land"); // host prefix beats title prefix
    assert_eq!(hist[1]["title"], "Hypr Docs"); // title prefix beats substring

    // FTS5 special characters escaped safely
    let escaped = idx
        .search_at(
            Search {
                query: "\"quoted\" AND",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            16_000,
        )
        .unwrap();
    assert_eq!(escaped["matchedCount"], 1);

    // Unicode small query (<3 chars): CJK & mixed-case Unicode
    let cjk = idx
        .search_at(
            Search {
                query: "日本",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            16_000,
        )
        .unwrap();
    assert_eq!(cjk["matchedCount"], 1);

    let umlaut = idx
        .search_at(
            Search {
                query: "üb",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            16_000,
        )
        .unwrap();
    assert_eq!(umlaut["matchedCount"], 1);

    // Empty query returns max 6 frequentSites + recent
    let empty = idx
        .search_at(
            Search {
                query: "",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            16_000,
        )
        .unwrap();
    let freq = empty["frequentSites"].as_array().unwrap();
    assert_eq!(freq.len(), 6);
    assert!(freq[0]["representativeUrl"].is_string());
    assert!(freq[0]["origin"].is_string());
    let recent = empty["recent"].as_array().unwrap();
    assert!(!recent.is_empty());
    assert!(recent[0]["lastVisitedMs"].as_i64() >= recent[1]["lastVisitedMs"].as_i64());
}

#[test]
fn url_mode_same_host_history_fill_and_lookalike_exclusion() {
    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-1", "firefox", "Firefox", "Default");
    idx.set_enabled(&fx.id, true).unwrap();
    idx.replace(
        &fx,
        vec![
            make_entry(
                "1",
                false,
                "https://docs.rs/tokio",
                "Tokio Docs",
                4,
                Some(10_000),
                &[],
            ),
            make_entry(
                "2",
                false,
                "https://docs.rs/serde",
                "Serde Docs",
                20,
                Some(20_000),
                &[],
            ),
            make_entry(
                "3",
                false,
                "https://api.docs.rs/hyper",
                "Hyper API",
                2,
                Some(15_000),
                &[],
            ),
            make_entry(
                "4",
                false,
                "https://docs.rs.evil.org/tokio-phish",
                "Evil Phish",
                999,
                Some(99_000),
                &[],
            ),
            make_entry(
                "5",
                false,
                "https://docs.rs/axum",
                "Axum Docs",
                1,
                Some(5_000),
                &[],
            ),
        ],
    )
    .unwrap();

    // Real text match ("tokio") comes first, filled with recent same-host/domain history up to limit=3;
    // completion and matches exclude lookalike host docs.rs.evil.org.
    let partial = idx
        .search_at(
            Search {
                query: "docs.rs/tok",
                engine_id: None,
                profile_ids: None,
                limit: 3,
            },
            false,
            25_000,
        )
        .unwrap();
    assert_eq!(partial["input"]["kind"], "url");
    assert_eq!(
        partial["input"]["directNavigationTarget"],
        "https://docs.rs/tok"
    );
    assert_eq!(
        partial["input"]["completion"]["suggestedUrl"],
        "https://docs.rs/tokio"
    );
    let urls: Vec<&str> = partial["groups"][1]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["url"].as_str().unwrap())
        .collect();
    assert_eq!(
        urls,
        vec![
            "https://docs.rs/tokio",
            "https://docs.rs/serde",
            "https://api.docs.rs/hyper",
        ]
    );

    // Domain-only prefix must not complete or match lookalike docs.rs.evil.org
    let host_only = idx
        .search_at(
            Search {
                query: "docs.rs",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            25_000,
        )
        .unwrap();
    assert_eq!(
        host_only["input"]["completion"]["suggestedUrl"],
        "https://docs.rs/serde"
    );
    assert!(
        host_only["groups"][1]["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["hostname"] != "docs.rs.evil.org")
    );

    // Unmatched path still retains direct target and fills recent same-host history
    let no_text_match = idx
        .search_at(
            Search {
                query: "docs.rs/nonexistent",
                engine_id: None,
                profile_ids: None,
                limit: 2,
            },
            false,
            25_000,
        )
        .unwrap();
    assert_eq!(
        no_text_match["input"]["directNavigationTarget"],
        "https://docs.rs/nonexistent"
    );
    assert_eq!(no_text_match["input"]["completion"], Value::Null);
    let filled_urls: Vec<&str> = no_text_match["groups"][1]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["url"].as_str().unwrap())
        .collect();
    assert_eq!(
        filled_urls,
        vec!["https://docs.rs/serde", "https://api.docs.rs/hyper"]
    );
}
