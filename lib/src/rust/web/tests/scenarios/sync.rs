use std::fs;

use serde_json::json;

use crate::support::{TestEnv, recent_prtime_us, recent_webkit_us, wait_for_profile_status};

#[test]
fn synthetic_profiles_opt_in_sync_search_snapshot_preservation_and_clear() {
    let env = TestEnv::new();
    let prtime = recent_prtime_us();
    let ff_dir = env.create_firefox_profile(
        "default-release",
        "Profiles/default.release",
        &[
            (
                "https://github.com/hyprwm/Hyprland",
                "Hyprland Repo",
                12,
                prtime,
            ),
            (
                "https://wiki.hypr.land/",
                "Hyprland Wiki",
                8,
                prtime - 1_000_000,
            ),
        ],
        &[("WM", "https://wiki.hypr.land/", "Hyprland Wiki")],
        Some("https://wiki.hypr.land/"),
    );

    let mut sidecar = env.spawn(&["--standalone"]);

    let list = sidecar.request(json!({"id": 1, "op": "web.profiles.list"}));
    let profiles = list["data"].as_array().unwrap();
    assert_eq!(profiles.len(), 1);
    let profile_id = profiles[0]["profileId"].as_str().unwrap().to_owned();
    assert_eq!(profiles[0]["browserId"], "firefox");
    assert_eq!(profiles[0]["enabled"], false);
    assert_eq!(profiles[0]["status"], "disabled");
    // No absolute paths leaked in profile DTO.
    let serialized = profiles[0].to_string();
    assert!(!serialized.contains(env.root.path().to_str().unwrap()));

    // Before opt-in: search must return zero indexed matches and favicon must not probe DB.
    let pre_search = sidecar.request(json!({"id": 2, "op": "web.search", "query": "hypr"}));
    assert_eq!(pre_search["data"]["matchedCount"], 0);
    let pre_fav = sidecar.request(json!({
        "id": 3,
        "op": "web.favicon.resolve",
        "url": "https://wiki.hypr.land/",
        "profileId": profile_id
    }));
    assert_eq!(pre_fav["error"]["code"], "PROFILE_DISABLED");

    // Opt-in enables and triggers immediate background sync.
    let enabled = sidecar.request(json!({
        "id": 4,
        "op": "web.profiles.setEnabled",
        "profileId": profile_id,
        "enabled": true
    }));
    assert_eq!(enabled["data"]["enabled"], true);

    let synced_profile = wait_for_profile_status(&mut sidecar, &profile_id, &["ready"]);
    assert!(synced_profile["lastGoodSyncMs"].as_i64().is_some());

    // Search finds bookmarks & history in order bookmarks -> history -> search, plus engine fallback.
    let search = sidecar.request(json!({"id": 5, "op": "web.search", "query": "hypr"}));
    assert!(search["data"]["matchedCount"].as_u64().unwrap() >= 2);
    let groups = search["data"]["groups"].as_array().unwrap();
    let kinds: Vec<&str> = groups.iter().map(|g| g["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["bookmarks", "history", "search"]);

    // Engine keyword split + URL-like completion.
    let gh = sidecar.request(json!({"id": 6, "op": "web.search", "query": "gh hypr"}));
    assert_eq!(gh["data"]["input"]["kind"], "engine");
    assert_eq!(gh["data"]["input"]["engineId"], "github");
    assert_eq!(gh["data"]["input"]["query"], "hypr");

    let url_like =
        sidecar.request(json!({"id": 7, "op": "web.search", "query": "github.com/hyprw"}));
    assert_eq!(url_like["data"]["input"]["kind"], "url");
    assert_eq!(
        url_like["data"]["input"]["directNavigationTarget"],
        "https://github.com/hyprw"
    );

    // Corrupt/remove source DB and trigger sync -> snapshot preserved & status becomes stale.
    let places = ff_dir.join("places.sqlite");
    fs::write(&places, b"not a sqlite file").unwrap();
    let sync_req = sidecar.request(json!({
        "id": 8,
        "op": "web.sync.request",
        "profileIds": [profile_id],
        "mode": "full"
    }));
    assert_eq!(sync_req["data"]["accepted"], true);

    let stale_profile = wait_for_profile_status(&mut sidecar, &profile_id, &["stale"]);
    assert!(stale_profile["errorCode"].is_string());
    assert!(
        !stale_profile
            .to_string()
            .contains(env.root.path().to_str().unwrap())
    );

    let after_fail_search = sidecar.request(json!({"id": 9, "op": "web.search", "query": "hypr"}));
    assert!(after_fail_search["data"]["matchedCount"].as_u64().unwrap() >= 2);

    // Remove profile directory -> discovery reconciles safely and preserves snapshot as stale.
    fs::remove_dir_all(&ff_dir).unwrap();
    let after_missing = sidecar.request(json!({"id": 10, "op": "web.profiles.list"}));
    let missing_p = after_missing["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["profileId"] == profile_id)
        .unwrap();
    assert_eq!(missing_p["status"], "stale");
    assert_eq!(missing_p["errorCode"], "PROFILE_MISSING");

    // web.prefs.clear purges all local web data and opt-outs.
    let cleared = sidecar.request(json!({"id": 11, "op": "web.prefs.clear"}));
    assert_eq!(cleared["data"]["cleared"], true);
    let after_clear_search =
        sidecar.request(json!({"id": 12, "op": "web.search", "query": "hypr"}));
    assert_eq!(after_clear_search["data"]["matchedCount"], 0);
}

#[test]
fn chromium_opt_in_sync_reports_enabled_profile_count_and_hyphenated_history() {
    let env = TestEnv::new();
    let webkit = recent_webkit_us();
    env.create_chromium_profile(
        "Default",
        "Default",
        &[("https://example.org/remya-bar", "remya-bar", 5, webkit)],
    );

    let mut sidecar = env.spawn(&["--standalone"]);

    let list = sidecar.request(json!({"id": 1, "op": "web.profiles.list"}));
    let profiles = list["data"].as_array().unwrap();
    assert_eq!(profiles.len(), 1);
    let profile_id = profiles[0]["profileId"].as_str().unwrap().to_owned();
    assert_eq!(profiles[0]["browserId"], "chromium");
    assert_eq!(profiles[0]["enabled"], false);
    assert_eq!(profiles[0]["status"], "disabled");

    // Before opt-in: zero matches and enabledProfileCount is 0.
    let pre_search = sidecar.request(json!({"id": 2, "op": "web.search", "query": "remya-bar"}));
    assert_eq!(pre_search["data"]["matchedCount"], 0);
    assert_eq!(pre_search["data"]["sync"]["enabledProfileCount"], 0);

    // Explicit opt-in enables and syncs the Chromium profile.
    let enabled = sidecar.request(json!({
        "id": 3,
        "op": "web.profiles.setEnabled",
        "profileId": profile_id,
        "enabled": true
    }));
    assert_eq!(enabled["data"]["enabled"], true);

    let synced_profile = wait_for_profile_status(&mut sidecar, &profile_id, &["ready"]);
    assert!(synced_profile["lastGoodSyncMs"].as_i64().is_some());

    // After opt-in and ready: enabledProfileCount is 1 and history matches remya-bar.
    let post_search = sidecar.request(json!({"id": 4, "op": "web.search", "query": "remya-bar"}));
    assert_eq!(post_search["data"]["sync"]["enabledProfileCount"], 1);
    assert!(post_search["data"]["matchedCount"].as_u64().unwrap() >= 1);

    let groups = post_search["data"]["groups"].as_array().unwrap();
    let history_group = groups
        .iter()
        .find(|g| g["kind"] == "history")
        .expect("expected history group");
    let items = history_group["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|item| item["title"] == "remya-bar"
                && item["url"] == "https://example.org/remya-bar")
    );
}
