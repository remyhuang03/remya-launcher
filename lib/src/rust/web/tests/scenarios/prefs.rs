use serde_json::{Value, json};

use crate::support::{SidecarHandle, TestEnv};

#[test]
fn default_engine_preference_provider_order_and_persistence() {
    let env = TestEnv::new();
    let mut sidecar = env.spawn(&["--standalone"]);

    let search_engine_ids = |sc: &mut SidecarHandle, id: u64, req: Value| -> Vec<String> {
        let mut payload = req;
        payload["id"] = json!(id);
        payload["op"] = json!("web.search");
        let resp = sc.request(payload);
        let groups = resp["data"]["groups"].as_array().unwrap();
        let search_group = groups.iter().find(|g| g["kind"] == "search").unwrap();
        search_group["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["engineId"].as_str().unwrap().to_owned())
            .collect()
    };

    // Default starts as duckduckgo.
    let initial = sidecar.request(json!({"id": 1, "op": "web.prefs.get"}));
    assert_eq!(initial["data"]["defaultEngineId"], "duckduckgo");
    assert_eq!(
        search_engine_ids(&mut sidecar, 2, json!({"query": "rust async"})),
        vec!["duckduckgo", "github"]
    );
    assert_eq!(
        search_engine_ids(&mut sidecar, 3, json!({"query": "site:docs.rs tokio"})),
        vec!["duckduckgo", "google"]
    );

    // Invalid or partially-invalid prefs.set must fail without writing.
    let bad_engine = sidecar.request(json!({
        "id": 4,
        "op": "web.prefs.set",
        "values": {"defaultEngineId": "github"}
    }));
    assert_eq!(bad_engine["error"]["code"], "INVALID_REQUEST");

    let mixed_bad = sidecar.request(json!({
        "id": 5,
        "op": "web.prefs.set",
        "values": {"defaultEngineId": "google", "networkIcons": true}
    }));
    assert_eq!(mixed_bad["error"]["code"], "OPERATION_UNSUPPORTED");
    assert_eq!(
        sidecar.request(json!({"id": 6, "op": "web.prefs.get"}))["data"]["defaultEngineId"],
        "duckduckgo"
    );

    // Set defaultEngineId to google and verify full prefs object & search order.
    let updated = sidecar.request(json!({
        "id": 7,
        "op": "web.prefs.set",
        "values": {"defaultEngineId": "google"}
    }));
    assert_eq!(updated["data"]["defaultEngineId"], "google");
    assert_eq!(updated["data"]["localIndex"], true);
    assert_eq!(updated["data"]["networkIcons"], false);
    assert_eq!(updated["data"]["nativeHistory"], false);
    assert_eq!(updated["data"]["syncIntervalSecs"], 60);

    assert_eq!(
        search_engine_ids(&mut sidecar, 8, json!({"query": "rust async"})),
        vec!["google", "github"]
    );
    assert_eq!(
        search_engine_ids(&mut sidecar, 9, json!({"query": "site:docs.rs tokio"})),
        vec!["google", "duckduckgo"]
    );

    // Custom keyword and explicit engineId override search without mutating prefs.
    let kw_ids = search_engine_ids(&mut sidecar, 10, json!({"query": "aw hyprland"}));
    assert_eq!(kw_ids.first().map(String::as_str), Some("archwiki"));
    let explicit_ids = search_engine_ids(
        &mut sidecar,
        11,
        json!({"query": "hyprland", "engineId": "youtube"}),
    );
    assert_eq!(explicit_ids.first().map(String::as_str), Some("youtube"));
    assert_eq!(
        sidecar.request(json!({"id": 12, "op": "web.prefs.get"}))["data"]["defaultEngineId"],
        "google"
    );

    // Clearing browsing data preserves the defaultEngineId preference.
    let cleared = sidecar.request(json!({"id": 13, "op": "web.prefs.clear"}));
    assert_eq!(cleared["data"]["cleared"], true);
    assert_eq!(
        sidecar.request(json!({"id": 14, "op": "web.prefs.get"}))["data"]["defaultEngineId"],
        "google"
    );

    // Concurrent / restarted sidecars observe the persisted preference.
    let mut sidecar2 = env.spawn(&["--standalone"]);
    assert_eq!(
        sidecar2.request(json!({"id": 15, "op": "web.prefs.get"}))["data"]["defaultEngineId"],
        "google"
    );
    assert_eq!(
        search_engine_ids(&mut sidecar2, 16, json!({"query": "rust async"})),
        vec!["google", "github"]
    );
    drop(sidecar2);
    drop(sidecar);

    let mut sidecar3 = env.spawn(&["--standalone"]);
    assert_eq!(
        sidecar3.request(json!({"id": 17, "op": "web.prefs.get"}))["data"]["defaultEngineId"],
        "google"
    );
}
