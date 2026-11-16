use rusqlite::{Connection, params};
use serde_json::json;

use crate::support::{TestEnv, recent_prtime_us, wait_for_profile_status};

#[test]
fn multi_process_shared_projection_consent_and_favicons() {
    let env = TestEnv::new();
    let prtime = recent_prtime_us();
    let ff_dir = env.create_firefox_profile(
        "default-release",
        "Profiles/default.release",
        &[
            ("https://wiki.hypr.land/", "Hyprland Wiki", 5, prtime),
            (
                "https://docs.example.org/",
                "Docs Example",
                3,
                prtime - 1_000_000,
            ),
        ],
        &[],
        Some("https://wiki.hypr.land/"),
    );
    // Add favicon mapping for https://docs.example.org/ as well.
    {
        let fconn = Connection::open(ff_dir.join("favicons.sqlite")).unwrap();
        fconn
            .execute(
                "INSERT INTO moz_pages_w_icons(id, page_url, page_url_hash) VALUES(2, ?1, 2)",
                params!["https://docs.example.org/"],
            )
            .unwrap();
        fconn
            .execute(
                "INSERT INTO moz_icons_to_pages(page_id, icon_id, expire_ms) VALUES(2, 1, 9999999999999)",
                [],
            )
            .unwrap();
    }

    let mut sidecar_a = env.spawn(&["--standalone"]);
    let mut sidecar_b = env.spawn(&["--standalone"]);

    let list_a = sidecar_a.request(json!({"id": 1, "op": "web.profiles.list"}));
    let ff_id = list_a["data"].as_array().unwrap()[0]["profileId"]
        .as_str()
        .unwrap()
        .to_owned();

    // Sidecar A enables the profile and finishes sync.
    sidecar_a.request(json!({
        "id": 2,
        "op": "web.profiles.setEnabled",
        "profileId": ff_id,
        "enabled": true
    }));
    wait_for_profile_status(&mut sidecar_a, &ff_id, &["ready"]);

    // Sidecar B sees the cross-process opt-in (no stale "disabled" status) and caches icons.
    let list_b = sidecar_b.request(json!({"id": 100, "op": "web.profiles.list"}));
    let prof_b = &list_b["data"].as_array().unwrap()[0];
    assert_eq!(prof_b["enabled"], true);
    assert_ne!(prof_b["status"], "disabled");

    let fav_wiki_b = sidecar_b.request(json!({
        "id": 101,
        "op": "web.favicon.resolve",
        "url": "https://wiki.hypr.land/",
        "profileId": ff_id
    }));
    assert_eq!(fav_wiki_b["data"]["status"], "ready");
    let wiki_key_b = fav_wiki_b["data"]["iconKey"].as_str().unwrap().to_owned();

    let fav_docs_b = sidecar_b.request(json!({
        "id": 102,
        "op": "web.favicon.resolve",
        "url": "https://docs.example.org/",
        "profileId": ff_id
    }));
    assert_eq!(fav_docs_b["data"]["status"], "ready");
    let docs_key_b = fav_docs_b["data"]["iconKey"].as_str().unwrap().to_owned();

    // Cached reads initially succeed in Sidecar B.
    let read_ok = sidecar_b.request(json!({
        "id": 103,
        "op": "web.favicon.read",
        "iconKey": wiki_key_b
    }));
    assert!(read_ok["data"]["pngBase64"].is_string());

    // 1) Sidecar A hides https://wiki.hypr.land/ locally -> Sidecar B must reject cached read & resolve.
    let search_a =
        sidecar_a.request(json!({"id": 3, "op": "web.search", "query": "Hyprland Wiki"}));
    let wiki_entry_id = search_a["data"]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["kind"] == "history")
        .unwrap()["items"][0]["entryId"]
        .as_str()
        .unwrap()
        .to_owned();
    let hide_a = sidecar_a.request(json!({
        "id": 4,
        "op": "web.perform",
        "action": "forget.localPage",
        "target": {"entryId": wiki_entry_id}
    }));
    assert_eq!(hide_a["data"]["status"], "completed");

    let hidden_read_b = sidecar_b.request(json!({
        "id": 104,
        "op": "web.favicon.read",
        "iconKey": wiki_key_b
    }));
    assert_eq!(hidden_read_b["error"]["code"], "ICON_NOT_FOUND");
    let hidden_resolve_b = sidecar_b.request(json!({
        "id": 105,
        "op": "web.favicon.resolve",
        "url": "https://wiki.hypr.land/",
        "profileId": ff_id
    }));
    assert_eq!(hidden_resolve_b["data"]["status"], "monogram");

    // 2) Source URL removed on sync in Sidecar A -> Sidecar B must reject cached read & resolve.
    {
        let pconn = Connection::open(ff_dir.join("places.sqlite")).unwrap();
        pconn
            .execute(
                "DELETE FROM moz_places WHERE url = 'https://docs.example.org/'",
                [],
            )
            .unwrap();
    }
    sidecar_a.request(json!({
        "id": 5,
        "op": "web.sync.request",
        "profileIds": [ff_id],
        "mode": "full"
    }));
    wait_for_profile_status(&mut sidecar_a, &ff_id, &["ready"]);

    let removed_read_b = sidecar_b.request(json!({
        "id": 106,
        "op": "web.favicon.read",
        "iconKey": docs_key_b
    }));
    assert_eq!(removed_read_b["error"]["code"], "ICON_NOT_FOUND");
    let removed_resolve_b = sidecar_b.request(json!({
        "id": 107,
        "op": "web.favicon.resolve",
        "url": "https://docs.example.org/",
        "profileId": ff_id
    }));
    assert_eq!(removed_resolve_b["data"]["status"], "monogram");

    // Re-insert docs URL, sync in A, re-cache in B, then test cross-process disable + consent token race.
    {
        let pconn = Connection::open(ff_dir.join("places.sqlite")).unwrap();
        pconn
            .execute(
                "INSERT INTO moz_places(id, url, title, visit_count, hidden, frecency, last_visit_date) VALUES(99, 'https://docs.example.org/', 'Docs Example', 4, 0, 100, ?1)",
                params![prtime],
            )
            .unwrap();
    }
    sidecar_a.request(json!({
        "id": 6,
        "op": "web.sync.request",
        "profileIds": [ff_id],
        "mode": "full"
    }));
    wait_for_profile_status(&mut sidecar_a, &ff_id, &["ready"]);

    let fav_docs_b2 = sidecar_b.request(json!({
        "id": 108,
        "op": "web.favicon.resolve",
        "url": "https://docs.example.org/",
        "profileId": ff_id
    }));
    assert_eq!(fav_docs_b2["data"]["status"], "ready");
    let docs_key_b2 = fav_docs_b2["data"]["iconKey"].as_str().unwrap().to_owned();

    // Capture the persisted consent token before Sidecar A disables & re-enables.
    let db_path = env.state_home.join("remya-launcher/web/index.sqlite");
    let mut external_idx = remya_launcher_web::index::Index::open(&db_path).unwrap();
    let old_token = external_idx.consent_token(&ff_id).unwrap().unwrap();

    // 3) Sidecar A disables profile -> Sidecar B immediately rejects cached icon read & resolve.
    sidecar_a.request(json!({
        "id": 7,
        "op": "web.profiles.setEnabled",
        "profileId": ff_id,
        "enabled": false
    }));
    let disabled_read_b = sidecar_b.request(json!({
        "id": 109,
        "op": "web.favicon.read",
        "iconKey": docs_key_b2
    }));
    assert_eq!(disabled_read_b["error"]["code"], "ICON_NOT_FOUND");
    let disabled_resolve_b = sidecar_b.request(json!({
        "id": 110,
        "op": "web.favicon.resolve",
        "url": "https://docs.example.org/",
        "profileId": ff_id
    }));
    assert_eq!(disabled_resolve_b["error"]["code"], "PROFILE_DISABLED");

    // 4) Sidecar A re-enables -> new consent token is issued; stale token cannot overwrite projection.
    sidecar_a.request(json!({
        "id": 8,
        "op": "web.profiles.setEnabled",
        "profileId": ff_id,
        "enabled": true
    }));
    wait_for_profile_status(&mut sidecar_a, &ff_id, &["ready"]);
    let new_token = external_idx.consent_token(&ff_id).unwrap().unwrap();
    assert_ne!(old_token, new_token);

    let stale_applied = external_idx
        .replace_if_current(
            &remya_launcher_web::browsers::Profile {
                id: ff_id.clone(),
                browser_id: "firefox".into(),
                browser_label: "Firefox".into(),
                label: "default-release".into(),
                path: ff_dir,
                family: remya_launcher_web::browsers::Family::Gecko,
            },
            vec![remya_launcher_web::browsers::Entry {
                source_id: "stale-1".into(),
                bookmark: false,
                url: "https://stale-old-data.example.org/".into(),
                title: "Stale Old Data".into(),
                visited_count: 10,
                last_visited_ms: None,
                folder_path: vec![],
            }],
            &old_token,
        )
        .unwrap();
    assert!(!stale_applied);

    let stale_search = sidecar_a.request(json!({
        "id": 9,
        "op": "web.search",
        "query": "stale-old-data"
    }));
    assert_eq!(stale_search["data"]["matchedCount"], 0);
}
