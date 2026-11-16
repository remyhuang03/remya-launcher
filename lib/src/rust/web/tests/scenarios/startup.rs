use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::support::TestEnv;

#[test]
fn no_profile_startup_capabilities_prefs_and_private_permissions() {
    let env = TestEnv::new();
    let mut sidecar = env.spawn(&["--standalone"]);

    let caps = sidecar.request(json!({"id": 1, "op": "web.capabilities"}));
    assert_eq!(caps["id"], 1);
    assert_eq!(
        caps["data"],
        json!({
            "localIndex": true,
            "networkIcons": false,
            "nativeHistory": false,
            "browsers": [],
            "defaultPrivate": false,
        })
    );

    let profiles = sidecar.request(json!({"id": 2, "op": "web.profiles.list"}));
    assert_eq!(profiles["data"], json!([]));

    let sync_status = sidecar.request(json!({"id": 3, "op": "web.sync.status"}));
    assert_eq!(
        sync_status["data"],
        json!({
            "syncing": false,
            "profiles": [],
        })
    );

    let engines = sidecar.request(json!({"id": 4, "op": "web.engines.list"}));
    let engine_ids: Vec<&str> = engines["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["engineId"].as_str().unwrap())
        .collect();
    for expected in [
        "github",
        "google",
        "duckduckgo",
        "wikipedia",
        "youtube",
        "archwiki",
        "aur",
        "archpackages",
    ] {
        assert!(
            engine_ids.contains(&expected),
            "missing engine {expected} in {engine_ids:?}"
        );
    }

    // Empty search and non-empty engine fallback search with zero profiles.
    let empty_search = sidecar.request(json!({"id": 5, "op": "web.search", "query": ""}));
    assert_eq!(empty_search["data"]["matchedCount"], 0);
    assert_eq!(empty_search["data"]["sync"]["stale"], false);

    let query_search = sidecar.request(json!({"id": 6, "op": "web.search", "query": "hypr"}));
    assert_eq!(query_search["data"]["matchedCount"], 0);
    let groups = query_search["data"]["groups"].as_array().unwrap();
    let search_group = groups.iter().find(|g| g["kind"] == "search").unwrap();
    assert!(!search_group["items"].as_array().unwrap().is_empty());

    // Fixed preferences reject unsupported edits.
    let prefs = sidecar.request(json!({"id": 7, "op": "web.prefs.get"}));
    assert_eq!(prefs["data"]["localIndex"], true);
    assert_eq!(prefs["data"]["networkIcons"], false);
    assert_eq!(prefs["data"]["nativeHistory"], false);

    let bad_prefs = sidecar.request(json!({
        "id": 8,
        "op": "web.prefs.set",
        "values": {"networkIcons": true}
    }));
    assert_eq!(bad_prefs["error"]["code"], "OPERATION_UNSUPPORTED");

    // Verify private directory and sqlite file permissions (0700 / 0600).
    let web_dir = env.state_home.join("remya-launcher/web");
    let dir_mode = fs::symlink_metadata(&web_dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700);
    let db_mode = fs::symlink_metadata(web_dir.join("index.sqlite"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(db_mode, 0o600);

    drop(sidecar);

    // Symlink in final state directory component must be rejected on startup.
    let symlink_env = TestEnv::new();
    let app_dir = symlink_env.state_home.join("remya-launcher");
    fs::create_dir_all(&app_dir).unwrap();
    let decoy = symlink_env.root.path().join("decoy_web");
    fs::create_dir_all(&decoy).unwrap();
    std::os::unix::fs::symlink(&decoy, app_dir.join("web")).unwrap();

    let mut bad_cmd = Command::new(env!("CARGO_BIN_EXE_remya-launcher-web"));
    bad_cmd
        .arg("--standalone")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    symlink_env.apply_env(&mut bad_cmd);
    let out = bad_cmd.output().unwrap();
    assert!(!out.status.success());
    assert!(!decoy.join("index.sqlite").exists());
}

#[test]
fn malformed_frames_limits_ids_and_recovery() {
    let env = TestEnv::new();
    let mut sidecar = env.spawn(&[]);

    // Malformed JSON.
    let bad_json = sidecar.request_raw("{not valid json");
    assert_eq!(bad_json["id"], Value::Null);
    assert_eq!(bad_json["error"]["code"], "INVALID_REQUEST");

    // Non-object JSON.
    let arr = sidecar.request_raw("[1, 2, 3]");
    assert_eq!(arr["id"], Value::Null);
    assert_eq!(arr["error"]["code"], "INVALID_REQUEST");

    // Malformed request IDs.
    for bad_req in [
        json!({"op": "web.capabilities"}),
        json!({"id": null, "op": "web.capabilities"}),
        json!({"id": true, "op": "web.capabilities"}),
        json!({"id": 1.5, "op": "web.capabilities"}),
        json!({"id": [], "op": "web.capabilities"}),
        json!({"id": "", "op": "web.capabilities"}),
    ] {
        let resp = sidecar.request(bad_req);
        assert_eq!(resp["id"], Value::Null);
        assert_eq!(resp["error"]["code"], "INVALID_REQUEST");
    }

    // Oversized frame (> 32 KiB) must error cleanly and allow subsequent valid frames.
    let huge_query = "a".repeat(33 * 1024);
    let oversized_line = format!(r#"{{"id":10,"op":"web.search","query":"{huge_query}"}}"#);
    let oversized_resp = sidecar.request_raw(&oversized_line);
    assert_eq!(oversized_resp["error"]["code"], "INVALID_REQUEST");

    // Query size limit and control char check (without leaking query into error).
    let long_query = "secret_token_".repeat(100);
    let too_long = sidecar.request(json!({"id": 11, "op": "web.search", "query": long_query}));
    assert_eq!(too_long["id"], 11);
    assert_eq!(too_long["error"]["code"], "INVALID_QUERY");
    assert!(
        !too_long["error"]["message"]
            .as_str()
            .unwrap()
            .contains("secret_token_")
    );

    let ctrl_query = sidecar.request(json!({"id": 12, "op": "web.search", "query": "foo\nbar"}));
    assert_eq!(ctrl_query["error"]["code"], "INVALID_QUERY");

    // Wrong field types.
    let wrong_type = sidecar.request(json!({"id": 13, "op": "web.search", "query": 12345}));
    assert_eq!(wrong_type["error"]["code"], "INVALID_REQUEST");

    // Malformed entity IDs.
    let bad_pid = sidecar.request(json!({
        "id": 14,
        "op": "web.profiles.setEnabled",
        "profileId": "../../etc/passwd",
        "enabled": true
    }));
    assert_eq!(bad_pid["error"]["code"], "INVALID_REQUEST");

    let bad_icon_key = sidecar.request(json!({
        "id": 15,
        "op": "web.favicon.read",
        "iconKey": "../secret.png"
    }));
    assert_eq!(bad_icon_key["error"]["code"], "INVALID_REQUEST");

    // Disallowed URLs.
    for bad_url in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "https://user:pass@example.com/path",
    ] {
        let resp = sidecar.request(json!({
            "id": 16,
            "op": "web.perform",
            "action": "open.default",
            "target": {"url": bad_url}
        }));
        assert_eq!(resp["error"]["code"], "INVALID_URL");
    }

    // Unknown operation.
    let unknown_op = sidecar.request(json!({"id": 17, "op": "web.unknownOp"}));
    assert_eq!(unknown_op["error"]["code"], "OPERATION_UNSUPPORTED");

    // Sidecar recovers cleanly and serves subsequent valid requests.
    let ok = sidecar.request(json!({"id": 18, "op": "web.capabilities"}));
    assert_eq!(ok["id"], 18);
    assert_eq!(ok["data"]["localIndex"], true);
}
