use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use flate2::Compression;
use flate2::write::GzEncoder;
use serde_json::{Value, json};
use tempfile::TempDir;

struct TestEnv {
    root: TempDir,
    data_home: PathBuf,
    sys_data: PathBuf,
    config_home: PathBuf,
    state_home: PathBuf,
    cache_home: PathBuf,
    runtime_dir: PathBuf,
    bin_dir: PathBuf,
    pacman_sync: PathBuf,
    pacman_local: PathBuf,
}

impl TestEnv {
    fn new() -> Self {
        let root = TempDir::new().unwrap();
        let p = root.path();
        let data_home = p.join("data_home");
        let sys_data = p.join("sys_data");
        let config_home = p.join("config_home");
        let state_home = p.join("state_home");
        let cache_home = p.join("cache_home");
        let runtime_dir = p.join("runtime");
        let bin_dir = p.join("bin");
        let pacman_sync = p.join("pacman_sync");
        let pacman_local = p.join("pacman_local");
        for dir in [
            data_home.join("applications"),
            sys_data.join("applications"),
            config_home.clone(),
            state_home.clone(),
            cache_home.clone(),
            runtime_dir.clone(),
            bin_dir.clone(),
            pacman_sync.clone(),
            pacman_local.clone(),
        ] {
            fs::create_dir_all(dir).unwrap();
        }
        fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700)).unwrap();

        Self {
            root,
            data_home,
            sys_data,
            config_home,
            state_home,
            cache_home,
            runtime_dir,
            bin_dir,
            pacman_sync,
            pacman_local,
        }
    }

    fn write_script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.bin_dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn write_desktop(&self, user: bool, rel_path: &str, contents: &str) -> PathBuf {
        let base = if user {
            self.data_home.join("applications")
        } else {
            self.sys_data.join("applications")
        };
        let path = base.join(rel_path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn write_flatpak_desktop(&self, desktop_id: &str, contents: &str) -> PathBuf {
        let path = self
            .data_home
            .join("flatpak/exports/share/applications")
            .join(desktop_id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    fn apply_env(&self, cmd: &mut Command) {
        let path = format!(
            "{}:{}",
            self.bin_dir.display(),
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into())
        );
        cmd.env("HOME", self.root.path())
            .env("XDG_DATA_HOME", &self.data_home)
            .env("XDG_DATA_DIRS", &self.sys_data)
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("XDG_STATE_HOME", &self.state_home)
            .env("XDG_CACHE_HOME", &self.cache_home)
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .env("XDG_CURRENT_DESKTOP", "Hyprland")
            .env("LANG", "zh_CN.UTF-8")
            .env("LC_ALL", "zh_CN.UTF-8")
            .env("REMYA_APPS_PACMAN_SYNC_DIR", &self.pacman_sync)
            .env("REMYA_APPS_PACMAN_LOCAL_DIR", &self.pacman_local)
            .env("PATH", path);
    }

    fn spawn_cli(&self, args: &[&str]) -> SidecarHandle {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_remya-launcher-apps"));
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.apply_env(&mut cmd);
        let mut child = cmd.spawn().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        SidecarHandle { child, stdout }
    }
}

struct SidecarHandle {
    child: Child,
    stdout: BufReader<std::process::ChildStdout>,
}

impl SidecarHandle {
    fn request(&mut self, payload: Value) -> Value {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{payload}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).unwrap();
        assert!(n > 0, "sidecar closed stdout unexpectedly");
        serde_json::from_str(line.trim()).unwrap()
    }
}

impl Drop for SidecarHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn write_pacman_sync_db(path: &Path, entries: &[(&str, &str, &str)]) {
    let mut tar_bytes = Vec::new();
    for (name, version, desc) in entries {
        let content = format!("%NAME%\n{name}\n\n%VERSION%\n{version}\n\n%DESC%\n{desc}\n");
        let entry_name = format!("{name}-{version}/desc");
        let mut header = [0u8; 512];
        let name_bytes = entry_name.as_bytes();
        header[..name_bytes.len()].copy_from_slice(name_bytes);
        let size_str = format!("{:011o}\0", content.len());
        header[124..136].copy_from_slice(size_str.as_bytes());
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        // Checksum field filled with spaces before calculation.
        header[148..156].fill(b' ');
        let sum: u32 = header.iter().map(|&b| u32::from(b)).sum();
        let sum_str = format!("{sum:06o}\0 ");
        header[148..156].copy_from_slice(sum_str.as_bytes());
        tar_bytes.extend_from_slice(&header);
        tar_bytes.extend_from_slice(content.as_bytes());
        let rem = content.len() % 512;
        if rem != 0 {
            tar_bytes.resize(tar_bytes.len() + (512 - rem), 0);
        }
    }
    tar_bytes.resize(tar_bytes.len() + 1024, 0);

    let file = File::create(path).unwrap();
    let mut enc = GzEncoder::new(file, Compression::default());
    enc.write_all(&tar_bytes).unwrap();
    enc.finish().unwrap();
}

#[test]
fn discovery_shadowing_filtering_and_live_refresh() {
    let env = TestEnv::new();
    env.write_script("alpha-bin", "exit 0");
    env.write_script("flat-bin", "exit 0");

    // System entry shadowed by user entry with same ID.
    env.write_desktop(
        false,
        "alpha.desktop",
        "[Desktop Entry]\nType=Application\nName=System Alpha\nExec=alpha-bin\n",
    );
    env.write_desktop(
        true,
        "alpha.desktop",
        "[Desktop Entry]\nType=Application\nName=User Alpha\nGenericName=Demo Tool\nIcon=alpha-icon\nExec=alpha-bin\nActions=open-new;prefs;\n\n[Desktop Action open-new]\nName=Open New\nExec=alpha-bin --new\n\n[Desktop Action prefs]\nName=Preferences\nExec=alpha-bin --prefs\n",
    );

    // User Hidden=true masks system entry with same desktop ID.
    env.write_desktop(
        false,
        "shadowed.desktop",
        "[Desktop Entry]\nType=Application\nName=Shadowed App\nExec=alpha-bin\n",
    );
    env.write_desktop(
        true,
        "shadowed.desktop",
        "[Desktop Entry]\nType=Application\nName=Shadowed App\nHidden=true\nExec=alpha-bin\n",
    );

    // Filtered out entries: NoDisplay, OnlyShowIn!=Hyprland, missing TryExec.
    env.write_desktop(
        false,
        "nodisplay.desktop",
        "[Desktop Entry]\nType=Application\nName=NoDisplay App\nNoDisplay=true\nExec=alpha-bin\n",
    );
    env.write_desktop(
        false,
        "gnomeonly.desktop",
        "[Desktop Entry]\nType=Application\nName=Gnome Only\nOnlyShowIn=GNOME;\nExec=alpha-bin\n",
    );
    env.write_desktop(
        false,
        "missing-tryexec.desktop",
        "[Desktop Entry]\nType=Application\nName=Missing TryExec\nTryExec=definitely-not-existing-xyz\nExec=alpha-bin\n",
    );

    // Flatpak export entry.
    env.write_flatpak_desktop(
        "org.example.Flat.desktop",
        "[Desktop Entry]\nType=Application\nName=Flat App\nExec=flat-bin\n",
    );

    let mut sidecar = env.spawn_cli(&["--standalone"]);
    let reply = sidecar.request(json!({"id": 42, "op": "search", "query": ""}));
    assert_eq!(reply["id"], 42);
    let data = &reply["data"];
    assert_eq!(data["total"], 2);
    let apps = data["apps"].as_array().unwrap();
    assert_eq!(apps.len(), 2);
    assert_eq!(apps[0]["id"], "org.example.Flat.desktop");
    assert_eq!(apps[0]["source"], "Flatpak");
    assert_eq!(apps[1]["id"], "alpha.desktop");
    assert_eq!(apps[1]["name"], "User Alpha");
    assert_eq!(apps[1]["icon"], "alpha-icon");
    assert_eq!(
        apps[1]["actions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["open-new", "prefs"]
    );

    // Write a new desktop entry while running -> live refresh discovers it.
    std::thread::sleep(Duration::from_millis(50));
    env.write_desktop(
        true,
        "zeta.desktop",
        "[Desktop Entry]\nType=Application\nName=Zeta Live\nExec=alpha-bin\n",
    );
    let mut seen_zeta = false;
    for _ in 0..10 {
        let r = sidecar.request(json!({"id": 43, "op": "search", "query": "zeta"}));
        if r["data"]["matched"].as_u64() == Some(1) {
            seen_zeta = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    assert!(seen_zeta, "live refresh did not pick up zeta.desktop");
}

#[test]
fn ranking_tiers_pinyin_action_split_frecency_and_query_winners() {
    let env = TestEnv::new();
    env.write_script("code", "exit 0");
    env.write_script("kitty", "exit 0");
    env.write_script("chrome", "exit 0");
    env.write_script("wechat", "exit 0");
    env.write_script("vpad", "exit 0");

    env.write_desktop(
        false,
        "code.desktop",
        "[Desktop Entry]\nType=Application\nName=Visual Studio Code\nGenericName=Code Editor\nExec=code\n",
    );
    env.write_desktop(
        false,
        "vpad.desktop",
        "[Desktop Entry]\nType=Application\nName=VscPad\nExec=vpad\n",
    );
    env.write_desktop(
        false,
        "kitty.desktop",
        "[Desktop Entry]\nType=Application\nName=kitty\nGenericName=Terminal emulator\nKeywords=shell;console;\nComment=Fast GPU terminal\nExec=kitty\n",
    );
    env.write_desktop(
        false,
        "google-chrome.desktop",
        "[Desktop Entry]\nType=Application\nName=Google Chrome\nComment=Access the Internet\nExec=chrome\nActions=new-window;new-incognito;\n\n[Desktop Action new-window]\nName=New Window\nExec=chrome\n\n[Desktop Action new-incognito]\nName=New Incognito Window\nExec=chrome --incognito\n",
    );
    env.write_desktop(
        false,
        "wechat.desktop",
        "[Desktop Entry]\nType=Application\nName=WeChat\nName[zh_CN]=微信\nExec=wechat\n",
    );

    let mut sidecar = env.spawn_cli(&["--standalone"]);

    // Tier 1 prefix ("vsc" -> VscPad) beats Tier 2 acronym ("vsc" -> Visual Studio Code).
    let r = sidecar.request(json!({"id": 1, "op": "search", "query": "vsc"}));
    let ids: Vec<&str> = r["data"]["apps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["vpad.desktop", "code.desktop"]);

    // Tier 4 GenericName ("te" -> Terminal emulator => matchKind: "kind").
    let r = sidecar.request(json!({"id": 2, "op": "search", "query": "te"}));
    assert_eq!(r["data"]["apps"][0]["id"], "kitty.desktop");
    assert_eq!(r["data"]["apps"][0]["matchKind"], "kind");
    assert_eq!(r["data"]["apps"][0]["matchText"], "Terminal emulator");

    // Tier 5 Keywords ("console" -> matchKind: "keyword").
    let r = sidecar.request(json!({"id": 3, "op": "search", "query": "console"}));
    assert_eq!(r["data"]["apps"][0]["id"], "kitty.desktop");
    assert_eq!(r["data"]["apps"][0]["matchKind"], "keyword");

    // Tier 6 Comment: ignored for < 3 chars ("gp"), matched for >= 3 chars ("gpu").
    let r = sidecar.request(json!({"id": 4, "op": "search", "query": "gp"}));
    assert_eq!(r["data"]["matched"], 0);
    let r = sidecar.request(json!({"id": 5, "op": "search", "query": "gpu"}));
    assert_eq!(r["data"]["apps"][0]["id"], "kitty.desktop");
    assert_eq!(r["data"]["apps"][0]["matchKind"], "comment");

    // P1 Pinyin initials & full pinyin ("wx" & "weixin" -> 微信), disabled when pinyin=false.
    let r = sidecar.request(json!({"id": 6, "op": "search", "query": "wx"}));
    assert_eq!(r["data"]["apps"][0]["id"], "wechat.desktop");
    let r = sidecar.request(json!({"id": 7, "op": "search", "query": "weixin"}));
    assert_eq!(r["data"]["apps"][0]["id"], "wechat.desktop");
    sidecar.request(json!({"id": 8, "op": "preferences", "values": {"pinyin": false}}));
    let r = sidecar.request(json!({"id": 9, "op": "search", "query": "wx"}));
    assert_eq!(r["data"]["matched"], 0);

    // Direct-hit desktop action ("chrome inco").
    let r = sidecar.request(json!({"id": 10, "op": "search", "query": "chrome inco"}));
    let hit = &r["data"]["apps"][0];
    assert_eq!(hit["id"], "google-chrome.desktop");
    assert_eq!(hit["name"], "Google Chrome");
    assert_eq!(hit["matchKind"], "action");
    assert_eq!(hit["matchText"], "New Incognito Window");
    assert_eq!(hit["actionId"], "new-incognito");

    // Query winner overrides tier for the same normalized query; empty query populates frequent.
    let open = sidecar.request(json!({
        "id": 11,
        "op": "perform",
        "action": "open",
        "appId": "code.desktop",
        "query": "vsc"
    }));
    assert!(open["data"].is_object());
    let r = sidecar.request(json!({"id": 12, "op": "search", "query": "vsc"}));
    assert_eq!(r["data"]["apps"][0]["id"], "code.desktop");

    let empty = sidecar.request(json!({"id": 13, "op": "search", "query": ""}));
    assert_eq!(empty["data"]["frequent"][0]["id"], "code.desktop");
}

#[test]
fn pin_hide_unhide_and_preferences_persistence() {
    let env = TestEnv::new();
    env.write_script("app-bin", "exit 0");
    env.write_desktop(
        false,
        "one.desktop",
        "[Desktop Entry]\nType=Application\nName=One App\nExec=app-bin\n",
    );
    env.write_desktop(
        false,
        "two.desktop",
        "[Desktop Entry]\nType=Application\nName=Two App\nExec=app-bin\n",
    );

    {
        let mut sidecar = env.spawn_cli(&["--standalone"]);

        // Preferences defaults & validation.
        let prefs = sidecar.request(json!({"id": 1, "op": "preferences"}));
        assert_eq!(
            prefs["data"],
            json!({
                "alwaysNew": false,
                "frequentDays": 30,
                "pinyin": true,
                "terminal": "",
                "showPackages": true,
                "official": true,
                "flatpak": true,
                "aur": true
            })
        );
        let invalid =
            sidecar.request(json!({"id": 2, "op": "preferences", "values": {"frequentDays": 0}}));
        assert!(invalid["error"].is_string());

        let updated = sidecar.request(json!({
            "id": 3,
            "op": "preferences",
            "values": {"alwaysNew": true, "terminal": "kitty"}
        }));
        assert_eq!(updated["data"]["alwaysNew"], true);
        assert_eq!(updated["data"]["terminal"], "kitty");

        // Pin preserves order.
        sidecar.request(json!({"id": 4, "op": "perform", "action": "pin", "appId": "two.desktop"}));
        sidecar.request(json!({"id": 5, "op": "perform", "action": "pin", "appId": "one.desktop"}));
        let empty = sidecar.request(json!({"id": 6, "op": "search", "query": ""}));
        assert_eq!(empty["data"]["alwaysNew"], true);
        let pinned_ids: Vec<&str> = empty["data"]["pinned"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["id"].as_str().unwrap())
            .collect();
        assert_eq!(pinned_ids, vec!["two.desktop", "one.desktop"]);

        // Hide removes from visible search & exposes hidden-only fallback.
        sidecar
            .request(json!({"id": 7, "op": "perform", "action": "hide", "appId": "one.desktop"}));
        let empty2 = sidecar.request(json!({"id": 8, "op": "search", "query": ""}));
        assert_eq!(empty2["data"]["total"], 1);
        let hidden_search = sidecar.request(json!({"id": 9, "op": "search", "query": "one"}));
        assert_eq!(hidden_search["data"]["hidden"], true);
        assert_eq!(hidden_search["data"]["apps"][0]["id"], "one.desktop");
        assert_eq!(hidden_search["data"]["apps"][0]["hidden"], true);

        let hidden_list = sidecar.request(json!({"id": 10, "op": "hidden"}));
        assert_eq!(hidden_list["data"]["apps"].as_array().unwrap().len(), 1);
    }

    // Persistence survives process restart.
    let mut sidecar2 = env.spawn_cli(&["--standalone"]);
    let prefs2 = sidecar2.request(json!({"id": 11, "op": "preferences"}));
    assert_eq!(prefs2["data"]["alwaysNew"], true);
    assert_eq!(prefs2["data"]["terminal"], "kitty");
    sidecar2
        .request(json!({"id": 12, "op": "perform", "action": "unhide", "appId": "one.desktop"}));
    let empty3 = sidecar2.request(json!({"id": 13, "op": "search", "query": ""}));
    assert_eq!(empty3["data"]["total"], 2);
}

#[test]
fn hyprland_windows_focus_quit_and_workspace_validation() {
    let env = TestEnv::new();
    let hypr_log = env.root.path().join("hypr.log");
    let launch_log = env.root.path().join("launch.log");
    env.write_script(
        "my-editor",
        &format!("echo launched >> '{}'", launch_log.display()),
    );
    env.write_script(
        "hyprctl",
        &format!(
            r#"if [ "$1" = "-j" ] && [ "$2" = "clients" ]; then
  cat <<'JSON'
[
  {{"address":"0xbbb","mapped":true,"class":"code","initialClass":"code","title":"DesktopStem","workspace":{{"id":2,"name":"2"}},"focusHistoryID":3}},
  {{"address":"0xddd","mapped":true,"class":"Code-OSS","initialClass":"Code-OSS","title":"Unfocused","workspace":{{"id":5,"name":"5"}},"focusHistoryID":-1}},
  {{"address":"0xeee","mapped":true,"class":"code","initialClass":"code","title":"MissingMRU","workspace":{{"id":6,"name":"6"}}}},
  {{"address":"0xaaa","mapped":true,"class":"Code-OSS","initialClass":"Code-OSS","title":"Newest","workspace":{{"id":4,"name":"4"}},"focusHistoryID":0}},
  {{"address":"0xccc","mapped":true,"class":"other","initialClass":"other","title":"Code Editor Decoy","workspace":{{"id":1,"name":"1"}},"focusHistoryID":1}}
]
JSON
  exit 0
fi
printf '%s\n' "$*" >> '{}'
echo ok"#,
            hypr_log.display()
        ),
    );

    env.write_desktop(
        false,
        "code.desktop",
        "[Desktop Entry]\nType=Application\nName=Code\nStartupWMClass=Code-OSS\nExec=my-editor\n",
    );

    let mut sidecar = env.spawn_cli(&["--standalone"]);
    let search = sidecar.request(json!({"id": 1, "op": "search", "query": "code"}));
    let windows = search["data"]["apps"][0]["windows"].as_array().unwrap();
    // Matched by StartupWMClass OR desktop ID (not decoy title!), with nonnegative MRU first
    // and negative/missing focusHistoryID clients ordered after them.
    assert_eq!(windows.len(), 4);
    assert_eq!(windows[0]["id"], "0xaaa");
    assert_eq!(windows[0]["workspace"], "4");
    assert_eq!(windows[1]["id"], "0xbbb");
    assert!(
        (windows[2]["id"] == "0xddd" && windows[3]["id"] == "0xeee")
            || (windows[2]["id"] == "0xeee" && windows[3]["id"] == "0xddd")
    );

    // open focuses MRU window via Hyprland Lua repl when alwaysNew=false.
    sidecar.request(json!({"id": 2, "op": "perform", "action": "open", "appId": "code.desktop"}));
    let log_text = fs::read_to_string(&hypr_log).unwrap();
    assert!(log_text.contains("repl "));
    assert!(log_text.contains("hl.dsp.focus({window=\"address:0xaaa\"})"));
    assert!(!launch_log.exists());

    // window rejects invalid/unknown addresses and accepts known address.
    let bad_win =
        sidecar.request(json!({"id": 3, "op": "perform", "action": "window", "target": "0x9999"}));
    assert!(bad_win["error"].is_string());
    let bad_inj = sidecar
        .request(json!({"id": 4, "op": "perform", "action": "window", "target": "0xaaa;id"}));
    assert!(bad_inj["error"].is_string());

    // workspace validates positive integer and switches workspace before launching.
    let bad_ws = sidecar.request(json!({"id": 5, "op": "perform", "action": "workspace", "appId": "code.desktop", "target": "0"}));
    assert!(bad_ws["error"].is_string());
    let bad_ws2 = sidecar.request(json!({"id": 6, "op": "perform", "action": "workspace", "appId": "code.desktop", "target": "3; rm -rf /"}));
    assert!(bad_ws2["error"].is_string());

    let ok_ws = sidecar.request(json!({"id": 7, "op": "perform", "action": "workspace", "appId": "code.desktop", "target": "3"}));
    assert!(ok_ws["data"].is_object());
    let log_text = fs::read_to_string(&hypr_log).unwrap();
    assert!(log_text.contains("hl.dsp.focus({workspace=\"3\"})"));

    // Wait briefly for detached child to write launch_log.
    for _ in 0..20 {
        if launch_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(launch_log.exists());

    // quit closes only the app's matched windows via Lua repl.
    sidecar.request(json!({"id": 8, "op": "perform", "action": "quit", "appId": "code.desktop"}));
    let log_text = fs::read_to_string(&hypr_log).unwrap();
    assert!(log_text.contains("hl.dsp.window.close({window=\"address:0xaaa\"})"));
    assert!(log_text.contains("hl.dsp.window.close({window=\"address:0xbbb\"})"));
    assert!(log_text.contains("hl.dsp.window.close({window=\"address:0xddd\"})"));
    assert!(log_text.contains("hl.dsp.window.close({window=\"address:0xeee\"})"));
    assert!(!log_text.contains("0xccc"));
}

#[test]
fn terminal_launch_without_shell_copy_command_edit_and_missing_exec() {
    let env = TestEnv::new();
    let term_log = env.root.path().join("term.log");
    env.write_script(
        "mock-term",
        &format!("printf '%s\\n' \"$@\" > '{}'", term_log.display()),
    );
    env.write_script("tui-tool", "echo noisy-stdout-ignored");

    env.write_desktop(
        false,
        "tui.desktop",
        "[Desktop Entry]\nType=Application\nName=TUI Tool\nIcon=tui-icon\nTerminal=true\nExec=tui-tool %i --name=%c --file=%k %%done %U\n",
    );
    env.write_desktop(
        false,
        "broken.desktop",
        "[Desktop Entry]\nType=Application\nName=Broken App\nExec=/nonexistent/binary/xyz123\n",
    );

    let mut sidecar = env.spawn_cli(&["--standalone"]);

    // copyCommand expands field codes cleanly.
    let copy = sidecar.request(
        json!({"id": 1, "op": "perform", "action": "copyCommand", "appId": "tui.desktop"}),
    );
    let copied = copy["data"]["text"].as_str().unwrap();
    assert!(copied.contains("--icon tui-icon"));
    assert!(copied.contains("'--name=TUI Tool'"));
    assert!(copied.contains("tui.desktop"));
    assert!(copied.contains("%done"));
    assert!(!copied.contains("%U"));

    // Missing executable returns error and does not record usage.
    let fail = sidecar
        .request(json!({"id": 2, "op": "perform", "action": "open", "appId": "broken.desktop"}));
    assert!(fail["error"].is_string());

    // Configure custom terminal with args (no shell) and launch Terminal=true app.
    sidecar.request(json!({
        "id": 3,
        "op": "preferences",
        "values": {"terminal": "mock-term --title app"}
    }));
    let ok = sidecar
        .request(json!({"id": 4, "op": "perform", "action": "open", "appId": "tui.desktop"}));
    assert!(ok["data"].is_object());

    for _ in 0..20 {
        if term_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let argv_lines: Vec<String> = fs::read_to_string(&term_log)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(argv_lines[0], "--title");
    assert_eq!(argv_lines[1], "app");
    assert_eq!(argv_lines[2], "tui-tool");
    assert!(!argv_lines.iter().any(|a| a == "/bin/sh" || a == "-c"));

    // Invalid configured terminal returns error cleanly.
    sidecar.request(json!({
        "id": 5,
        "op": "preferences",
        "values": {"terminal": "nonexistent-terminal-xyz"}
    }));
    let term_err = sidecar
        .request(json!({"id": 6, "op": "perform", "action": "open", "appId": "tui.desktop"}));
    assert!(term_err["error"].is_string());

    // edit copies system desktop entry to user applications dir without clobbering existing overrides or symlinks.
    let _ = sidecar
        .request(json!({"id": 7, "op": "perform", "action": "edit", "appId": "tui.desktop"}));
    let user_tui = env.data_home.join("applications/tui.desktop");
    assert!(user_tui.is_file());
    fs::write(
        &user_tui,
        "[Desktop Entry]\nType=Application\nName=Custom Override\nExec=tui-tool\n",
    )
    .unwrap();
    let _ = sidecar
        .request(json!({"id": 8, "op": "perform", "action": "edit", "appId": "tui.desktop"}));
    assert!(
        fs::read_to_string(&user_tui)
            .unwrap()
            .contains("Custom Override")
    );

    // Symlink in user applications dir is not clobbered by copy_no_clobber.
    let link_target = env.root.path().join("protected.desktop");
    fs::write(&link_target, "UNTOUCHED").unwrap();
    let user_sym = env.data_home.join("applications/sym.desktop");
    std::os::unix::fs::symlink(&link_target, &user_sym).unwrap();
    env.write_desktop(
        false,
        "sym.desktop",
        "[Desktop Entry]\nType=Application\nName=Sym App\nExec=tui-tool\n",
    );
    let _ = sidecar
        .request(json!({"id": 81, "op": "perform", "action": "edit", "appId": "sym.desktop"}));
    assert_eq!(fs::read_to_string(&link_target).unwrap(), "UNTOUCHED");

    // new skips private/incognito actions in favor of canonical NewWindow.
    let action_log = env.root.path().join("action.log");
    env.write_script(
        "browser-bin",
        &format!("printf '%s\\n' \"$@\" >> '{}'", action_log.display()),
    );
    env.write_desktop(
        false,
        "browser.desktop",
        "[Desktop Entry]\nType=Application\nName=Browser\nExec=browser-bin\nActions=new-private-window;NewWindow;\n\n[Desktop Action new-private-window]\nName=New Private Window\nExec=browser-bin --private\n\n[Desktop Action NewWindow]\nName=New Window\nExec=browser-bin --new-window\n",
    );
    sidecar.request(json!({"id": 9, "op": "perform", "action": "new", "appId": "browser.desktop"}));
    for _ in 0..20 {
        if action_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let launched_args = fs::read_to_string(&action_log).unwrap_or_default();
    assert!(launched_args.contains("--new-window"));
    assert!(!launched_args.contains("--private"));
}

#[test]
fn offline_packages_search_info_and_interactive_install() {
    let env = TestEnv::new();
    let term_log = env.root.path().join("install_term.log");
    env.write_script(
        "kitty",
        &format!("printf '%s\\n' \"$@\" > '{}'", term_log.display()),
    );
    env.write_script("pacman", "exit 0");
    env.write_script("sudo", "exit 0");
    env.write_script("yay", "exit 0");
    env.write_script("flatpak", "exit 0");

    // Official repo fixture.
    write_pacman_sync_db(
        &env.pacman_sync.join("extra.db"),
        &[("obsidian", "1.7.7-1", "Markdown knowledge base")],
    );

    // Flatpak appstream fixture.
    let appstream_dir = env
        .data_home
        .join("flatpak/appstream/flathub/x86_64/active");
    fs::create_dir_all(&appstream_dir).unwrap();
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<components version="0.8">
  <component type="desktop-application">
    <id>md.obsidian.Obsidian</id>
    <name>Obsidian</name>
    <summary>Note-taking app</summary>
    <releases><release version="1.7.7"/></releases>
  </component>
</components>"#;
    {
        let f = File::create(appstream_dir.join("appstream.xml.gz")).unwrap();
        let mut enc = GzEncoder::new(f, Compression::default());
        enc.write_all(xml.as_bytes()).unwrap();
        enc.finish().unwrap();
    }

    // AUR yay completion.cache fixture.
    let yay_dir = env.cache_home.join("yay");
    fs::create_dir_all(&yay_dir).unwrap();
    fs::write(yay_dir.join("completion.cache"), "obsidian-bin\tAUR\n").unwrap();

    let mut sidecar = env.spawn_cli(&["--standalone"]);

    // Query < 3 chars omits packages.
    let short = sidecar.request(json!({"id": 1, "op": "search", "query": "ob"}));
    assert!(short["data"]["packages"].as_array().unwrap().is_empty());

    // All mode (packages: false) omits packages even for >= 3 chars.
    let all_mode =
        sidecar.request(json!({"id": 2, "op": "search", "query": "obsidian", "packages": false}));
    assert!(all_mode["data"]["packages"].as_array().unwrap().is_empty());

    // Normal search >= 3 chars returns Official > Flatpak > AUR.
    let pkg_search = sidecar.request(json!({"id": 3, "op": "search", "query": "obsidian"}));
    let pkgs = pkg_search["data"]["packages"].as_array().unwrap();
    assert_eq!(pkgs.len(), 3);
    assert_eq!(pkgs[0]["source"], "extra");
    assert_eq!(pkgs[0]["command"], "sudo pacman -S obsidian");
    assert_eq!(pkgs[1]["source"], "Flatpak");
    assert_eq!(
        pkgs[1]["command"],
        "flatpak install flathub md.obsidian.Obsidian"
    );
    assert_eq!(pkgs[2]["source"], "AUR");
    assert_eq!(pkgs[2]["command"], "yay -S obsidian-bin");

    // info for package candidate.
    let info = sidecar.request(json!({
        "id": 4,
        "op": "perform",
        "action": "info",
        "packageId": "obsidian",
        "source": "extra"
    }));
    let info_text = info["data"]["text"].as_str().unwrap();
    assert!(info_text.contains("1.7.7-1"));
    assert!(info_text.contains("extra"));

    // install rejects unsafe package IDs.
    let bad = sidecar.request(json!({
        "id": 5,
        "op": "perform",
        "action": "install",
        "packageId": "obsidian;id",
        "source": "extra"
    }));
    assert!(bad["error"].is_string());

    // install rejects uncached package IDs and mismatched sources cleanly.
    let uncached = sidecar.request(json!({
        "id": 51,
        "op": "perform",
        "action": "install",
        "packageId": "uncached-pkg",
        "source": "extra"
    }));
    assert!(uncached["error"].is_string());

    // install opens terminal with interactive pacman command (no --noconfirm, no shell).
    sidecar.request(json!({
        "id": 60,
        "op": "preferences",
        "values": {"terminal": "kitty"}
    }));
    let ok = sidecar.request(json!({
        "id": 6,
        "op": "perform",
        "action": "install",
        "packageId": "obsidian",
        "source": "extra"
    }));
    assert!(ok["data"].is_object());
    for _ in 0..20 {
        if term_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let argv: Vec<String> = fs::read_to_string(&term_log)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(argv, vec!["sudo", "pacman", "-S", "obsidian"]);
}

#[test]
fn resident_daemon_proxies_matching_env_and_isolates_mismatched_xdg_env() {
    let env = TestEnv::new();
    env.write_script("demo-bin", "exit 0");
    env.write_desktop(
        false,
        "demo.desktop",
        "[Desktop Entry]\nType=Application\nName=Daemon Demo\nExec=demo-bin\n",
    );

    let mut daemon_cmd = Command::new(env!("CARGO_BIN_EXE_remya-launcher-apps"));
    daemon_cmd
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    env.apply_env(&mut daemon_cmd);
    let _daemon = ChildGuard(daemon_cmd.spawn().unwrap());

    let sock_path = env.runtime_dir.join("remya-launcher/apps.sock");
    for _ in 0..40 {
        if sock_path.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(sock_path.exists(), "daemon did not create apps.sock");

    // Matching env proxies through daemon socket and sees demo.desktop.
    let mut proxy_cli = env.spawn_cli(&[]);
    let reply = proxy_cli.request(json!({"id": 77, "op": "search", "query": "daemon"}));
    assert_eq!(reply["id"], 77);
    assert_eq!(reply["data"]["apps"][0]["id"], "demo.desktop");

    // Different XDG_DATA_DIRS falls back to local isolated index even with same XDG_RUNTIME_DIR.
    let alt_root = TempDir::new().unwrap();
    let alt_sys = alt_root.path().join("sys_data");
    let alt_data = alt_root.path().join("data_home");
    fs::create_dir_all(alt_sys.join("applications")).unwrap();
    fs::create_dir_all(alt_data.join("applications")).unwrap();

    let mut isolated_cmd = Command::new(env!("CARGO_BIN_EXE_remya-launcher-apps"));
    isolated_cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    env.apply_env(&mut isolated_cmd);
    isolated_cmd
        .env("XDG_DATA_DIRS", &alt_sys)
        .env("XDG_DATA_HOME", &alt_data);
    let mut isolated_child = isolated_cmd.spawn().unwrap();
    let mut iso = SidecarHandle {
        stdout: BufReader::new(isolated_child.stdout.take().unwrap()),
        child: isolated_child,
    };
    let iso_reply = iso.request(json!({"id": 88, "op": "search", "query": ""}));
    assert_eq!(iso_reply["id"], 88);
    assert_eq!(iso_reply["data"]["total"], 0);

    // Different HYPRLAND_INSTANCE_SIGNATURE falls back to isolated local instance.
    let mut hypr_iso_cmd = Command::new(env!("CARGO_BIN_EXE_remya-launcher-apps"));
    hypr_iso_cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    env.apply_env(&mut hypr_iso_cmd);
    hypr_iso_cmd
        .env("HYPRLAND_INSTANCE_SIGNATURE", "different-signature")
        .env("XDG_DATA_DIRS", &alt_sys)
        .env("XDG_DATA_HOME", &alt_data);
    let mut hypr_iso_child = hypr_iso_cmd.spawn().unwrap();
    let mut hypr_iso = SidecarHandle {
        stdout: BufReader::new(hypr_iso_child.stdout.take().unwrap()),
        child: hypr_iso_child,
    };
    let hypr_iso_reply = hypr_iso.request(json!({"id": 89, "op": "search", "query": ""}));
    assert_eq!(hypr_iso_reply["data"]["total"], 0);
}

#[test]
fn proxy_handles_large_catalog_replies_above_request_cap() {
    let env = TestEnv::new();
    env.write_script("big-bin", "exit 0");
    let long_comment = "x".repeat(3500);
    for idx in 0..90 {
        env.write_desktop(
            false,
            &format!("big-{idx:03}.desktop"),
            &format!(
                "[Desktop Entry]\nType=Application\nName=Big App {idx:03}\nComment={long_comment}\nExec=big-bin\n"
            ),
        );
    }

    let mut daemon_cmd = Command::new(env!("CARGO_BIN_EXE_remya-launcher-apps"));
    daemon_cmd
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    env.apply_env(&mut daemon_cmd);
    let _daemon = ChildGuard(daemon_cmd.spawn().unwrap());

    let sock_path = env.runtime_dir.join("remya-launcher/apps.sock");
    for _ in 0..40 {
        if sock_path.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(sock_path.exists());

    let mut proxy_cli = env.spawn_cli(&[]);
    let reply = proxy_cli.request(json!({"id": 1, "op": "search", "query": ""}));
    assert_eq!(reply["id"], 1);
    assert_eq!(reply["data"]["total"], 90);
}

#[test]
fn stats_persistence_failure_does_not_fail_successful_launch() {
    let env = TestEnv::new();
    let launch_log = env.root.path().join("launched.log");
    env.write_script("ok-bin", &format!("echo ok >> '{}'", launch_log.display()));
    env.write_desktop(
        false,
        "okapp.desktop",
        "[Desktop Entry]\nType=Application\nName=Ok App\nExec=ok-bin\n",
    );

    let mut sidecar = env.spawn_cli(&["--standalone"]);
    let _ = sidecar.request(json!({"id": 0, "op": "search", "query": ""}));
    // Make state dir read-only after startup so persisting frecency fails.
    let state_dir = env.state_home.join("remya/launcher");
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o500)).unwrap();

    let reply = sidecar
        .request(json!({"id": 1, "op": "perform", "action": "open", "appId": "okapp.desktop"}));
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700)).unwrap();

    assert!(reply["error"].is_null(), "unexpected error reply: {reply}");
    assert!(
        reply["data"]["warning"].is_string(),
        "expected separate warning: {reply}"
    );

    for _ in 0..20 {
        if launch_log.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(launch_log.exists());
}
