use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use serde_json::{Value, json};
use tempfile::TempDir;

// Minimal valid 1x1 RGBA PNG.
pub const SAMPLE_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

pub struct TestEnv {
    pub root: TempDir,
    pub data_home: PathBuf,
    pub sys_data: PathBuf,
    pub config_home: PathBuf,
    pub state_home: PathBuf,
    pub cache_home: PathBuf,
    pub runtime_dir: PathBuf,
    pub bin_dir: PathBuf,
}

impl TestEnv {
    pub fn new() -> Self {
        let root = TempDir::new().unwrap();
        let p = root.path();
        let data_home = p.join("data_home");
        let sys_data = p.join("sys_data");
        let config_home = p.join("config_home");
        let state_home = p.join("state_home");
        let cache_home = p.join("cache_home");
        let runtime_dir = p.join("runtime");
        let bin_dir = p.join("bin");

        for dir in [
            data_home.join("applications"),
            sys_data.join("applications"),
            config_home.clone(),
            state_home.clone(),
            cache_home.clone(),
            runtime_dir.clone(),
            bin_dir.clone(),
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
        }
    }

    pub fn write_script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.bin_dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    pub fn write_desktop(&self, desktop_id: &str, contents: &str) -> PathBuf {
        let path = self.data_home.join("applications").join(desktop_id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, contents).unwrap();
        path
    }

    pub fn write_mimeapps(&self, default_desktop_id: &str) {
        let path = self.config_home.join("mimeapps.list");
        fs::write(
            path,
            format!(
                "[Default Applications]\nx-scheme-handler/https={default_desktop_id};\nx-scheme-handler/http={default_desktop_id};\n"
            ),
        )
        .unwrap();
    }

    pub fn apply_env(&self, cmd: &mut Command) {
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
            .env("PATH", path);
    }

    pub fn spawn(&self, args: &[&str]) -> SidecarHandle {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_remya-launcher-web"));
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.apply_env(&mut cmd);
        let mut child = cmd.spawn().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        SidecarHandle { child, stdout }
    }

    pub fn create_firefox_profile(
        &self,
        profile_name: &str,
        rel_dir: &str,
        urls: &[(&str, &str, i64, i64)],
        bookmarks: &[(&str, &str, &str)],
        favicon_url: Option<&str>,
    ) -> PathBuf {
        let ff_root = self.root.path().join(".mozilla/firefox");
        let prof_dir = ff_root.join(rel_dir);
        fs::create_dir_all(&prof_dir).unwrap();

        fs::write(
            ff_root.join("profiles.ini"),
            format!(
                "[General]\nStartWithLastProfile=1\n\n[Profile0]\nName={profile_name}\nIsRelative=1\nPath={rel_dir}\nDefault=1\n"
            ),
        )
        .unwrap();

        let places_path = prof_dir.join("places.sqlite");
        let conn = Connection::open(&places_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE moz_places (
                id INTEGER PRIMARY KEY,
                url TEXT,
                title TEXT,
                rev_host TEXT,
                visit_count INTEGER DEFAULT 0,
                hidden INTEGER DEFAULT 0,
                typed INTEGER DEFAULT 0,
                frecency INTEGER DEFAULT 0,
                last_visit_date INTEGER
            );
            CREATE TABLE moz_historyvisits (
                id INTEGER PRIMARY KEY,
                from_visit INTEGER,
                place_id INTEGER,
                visit_date INTEGER,
                visit_type INTEGER,
                session INTEGER
            );
            CREATE TABLE moz_bookmarks (
                id INTEGER PRIMARY KEY,
                type INTEGER,
                fk INTEGER,
                parent INTEGER,
                position INTEGER,
                title TEXT,
                dateAdded INTEGER,
                lastModified INTEGER
            );
            INSERT INTO moz_bookmarks(id, type, fk, parent, position, title) VALUES(1, 2, NULL, 0, 0, '');
            INSERT INTO moz_bookmarks(id, type, fk, parent, position, title) VALUES(2, 2, NULL, 1, 0, 'Bookmarks Menu');
            INSERT INTO moz_bookmarks(id, type, fk, parent, position, title) VALUES(3, 2, NULL, 1, 1, 'Bookmarks Toolbar');
            ",
        )
        .unwrap();

        for (idx, (url, title, visits, last_prtime_us)) in urls.iter().enumerate() {
            let id = (idx + 1) as i64;
            conn.execute(
                "INSERT INTO moz_places(id, url, title, visit_count, hidden, frecency, last_visit_date) VALUES(?1, ?2, ?3, ?4, 0, 100, ?5)",
                params![id, url, title, visits, last_prtime_us],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO moz_historyvisits(id, from_visit, place_id, visit_date, visit_type, session) VALUES(?1, 0, ?2, ?3, 1, 0)",
                params![id, id, last_prtime_us],
            )
            .unwrap();
        }

        let mut next_bkm_id = 100i64;
        for (folder, url, title) in bookmarks {
            let folder_id = next_bkm_id;
            next_bkm_id += 1;
            conn.execute(
                "INSERT INTO moz_bookmarks(id, type, fk, parent, position, title, dateAdded, lastModified) VALUES(?1, 2, NULL, 2, 0, ?2, 1700000000000000, 1700000000000000)",
                params![folder_id, folder],
            )
            .unwrap();
            let place_id: i64 = match conn.query_row(
                "SELECT id FROM moz_places WHERE url = ?1",
                params![url],
                |r| r.get(0),
            ) {
                Ok(id) => id,
                Err(_) => {
                    let pid = next_bkm_id;
                    next_bkm_id += 1;
                    conn.execute(
                        "INSERT INTO moz_places(id, url, title, visit_count, hidden, frecency, last_visit_date) VALUES(?1, ?2, ?3, 0, 0, 0, NULL)",
                        params![pid, url, title],
                    )
                    .unwrap();
                    pid
                }
            };
            let bkm_id = next_bkm_id;
            next_bkm_id += 1;
            conn.execute(
                "INSERT INTO moz_bookmarks(id, type, fk, parent, position, title, dateAdded, lastModified) VALUES(?1, 1, ?2, ?3, 0, ?4, 1700000000000000, 1700000000000000)",
                params![bkm_id, place_id, folder_id, title],
            )
            .unwrap();
        }

        if let Some(furl) = favicon_url {
            let fav_path = prof_dir.join("favicons.sqlite");
            let fconn = Connection::open(&fav_path).unwrap();
            fconn
                .execute_batch(
                    "
                    CREATE TABLE moz_pages_w_icons (id INTEGER PRIMARY KEY, page_url TEXT, page_url_hash INTEGER);
                    CREATE TABLE moz_icons_to_pages (page_id INTEGER, icon_id INTEGER, expire_ms INTEGER);
                    CREATE TABLE moz_icons (id INTEGER PRIMARY KEY, icon_url TEXT, fixed_icon_url_hash INTEGER, width INTEGER, root INTEGER, expire_ms INTEGER, data BLOB);
                    ",
                )
                .unwrap();
            fconn
                .execute(
                    "INSERT INTO moz_pages_w_icons(id, page_url, page_url_hash) VALUES(1, ?1, 1)",
                    params![furl],
                )
                .unwrap();
            fconn
                .execute(
                    "INSERT INTO moz_icons(id, icon_url, fixed_icon_url_hash, width, root, expire_ms, data) VALUES(1, ?1, 1, 64, 0, 9999999999999, ?2)",
                    params![format!("{furl}favicon.png"), SAMPLE_PNG],
                )
                .unwrap();
            fconn
                .execute(
                    "INSERT INTO moz_icons_to_pages(page_id, icon_id, expire_ms) VALUES(1, 1, 9999999999999)",
                    [],
                )
                .unwrap();
        }

        prof_dir
    }

