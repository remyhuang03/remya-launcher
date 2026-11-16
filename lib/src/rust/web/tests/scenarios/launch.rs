use std::fs;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use rusqlite::Connection;
use serde_json::json;

use crate::support::{
    SAMPLE_PNG, TestEnv, recent_prtime_us, recent_webkit_us, wait_for_profile_status,
};

#[test]
fn browser_launch_routing_idempotency_forget_and_unsupported() {
    let env = TestEnv::new();
    let ff_log = env.root.path().join("firefox.log");
    let ch_log = env.root.path().join("chromium.log");
    let fp_log = env.root.path().join("flatpak.log");

    env.write_script(
        "mock-firefox",
        &format!("printf '%s\\n' \"$@\" >> '{}'", ff_log.display()),
    );
    env.write_script(
        "mock-chromium",
        &format!("printf '%s\\n' \"$@\" >> '{}'", ch_log.display()),
    );
    env.write_script(
        "flatpak",
        &format!("printf '%s\\n' \"$@\" >> '{}'", fp_log.display()),
    );

    env.write_desktop(
        "firefox.desktop",
        "[Desktop Entry]\nType=Application\nName=Firefox\nExec=mock-firefox %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\n",
    );
    env.write_desktop(
        "chromium.desktop",
        "[Desktop Entry]\nType=Application\nName=Chromium\nExec=mock-chromium %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\n",
    );
    // Flatpak-wrapped Brave: installed/runnable, but profile routing is unsupported.
    env.write_desktop(
        "com.brave.Browser.desktop",
        "[Desktop Entry]\nType=Application\nName=Brave\nExec=flatpak run com.brave.Browser %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\n",
    );
    env.write_mimeapps("firefox.desktop");

    let prtime = recent_prtime_us();
    let webkit = recent_webkit_us();
    env.create_firefox_profile(
        "default-release",
        "Profiles/default.release",
        &[("https://wiki.hypr.land/", "Hyprland Wiki", 5, prtime)],
        &[],
        Some("https://wiki.hypr.land/"),
    );
    let ch_prof_dir = env.create_chromium_profile(
        "Profile 1",
        "Work",
        &[("https://example.org/docs", "Example Docs", 4, webkit)],
    );

    let mut sidecar = env.spawn(&["--standalone"]);

    // Capabilities reflect only installed allowlisted browsers and accurate private support.
    let caps = sidecar.request(json!({"id": 1, "op": "web.capabilities"}));
    assert_eq!(caps["data"]["defaultPrivate"], true);
    let cap_browsers = caps["data"]["browsers"].as_array().unwrap();
    let cap_bids: Vec<&str> = cap_browsers
        .iter()
        .map(|b| b["browserId"].as_str().unwrap())
        .collect();
    assert!(cap_bids.contains(&"firefox"));
    assert!(cap_bids.contains(&"chromium"));
    assert!(cap_bids.contains(&"brave"));
    assert!(!cap_bids.contains(&"google-chrome"));
    assert_eq!(
        cap_browsers
            .iter()
            .find(|b| b["browserId"] == "firefox")
            .unwrap()["canOpenPrivate"],
        true
    );
    assert_eq!(
        cap_browsers
            .iter()
            .find(|b| b["browserId"] == "brave")
            .unwrap()["canOpenPrivate"],
        false
    );

    // Enable both profiles and wait for ready.
    let list = sidecar.request(json!({"id": 2, "op": "web.profiles.list"}));
    let profs = list["data"].as_array().unwrap();
    let ff_id = profs.iter().find(|p| p["browserId"] == "firefox").unwrap()["profileId"]
        .as_str()
        .unwrap()
        .to_owned();
    let ch_id = profs.iter().find(|p| p["browserId"] == "chromium").unwrap()["profileId"]
        .as_str()
        .unwrap()
        .to_owned();

    sidecar.request(
        json!({"id": 3, "op": "web.profiles.setEnabled", "profileId": ff_id, "enabled": true}),
    );
    sidecar.request(
        json!({"id": 4, "op": "web.profiles.setEnabled", "profileId": ch_id, "enabled": true}),
    );
    wait_for_profile_status(&mut sidecar, &ff_id, &["ready"]);
    wait_for_profile_status(&mut sidecar, &ch_id, &["ready"]);

    let search = sidecar.request(json!({"id": 5, "op": "web.search", "query": "hypr"}));
    let entry_id = search["data"]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["kind"] == "history")
        .unwrap()["items"][0]["entryId"]
        .as_str()
        .unwrap()
        .to_owned();

    // web.actions.available lists expected actions including pin.local toggle.
    let avail =
        sidecar.request(json!({"id": 6, "op": "web.actions.available", "entryId": entry_id}));
    let avail_arr = avail["data"].as_array().unwrap();
    let bookmark = avail_arr
        .iter()
        .find(|a| a["action"] == "bookmark.browser")
        .unwrap();
    assert_eq!(bookmark["enabled"], false);
    assert_eq!(bookmark["reason"], "EXTENSION_NOT_CONNECTED");
    let pin_before = avail_arr
        .iter()
        .find(|a| a["action"] == "pin.local")
        .unwrap();
    assert_eq!(pin_before["enabled"], true);
    assert_eq!(pin_before["label"], "Pin to Launcher");

    // pin.local requires authoritative entryId only.
    let pin_url_only = sidecar.request(json!({
        "id": 601,
        "op": "web.perform",
        "action": "pin.local",
        "target": {"url": "https://wiki.hypr.land/"}
    }));
    assert_eq!(pin_url_only["error"]["code"], "INVALID_REQUEST");

    let pin_missing = sidecar.request(json!({
        "id": 602,
        "op": "web.perform",
        "action": "pin.local",
        "target": {"entryId": "nonexistent-entry-id"}
    }));
    assert_eq!(pin_missing["error"]["code"], "ENTRY_NOT_FOUND");

    let pin_on = sidecar.request(json!({
        "id": 603,
        "op": "web.perform",
        "action": "pin.local",
        "target": {"entryId": entry_id}
    }));
    assert_eq!(
        pin_on["data"],
        json!({"status": "completed", "pinned": true})
    );

    let avail_pinned =
        sidecar.request(json!({"id": 604, "op": "web.actions.available", "entryId": entry_id}));
    let pin_after = avail_pinned["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["action"] == "pin.local")
        .unwrap();
    assert_eq!(pin_after["label"], "Unpin from Launcher");

    let empty_with_pin = sidecar.request(json!({"id": 605, "op": "web.search", "query": ""}));
    assert!(
        empty_with_pin["data"]["frequentSites"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["entryId"] == entry_id)
    );

    let pin_off = sidecar.request(json!({
        "id": 606,
        "op": "web.perform",
        "action": "pin.local",
        "target": {"entryId": entry_id}
    }));
    assert_eq!(
        pin_off["data"],
        json!({"status": "completed", "pinned": false})
    );

    // open.default with eventId + idempotent retry.
    let open1 = sidecar.request(json!({
        "id": 7,
        "op": "web.perform",
        "action": "open.default",
        "target": {"entryId": entry_id},
        "eventId": "evt-retry-1"
    }));
    assert_eq!(
        open1["data"],
        json!({
            "status": "dispatched",
            "nativeHistoryExpected": true,
            "nativeVisitConfirmed": false,
            "launcherOpenRecorded": true,
        })
    );
    for _ in 0..20 {
        if ff_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let ff_text_1 = fs::read_to_string(&ff_log).unwrap();
    assert!(ff_text_1.contains("https://wiki.hypr.land/"));
    let count_before = ff_text_1.lines().count();

    let open_retry = sidecar.request(json!({
        "id": 8,
        "op": "web.perform",
        "action": "open.default",
        "target": {"entryId": entry_id},
        "eventId": "evt-retry-1"
    }));
    assert_eq!(open_retry["data"], open1["data"]);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        fs::read_to_string(&ff_log).unwrap().lines().count(),
        count_before
    );

    // Conflicting reuse of same eventId must fail.
    let conflict = sidecar.request(json!({
        "id": 9,
        "op": "web.perform",
        "action": "open.default",
        "target": {"url": "https://example.org/other"},
        "eventId": "evt-retry-1"
    }));
    assert_eq!(conflict["error"]["code"], "INVALID_REQUEST");

    // open.private uses allowlisted private flag and never records launcherOpenRecorded.
    let priv_open = sidecar.request(json!({
        "id": 10,
        "op": "web.perform",
        "action": "open.private",
        "target": {"url": "https://example.org/private"}
    }));
    assert_eq!(
        priv_open["data"],
        json!({
            "status": "dispatched",
            "nativeHistoryExpected": false,
            "nativeVisitConfirmed": false,
            "launcherOpenRecorded": false,
        })
    );
    for _ in 0..20 {
        if fs::read_to_string(&ff_log)
            .unwrap()
            .contains("https://example.org/private")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let ff_text_2 = fs::read_to_string(&ff_log).unwrap();
    assert!(ff_text_2.contains("--private-window"));

    // open.browser with explicit Chromium profile routing.
    let ch_open = sidecar.request(json!({
        "id": 11,
        "op": "web.perform",
        "action": "open.browser",
        "browserId": "chromium",
        "profileId": ch_id,
        "target": {"url": "https://example.org/docs"}
    }));
    assert_eq!(ch_open["data"]["status"], "dispatched");
    for _ in 0..20 {
        if ch_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let ch_text = fs::read_to_string(&ch_log).unwrap();
    assert!(ch_text.contains(&format!(
        "--user-data-dir={}",
        ch_prof_dir.parent().unwrap().display()
    )));
    assert!(ch_text.contains("--profile-directory=Profile 1"));

    // No implicit fallback for missing browserId.
    let missing_browser = sidecar.request(json!({
        "id": 12,
        "op": "web.perform",
        "action": "open.browser",
        "browserId": "vivaldi",
        "target": {"url": "https://example.org/"}
    }));
    assert_eq!(missing_browser["error"]["code"], "BROWSER_NOT_INSTALLED");

    // Profile routing mismatch on open.default must error, not silently ignore profileId.
    let mismatched_default = sidecar.request(json!({
        "id": 13,
        "op": "web.perform",
        "action": "open.default",
        "profileId": ch_id,
        "target": {"url": "https://example.org/"}
    }));
    assert_eq!(mismatched_default["error"]["code"], "OPERATION_UNSUPPORTED");

    // Unsupported browser writes and UI-only actions.
    let forget_browser = sidecar.request(json!({
        "id": 14,
        "op": "web.perform",
        "action": "forget.browserPage",
        "profileId": ff_id,
        "target": {"url": "https://wiki.hypr.land/"}
    }));
    assert_eq!(forget_browser["error"]["code"], "EXTENSION_NOT_CONNECTED");

    let copy_url = sidecar.request(json!({
        "id": 15,
        "op": "web.perform",
        "action": "copy.url",
        "target": {"entryId": entry_id}
    }));
    assert_eq!(copy_url["error"]["code"], "OPERATION_UNSUPPORTED");

    // Favicon resolve + read + invalidation on forget.localPage.
    let fav_res = sidecar.request(json!({
        "id": 16,
        "op": "web.favicon.resolve",
        "url": "https://wiki.hypr.land/",
        "profileId": ff_id
    }));
    assert_eq!(fav_res["data"]["status"], "ready");
    let icon_key = fav_res["data"]["iconKey"].as_str().unwrap().to_owned();

    let fav_read = sidecar.request(json!({
        "id": 17,
        "op": "web.favicon.read",
        "iconKey": icon_key
    }));
    let decoded = BASE64_STANDARD
        .decode(fav_read["data"]["pngBase64"].as_str().unwrap())
        .unwrap();
    assert!(decoded.starts_with(&SAMPLE_PNG[..8]));
    assert!(decoded.windows(4).any(|w| w == b"IHDR"));

    // Flatpak-wrapped browser rejects explicit profile routing with OPERATION_UNSUPPORTED.
    let brave_dir = env.config_home.join("BraveSoftware/Brave-Browser/Default");
    fs::create_dir_all(&brave_dir).unwrap();
    let brave_hist = Connection::open(brave_dir.join("History")).unwrap();
    brave_hist
        .execute_batch(&format!(
            "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER, typed_count INTEGER, last_visit_time INTEGER, hidden INTEGER);
             INSERT INTO urls VALUES (1, 'https://brave.com/', 'Brave', 2, 1, {webkit}, 0);"
        ))
        .unwrap();
    drop(brave_hist);
    let list_with_brave = sidecar.request(json!({"id": 170, "op": "web.profiles.list"}));
    let brave_id = list_with_brave["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["browserId"] == "brave")
        .unwrap()["profileId"]
        .as_str()
        .unwrap()
        .to_owned();
    sidecar.request(
        json!({"id": 171, "op": "web.profiles.setEnabled", "profileId": brave_id, "enabled": true}),
    );
    wait_for_profile_status(&mut sidecar, &brave_id, &["ready"]);
    let fp_profile_open = sidecar.request(json!({
        "id": 172,
        "op": "web.perform",
        "action": "open.browser",
        "browserId": "brave",
        "profileId": brave_id,
        "target": {"url": "https://brave.com/"}
    }));
    assert_eq!(fp_profile_open["error"]["code"], "OPERATION_UNSUPPORTED");

    let fp_private_open = sidecar.request(json!({
        "id": 1721,
        "op": "web.perform",
        "action": "open.private",
        "browserId": "brave",
        "target": {"url": "https://brave.com/"}
    }));
    assert_eq!(fp_private_open["error"]["code"], "OPERATION_UNSUPPORTED");
    assert!(!fp_log.exists());

    // Normal Flatpak-wrapped browser open uses trusted GIO handler without profile/private flags.
    let fp_normal_open = sidecar.request(json!({
        "id": 1722,
        "op": "web.perform",
        "action": "open.browser",
        "browserId": "brave",
        "target": {"url": "https://brave.com/"}
    }));
    assert_eq!(
        fp_normal_open["data"],
        json!({
            "status": "dispatched",
            "nativeHistoryExpected": true,
            "nativeVisitConfirmed": false,
            "launcherOpenRecorded": true,
        })
    );
    for _ in 0..20 {
        if fp_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let fp_text = fs::read_to_string(&fp_log).unwrap();
    assert!(fp_text.contains("https://brave.com/"));

    // Favicon cache invalidates after sync reconciliation.
    sidecar.request(
        json!({"id": 173, "op": "web.sync.request", "profileIds": [ff_id], "mode": "full"}),
    );
    wait_for_profile_status(&mut sidecar, &ff_id, &["ready"]);
    let fav_after_sync =
        sidecar.request(json!({"id": 174, "op": "web.favicon.read", "iconKey": icon_key}));
    assert_eq!(fav_after_sync["error"]["code"], "ICON_NOT_FOUND");

    // Re-resolve then test invalidation on forget.localPage.
    let fav_res_2 = sidecar.request(json!({
        "id": 175,
        "op": "web.favicon.resolve",
        "url": "https://wiki.hypr.land/",
        "profileId": ff_id
    }));
    let icon_key_2 = fav_res_2["data"]["iconKey"].as_str().unwrap().to_owned();

    // search action builds and dispatches engine query URL via shared index::engine_url.
    let bad_search_ctrl = sidecar.request(json!({
        "id": 1751,
        "op": "web.perform",
        "action": "search",
        "target": {"engineId": "archwiki", "query": "bad\nquery"}
    }));
    assert_eq!(bad_search_ctrl["error"]["code"], "INVALID_QUERY");

    let search_perform = sidecar.request(json!({
        "id": 176,
        "op": "web.perform",
        "action": "search",
        "target": {"engineId": "archwiki", "query": "hyprland wayland"}
    }));
    assert_eq!(search_perform["data"]["status"], "dispatched");
    for _ in 0..20 {
        if fs::read_to_string(&ff_log)
            .unwrap()
            .contains("https://wiki.archlinux.org/index.php?search=hyprland+wayland")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        fs::read_to_string(&ff_log)
            .unwrap()
            .contains("https://wiki.archlinux.org/index.php?search=hyprland+wayland")
    );

    let forget_local = sidecar.request(json!({
        "id": 18,
        "op": "web.perform",
        "action": "forget.localPage",
        "target": {"entryId": entry_id}
    }));
    assert_eq!(forget_local["data"]["status"], "completed");
    assert_eq!(forget_local["data"]["scope"], "launcherOnly");
    assert_eq!(forget_local["data"]["affectedProfileCount"], 1);

    // Hidden entry no longer resolves or reads cached favicon.
    let fav_after_hide = sidecar.request(json!({
        "id": 19,
        "op": "web.favicon.read",
        "iconKey": icon_key_2
    }));
    assert_eq!(fav_after_hide["error"]["code"], "ICON_NOT_FOUND");

    let fav_resolve_after_hide = sidecar.request(json!({
        "id": 20,
        "op": "web.favicon.resolve",
        "url": "https://wiki.hypr.land/",
        "profileId": ff_id
    }));
    assert_eq!(fav_resolve_after_hide["data"]["status"], "monogram");

    // Disabling Chromium profile purges its entries from search immediately.
    let ch_before = sidecar.request(json!({"id": 21, "op": "web.search", "query": "example"}));
    assert!(ch_before["data"]["matchedCount"].as_u64().unwrap() >= 1);
    sidecar.request(
        json!({"id": 22, "op": "web.profiles.setEnabled", "profileId": ch_id, "enabled": false}),
    );
    let ch_after = sidecar.request(json!({"id": 23, "op": "web.search", "query": "example"}));
    assert_eq!(ch_after["data"]["matchedCount"], 0);
}
