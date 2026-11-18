use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::{Command, Stdio};

#[test]
fn executable_opens_in_a_viewer_and_its_stdout_stays_out_of_the_protocol() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path();
    let applications = root.join("data/applications");
    fs::create_dir_all(&applications).unwrap();
    fs::create_dir(root.join("config")).unwrap();
    let selected = root.join("executable with spaces");
    fs::write(&selected, "#!/bin/sh\ntouch \"$HOME/executed\"\n").unwrap();
    let viewer = root.join("viewer");
    fs::write(
        &viewer,
        "#!/bin/sh\necho viewer-output\nprintf '%s' \"$1\" > \"$HOME/opened\"\n",
    )
    .unwrap();
    for path in [&selected, &viewer] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fs::write(
        applications.join("viewer.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Viewer\nExec={} %f\nMimeType=text/plain;\n",
            viewer.display()
        ),
    )
    .unwrap();
    fs::write(
        root.join("config/mimeapps.list"),
        "[Default Applications]\ntext/plain=viewer.desktop;\n",
    )
    .unwrap();
    let meta = selected.metadata().unwrap();
    let version = format!(
        "{}:{}:{}:{}:{}:{}",
        meta.dev(),
        meta.ino(),
        meta.mode(),
        meta.len(),
        meta.ctime(),
        meta.ctime_nsec()
    );
    let request = json!({"id":1,"op":"action","action":"open","operationId":"isolated-open","entryId":URL_SAFE_NO_PAD.encode(selected.as_os_str().as_encoded_bytes()),"version":version,"preferences":{"files-history":{"enabled":false}}});
    let mut child = Command::new(env!("CARGO_BIN_EXE_remya-launcher-files"))
        .env("HOME", root)
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("GIO_USE_VFS", "local")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let output = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["data"]["dispatched"], true, "{response}");
    for _ in 0..100 {
        if fs::read(root.join("opened"))
            .is_ok_and(|bytes| bytes == selected.as_os_str().as_encoded_bytes())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(
        fs::read(root.join("opened")).unwrap(),
        selected.as_os_str().as_encoded_bytes()
    );
    assert!(!root.join("executed").exists());
}
