use std::fs;
use std::time::Duration;

use serde_json::json;

use crate::support::{TestEnv, recent_prtime_us, wait_for_profile_status};

#[test]
fn gio_shell_snap_and_wrapper_safety() {
    let env = TestEnv::new();
    let shell_marker = env.root.path().join("shell_ran.log");
    let snap_log = env.root.path().join("snap.log");
    let ff_log = env.root.path().join("ff_wrapped.log");

    env.write_script(
        "mock-firefox",
        &format!("printf '%s\\n' \"$@\" >> '{}'", ff_log.display()),
    );
    env.write_script(
        "snap",
        &format!("printf '%s\\n' \"$@\" >> '{}'", snap_log.display()),
    );

    // Shell-wrapped default handler must never be executed by GIO default open.
    env.write_desktop(
        "shell-default.desktop",
        &format!(
            "[Desktop Entry]\nType=Application\nName=ShellDefault\nExec=sh -c \"touch '{}'\" %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\n",
            shell_marker.display()
        ),
    );
    // Firefox desktop with pre-configured --profile flag has wrapper semantics that route wrong profile.
    env.write_desktop(
        "firefox.desktop",
        "[Desktop Entry]\nType=Application\nName=Firefox\nExec=mock-firefox --profile /tmp/other %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\n",
    );
    // Snap-wrapped Chromium: normal open via GIO works, private/profile routing rejected.
    env.write_desktop(
        "chromium_chromium.desktop",
        "[Desktop Entry]\nType=Application\nName=Chromium\nExec=snap run chromium %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\n",
    );
    env.write_mimeapps("shell-default.desktop");

    let prtime = recent_prtime_us();
    env.create_firefox_profile(
        "default-release",
        "Profiles/default.release",
        &[("https://wiki.hypr.land/", "Hyprland Wiki", 5, prtime)],
        &[],
        None,
    );

    let mut sidecar = env.spawn(&["--standalone"]);

    let caps = sidecar.request(json!({"id": 1, "op": "web.capabilities"}));
    assert_eq!(caps["data"]["defaultPrivate"], false);
    let browsers = caps["data"]["browsers"].as_array().unwrap();
    let ch_cap = browsers
        .iter()
        .find(|b| b["browserId"] == "chromium")
        .unwrap();
    assert_eq!(ch_cap["canOpen"], true);
    assert_eq!(ch_cap["canOpenPrivate"], false);
    let ff_cap = browsers
        .iter()
        .find(|b| b["browserId"] == "firefox")
        .unwrap();
    assert_eq!(ff_cap["canOpen"], true);
    assert_eq!(ff_cap["canOpenPrivate"], false);

    // Default open rejects shell handler without executing sh.
    let shell_open = sidecar.request(json!({
        "id": 2,
        "op": "web.perform",
        "action": "open.default",
        "target": {"url": "https://example.org/"}
    }));
    assert_eq!(shell_open["error"]["code"], "BROWSER_NOT_INSTALLED");
    assert!(!shell_marker.exists());

    // Snap Chromium rejects private open, allows normal open via trusted GIO handler.
    let snap_priv = sidecar.request(json!({
        "id": 3,
        "op": "web.perform",
        "action": "open.private",
        "browserId": "chromium",
        "target": {"url": "https://example.org/priv"}
    }));
    assert_eq!(snap_priv["error"]["code"], "OPERATION_UNSUPPORTED");
    assert!(!snap_log.exists());

    let snap_norm = sidecar.request(json!({
        "id": 4,
        "op": "web.perform",
        "action": "open.browser",
        "browserId": "chromium",
        "target": {"url": "https://example.org/snap-ok"}
    }));
    assert_eq!(snap_norm["data"]["status"], "dispatched");
    assert_eq!(snap_norm["data"]["nativeVisitConfirmed"], false);
    for _ in 0..20 {
        if snap_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        fs::read_to_string(&snap_log)
            .unwrap()
            .contains("https://example.org/snap-ok")
    );

    // Wrapper --profile semantics reject explicit profile routing & private window.
    let list = sidecar.request(json!({"id": 5, "op": "web.profiles.list"}));
    let ff_id = list["data"].as_array().unwrap()[0]["profileId"]
        .as_str()
        .unwrap()
        .to_owned();
    sidecar.request(json!({
        "id": 6,
        "op": "web.profiles.setEnabled",
        "profileId": ff_id,
        "enabled": true
    }));
    wait_for_profile_status(&mut sidecar, &ff_id, &["ready"]);

    let ff_prof = sidecar.request(json!({
        "id": 7,
        "op": "web.perform",
        "action": "open.browser",
        "browserId": "firefox",
        "profileId": ff_id,
        "target": {"url": "https://wiki.hypr.land/"}
    }));
    assert_eq!(ff_prof["error"]["code"], "OPERATION_UNSUPPORTED");
    assert!(!ff_log.exists());
}