    pub fn create_chromium_profile(
        &self,
        dir_name: &str,
        display_name: &str,
        urls: &[(&str, &str, i64, i64)],
    ) -> PathBuf {
        let user_data = self.config_home.join("chromium");
        let prof_dir = user_data.join(dir_name);
        fs::create_dir_all(&prof_dir).unwrap();

        fs::write(
            user_data.join("Local State"),
            json!({
                "profile": {
                    "info_cache": {
                        dir_name: { "name": display_name }
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let hist_path = prof_dir.join("History");
        let conn = Connection::open(&hist_path).unwrap();
        conn.execute_batch(
            "
            CREATE TABLE urls (
                id INTEGER PRIMARY KEY,
                url TEXT,
                title TEXT,
                visit_count INTEGER DEFAULT 0,
                typed_count INTEGER DEFAULT 0,
                last_visit_time INTEGER DEFAULT 0,
                hidden INTEGER DEFAULT 0
            );
            CREATE TABLE visits (
                id INTEGER PRIMARY KEY,
                url INTEGER,
                visit_time INTEGER,
                transition INTEGER
            );
            ",
        )
        .unwrap();

        for (idx, (url, title, visits, webkit_us)) in urls.iter().enumerate() {
            let id = (idx + 1) as i64;
            conn.execute(
                "INSERT INTO urls(id, url, title, visit_count, typed_count, last_visit_time, hidden) VALUES(?1, ?2, ?3, ?4, 1, ?5, 0)",
                params![id, url, title, visits, webkit_us],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO visits(id, url, visit_time, transition) VALUES(?1, ?2, ?3, 0)",
                params![id, id, webkit_us],
            )
            .unwrap();
        }

        prof_dir
    }
}

pub struct SidecarHandle {
    child: Child,
    stdout: BufReader<std::process::ChildStdout>,
}

impl SidecarHandle {
    pub fn request(&mut self, payload: Value) -> Value {
        let line = payload.to_string();
        self.request_raw(&line)
    }

    pub fn request_raw(&mut self, raw_line: &str) -> Value {
        let stdin = self.child.stdin.as_mut().unwrap();
        writeln!(stdin, "{raw_line}").unwrap();
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

pub fn wait_for_profile_status(
    sidecar: &mut SidecarHandle,
    profile_id: &str,
    wanted_statuses: &[&str],
) -> Value {
    for _ in 0..40 {
        let status = sidecar.request(json!({"id": 9000, "op": "web.sync.status"}));
        if let Some(profiles) = status["data"]["profiles"].as_array()
            && let Some(p) = profiles.iter().find(|p| p["profileId"] == profile_id)
            && let Some(st) = p["status"].as_str()
            && wanted_statuses.contains(&st)
        {
            return p.clone();
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for profile status {wanted_statuses:?}");
}

pub fn recent_prtime_us() -> i64 {
    let now_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as i64;
    now_us - 60_000_000
}

pub fn recent_webkit_us() -> i64 {
    recent_prtime_us() + 11_644_473_600_000_000
}
