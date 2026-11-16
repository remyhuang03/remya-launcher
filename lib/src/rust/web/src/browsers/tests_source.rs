use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use rusqlite::{Connection, params};
use tempfile::TempDir;

use super::discovery::profile_id;
use super::sqlite::unix_ms_to_chromium_us;
use super::tests_favicon::make_png;
use super::{Family, NINETY_DAYS_MS, Profile, current_unix_ms, favicon, read};

fn unix_now_ms() -> i64 {
    current_unix_ms()
}

pub(super) fn create_gecko_db(path: &Path, wal_mode: bool) -> Connection {
    let conn = Connection::open(path).unwrap();
    if wal_mode {
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    }
    conn.execute_batch(
        "CREATE TABLE moz_places (
                id INTEGER PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT,
                visit_count INTEGER DEFAULT 0,
                hidden INTEGER DEFAULT 0,
                frecency INTEGER DEFAULT 0,
                last_visit_date INTEGER
            );
            CREATE TABLE moz_bookmarks (
                id INTEGER PRIMARY KEY,
                type INTEGER,
                fk INTEGER,
                parent INTEGER,
                position INTEGER,
                title TEXT,
                dateAdded INTEGER
            );",
    )
    .unwrap();
    conn
}

pub(super) fn create_chromium_history_db(path: &Path, wal_mode: bool) -> Connection {
    let conn = Connection::open(path).unwrap();
    if wal_mode {
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    }
    conn.execute_batch(
        "CREATE TABLE urls (
                id INTEGER PRIMARY KEY,
                url TEXT NOT NULL,
                title TEXT,
                visit_count INTEGER DEFAULT 0,
                typed_count INTEGER DEFAULT 0,
                last_visit_time INTEGER DEFAULT 0,
                hidden INTEGER DEFAULT 0
            );",
    )
    .unwrap();
    conn
}

#[test]
fn gecko_adapter_reads_wal_history_bookmarks_folders_and_clamps_timestamps() {
    let tmp = TempDir::new().unwrap();
    let prof_dir = tmp.path().join("ff-prof");
    fs::create_dir_all(&prof_dir).unwrap();
    let db_path = prof_dir.join("places.sqlite");

    let now_ms = unix_now_ms();
    let recent_us = (now_ms - 5_000) * 1000;
    let future_us = (now_ms + 86_400_000) * 1000;
    let stale_us = (now_ms - NINETY_DAYS_MS - 10_000) * 1000;
    let old_bookmark_us = (now_ms - NINETY_DAYS_MS * 4) * 1000;

    let conn = create_gecko_db(&db_path, true);
    // Seed initial rows and checkpoint once, then insert uncheckpointed WAL rows.
    conn.execute(
        "INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date) VALUES
             (1, 'https://example.com/recent', '  Recent\n\tTitle  ', 3, 0, ?1),
             (2, 'https://example.com/stale', 'Too Old', 5, 0, ?2),
             (3, 'https://example.com/future', 'Future Visit', 2, 0, ?3),
             (4, 'https://example.com/hidden', 'Hidden', 4, 1, ?1),
             (5, 'javascript:alert(1)', 'XSS', 2, 0, ?1),
             (6, 'https://user:secret@example.com/auth', 'Creds', 2, 0, ?1),
             (7, 'https://docs.rs/sqlite', 'Docs.rs', 0, 0, ?4)",
        params![recent_us, stale_us, future_us, old_bookmark_us],
    )
    .unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(FULL);").unwrap();

    // Uncheckpointed WAL transaction: new history row + folder hierarchy + cyclic folder
    conn.execute(
        "INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date) VALUES
             (8, 'https://wal-committed.example.org/', 'WAL Row', 4, 0, ?1)",
        params![recent_us + 1000],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO moz_bookmarks (id, type, fk, parent, position, title, dateAdded) VALUES
             (1, 2, NULL, 0, 0, '', ?1),
             (2, 2, NULL, 1, 0, 'Bookmarks Menu', ?1),
             (3, 2, NULL, 2, 0, 'Dev', ?1),
             (4, 2, NULL, 3, 0, 'Rust', ?1),
             (5, 1, 7, 4, 0, 'Rust SQLite', ?2),
             (6, 2, NULL, 7, 0, 'CycleA', ?1),
             (7, 2, NULL, 6, 0, 'CycleB', ?1),
             (8, 1, 1, 7, 0, 'Cyclic Bookmark', ?1)",
        params![recent_us, old_bookmark_us],
    )
    .unwrap();
    // Keep `conn` open so `-wal` stays uncheckpointed while `read()` runs!

    let profile = Profile {
        id: profile_id("firefox", &prof_dir),
        browser_id: "firefox".into(),
        browser_label: "Firefox".into(),
        label: "default".into(),
        path: prof_dir.clone(),
        family: Family::Gecko,
    };

    let entries = read(&profile).unwrap();

    let wal_entry = entries
        .iter()
        .find(|e| !e.bookmark && e.url == "https://wal-committed.example.org/")
        .expect("uncheckpointed WAL row must be visible");
    assert_eq!(wal_entry.title, "WAL Row");
    assert_eq!(wal_entry.visited_count, 4);

    let recent_entry = entries
        .iter()
        .find(|e| !e.bookmark && e.url == "https://example.com/recent")
        .unwrap();
    assert_eq!(recent_entry.title, "Recent Title");

    let future_entry = entries
        .iter()
        .find(|e| !e.bookmark && e.url == "https://example.com/future")
        .unwrap();
    assert!(future_entry.last_visited_ms.unwrap() <= unix_now_ms());

    assert!(!entries.iter().any(|e| e.url.contains("/stale")));
    assert!(!entries.iter().any(|e| e.url.contains("/hidden")));
    assert!(!entries.iter().any(|e| e.url.starts_with("javascript:")));
    assert!(!entries.iter().any(|e| e.url.contains("secret")));

    let bm = entries
        .iter()
        .find(|e| e.bookmark && e.url == "https://docs.rs/sqlite")
        .expect("old bookmark must remain indexed");
    assert_eq!(bm.folder_path, vec!["Bookmarks Menu", "Dev", "Rust"]);
    assert_eq!(bm.last_visited_ms, Some(old_bookmark_us / 1000));
    assert!(
        !entries
            .iter()
            .any(|e| e.bookmark && e.title == "Cyclic Bookmark")
    );
    drop(conn);
}

#[test]
fn chromium_adapter_reads_wal_history_and_bookmarks_folders_and_classifies_errors() {
    let tmp = TempDir::new().unwrap();
    let prof_dir = tmp.path().join("chr-prof");
    fs::create_dir_all(&prof_dir).unwrap();
    let db_path = prof_dir.join("History");

    let now_ms = unix_now_ms();
    let recent_webkit = unix_ms_to_chromium_us(now_ms - 10_000);
    let future_webkit = unix_ms_to_chromium_us(now_ms + 90_000_000);
    let stale_webkit = unix_ms_to_chromium_us(now_ms - NINETY_DAYS_MS - 60_000);
    let old_bm_webkit = unix_ms_to_chromium_us(now_ms - NINETY_DAYS_MS * 3);

    let conn = create_chromium_history_db(&db_path, true);
    conn.execute(
        "INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden) VALUES
             (1, 'https://chromium.org/', 'Chromium', 7, ?1, 0),
             (2, 'https://stale.example.com/', 'Stale', 3, ?2, 0),
             (3, 'https://future.example.com/', 'Future', 2, ?3, 0)",
        params![recent_webkit, stale_webkit, future_webkit],
    )
    .unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(FULL);").unwrap();
    conn.execute(
        "INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden) VALUES
             (4, 'https://wal.chromium.example/', 'WAL Chrome', 2, ?1, 0)",
        params![recent_webkit + 2000],
    )
    .unwrap();

    fs::write(
        prof_dir.join("Bookmarks"),
        format!(
            r#"{{
                    "version": 1,
                    "roots": {{
                        "bookmark_bar": {{
                            "type": "folder",
                            "name": "Bookmarks bar",
                            "children": [
                                {{
                                    "type": "folder",
                                    "name": "Engineering",
                                    "children": [
                                        {{
                                            "type": "url",
                                            "guid": "bm-1",
                                            "name": "Rust Book",
                                            "url": "https://doc.rust-lang.org/book/",
                                            "date_added": "{old_bm_webkit}"
                                        }},
                                        {{
                                            "type": "url",
                                            "guid": "bm-future",
                                            "name": "Future BM",
                                            "url": "https://future-bm.example/",
                                            "date_added": "{future_webkit}"
                                        }}
                                    ]
                                }}
                            ]
                        }}
                    }}
                }}"#
        ),
    )
    .unwrap();

    let profile = Profile {
        id: profile_id("chromium", &prof_dir),
        browser_id: "chromium".into(),
        browser_label: "Chromium".into(),
        label: "Default".into(),
        path: prof_dir.clone(),
        family: Family::Chromium,
    };

    let entries = read(&profile).unwrap();
    assert!(
        entries
            .iter()
            .any(|e| !e.bookmark && e.url == "https://wal.chromium.example/")
    );
    assert!(!entries.iter().any(|e| e.url.contains("stale.example.com")));
    let future_hist = entries
        .iter()
        .find(|e| !e.bookmark && e.url == "https://future.example.com/")
        .unwrap();
    assert!(future_hist.last_visited_ms.unwrap() <= unix_now_ms());

    let bm = entries
        .iter()
        .find(|e| e.bookmark && e.url == "https://doc.rust-lang.org/book/")
        .unwrap();
    assert_eq!(bm.folder_path, vec!["Bookmarks bar", "Engineering"]);
    assert_eq!(bm.last_visited_ms, Some(now_ms - NINETY_DAYS_MS * 3));

    let bm_future = entries
        .iter()
        .find(|e| e.bookmark && e.url == "https://future-bm.example/")
        .unwrap();
    assert!(bm_future.last_visited_ms.unwrap() <= unix_now_ms());

    // Exclusive lock classification -> SOURCE_LOCKED
    conn.execute_batch("PRAGMA locking_mode = EXCLUSIVE; BEGIN EXCLUSIVE;")
        .unwrap();
    assert_eq!(read(&profile), Err("SOURCE_LOCKED"));
    conn.execute_batch("ROLLBACK;").unwrap();
    drop(conn);

    // Corrupt Bookmarks JSON must not wipe valid History entries
    fs::write(prof_dir.join("Bookmarks"), "{not valid json").unwrap();
    let after_bad_bm = read(&profile).unwrap();
    assert!(
        after_bad_bm
            .iter()
            .any(|e| !e.bookmark && e.url == "https://chromium.org/")
    );
    assert!(!after_bad_bm.iter().any(|e| e.bookmark));

    // Missing required column -> UNSUPPORTED_SCHEMA
    let bad_dir = tmp.path().join("bad-chr");
    fs::create_dir_all(&bad_dir).unwrap();
    let bad_conn = Connection::open(bad_dir.join("History")).unwrap();
    bad_conn
        .execute_batch("CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT);")
        .unwrap();
    drop(bad_conn);
    let bad_profile = Profile {
        id: profile_id("chromium", &bad_dir),
        browser_id: "chromium".into(),
        browser_label: "Chromium".into(),
        label: "Default".into(),
        path: bad_dir,
        family: Family::Chromium,
    };
    assert_eq!(read(&bad_profile), Err("UNSUPPORTED_SCHEMA"));
}

fn dir_entry_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn closed_and_stale_wal_read_without_creating_sidecars() {
    let tmp = TempDir::new().unwrap();
    let now_ms = unix_now_ms();
    let recent_us = (now_ms - 5_000) * 1000;

    // 1. Closed Gecko WAL DB (clean checkpoint + close -> no -wal, no -shm)
    let ff_dir = tmp.path().join("ff-closed-wal");
    fs::create_dir_all(&ff_dir).unwrap();
    let places_path = ff_dir.join("places.sqlite");
    {
        let conn = create_gecko_db(&places_path, true);
        conn.execute(
            "INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
                 VALUES (1, 'https://closed-wal.example/', 'Closed WAL', 2, 0, ?1)",
            params![recent_us],
        )
        .unwrap();
    }
    assert!(!ff_dir.join("places.sqlite-wal").exists());
    assert!(!ff_dir.join("places.sqlite-shm").exists());

    // Also include WAL-mode favicons.sqlite with stale 0-byte -wal and no -shm.
    let fav_path = ff_dir.join("favicons.sqlite");
    {
        let fconn = Connection::open(&fav_path).unwrap();
        fconn.pragma_update(None, "journal_mode", "WAL").unwrap();
        fconn
                .execute_batch(
                    "CREATE TABLE moz_pages_w_icons (id INTEGER PRIMARY KEY, page_url TEXT);
                     CREATE TABLE moz_icons_to_pages (page_id INTEGER, icon_id INTEGER);
                     CREATE TABLE moz_icons (id INTEGER PRIMARY KEY, icon_url TEXT, width INTEGER, data BLOB);",
                )
                .unwrap();
        fconn
                .execute(
                    "INSERT INTO moz_pages_w_icons (id, page_url) VALUES (1, 'https://closed-wal.example/')",
                    [],
                )
                .unwrap();
        fconn
                .execute(
                    "INSERT INTO moz_icons (id, icon_url, width, data) VALUES (1, 'https://closed-wal.example/icon.png', 32, ?1)",
                    params![make_png(32, 32)],
                )
                .unwrap();
        fconn
            .execute(
                "INSERT INTO moz_icons_to_pages (page_id, icon_id) VALUES (1, 1)",
                [],
            )
            .unwrap();
    }
    fs::write(ff_dir.join("favicons.sqlite-wal"), b"").unwrap();
    assert!(!ff_dir.join("favicons.sqlite-shm").exists());

    // Lock directory to 0o500 so any SQLite attempt to create -shm or -wal fails at OS level.
    let before_entries = dir_entry_names(&ff_dir);
    fs::set_permissions(&ff_dir, fs::Permissions::from_mode(0o500)).unwrap();

    let ff_profile = Profile {
        id: profile_id("firefox", &ff_dir),
        browser_id: "firefox".into(),
        browser_label: "Firefox".into(),
        label: "default".into(),
        path: ff_dir.clone(),
        family: Family::Gecko,
    };
    let entries = read(&ff_profile).expect("closed WAL must read in read-only dir");
    assert!(
        entries
            .iter()
            .any(|e| e.url == "https://closed-wal.example/" && e.title == "Closed WAL")
    );
    assert!(favicon(&ff_profile, "https://closed-wal.example/").is_some());
    assert_eq!(dir_entry_names(&ff_dir), before_entries);

    fs::set_permissions(&ff_dir, fs::Permissions::from_mode(0o700)).unwrap();

    // 2. Closed Chromium WAL History DB with stale 0-byte -wal and no -shm
    let chr_dir = tmp.path().join("chr-stale-wal");
    fs::create_dir_all(&chr_dir).unwrap();
    let hist_path = chr_dir.join("History");
    {
        let conn = create_chromium_history_db(&hist_path, true);
        conn.execute(
            "INSERT INTO urls (id, url, title, visit_count, last_visit_time, hidden)
                 VALUES (1, 'https://stale-wal-chr.example/', 'Stale WAL', 3, ?1, 0)",
            params![unix_ms_to_chromium_us(now_ms - 5_000)],
        )
        .unwrap();
    }
    fs::write(chr_dir.join("History-wal"), b"").unwrap();
    let chr_before = dir_entry_names(&chr_dir);
    fs::set_permissions(&chr_dir, fs::Permissions::from_mode(0o500)).unwrap();

    let chr_profile = Profile {
        id: profile_id("chromium", &chr_dir),
        browser_id: "chromium".into(),
        browser_label: "Chromium".into(),
        label: "Default".into(),
        path: chr_dir.clone(),
        family: Family::Chromium,
    };
    let chr_entries = read(&chr_profile).expect("stale 0-byte WAL must read without -shm");
    assert!(
        chr_entries
            .iter()
            .any(|e| e.url == "https://stale-wal-chr.example/")
    );
    assert_eq!(dir_entry_names(&chr_dir), chr_before);
    fs::set_permissions(&chr_dir, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn inconsistent_wal_or_shm_returns_retryable_source_locked_without_side_files() {
    let tmp = TempDir::new().unwrap();
    let now_ms = unix_now_ms();
    let recent_us = (now_ms - 5_000) * 1000;

    // 1. Nonempty -wal with missing -shm -> Err("SOURCE_LOCKED"), never creates -shm
    let prof_dir = tmp.path().join("nonempty-wal-no-shm");
    fs::create_dir_all(&prof_dir).unwrap();
    let places_path = prof_dir.join("places.sqlite");
    let saved_wal: Vec<u8>;
    {
        let conn = create_gecko_db(&places_path, true);
        conn.execute(
            "INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
                 VALUES (1, 'https://base.example/', 'Base', 1, 0, ?1)",
            params![recent_us],
        )
        .unwrap();
        conn.execute_batch("PRAGMA wal_checkpoint(FULL);").unwrap();
        conn.execute(
            "INSERT INTO moz_places (id, url, title, visit_count, hidden, last_visit_date)
                 VALUES (2, 'https://uncheckpointed.example/', 'WAL Only', 2, 0, ?1)",
            params![recent_us + 1000],
        )
        .unwrap();
        saved_wal = fs::read(prof_dir.join("places.sqlite-wal")).unwrap();
        assert!(!saved_wal.is_empty());
    }
    // Restore non-empty -wal while keeping -shm absent.
    fs::write(prof_dir.join("places.sqlite-wal"), &saved_wal).unwrap();
    assert!(!prof_dir.join("places.sqlite-shm").exists());

    let profile = Profile {
        id: profile_id("firefox", &prof_dir),
        browser_id: "firefox".into(),
        browser_label: "Firefox".into(),
        label: "default".into(),
        path: prof_dir.clone(),
        family: Family::Gecko,
    };
    assert_eq!(read(&profile), Err("SOURCE_LOCKED"));
    assert!(!prof_dir.join("places.sqlite-shm").exists());

    // 2. -shm present while -wal is absent -> Err("SOURCE_LOCKED"), never creates -wal
    fs::remove_file(prof_dir.join("places.sqlite-wal")).unwrap();
    fs::write(prof_dir.join("places.sqlite-shm"), vec![0u8; 32]).unwrap();
    assert_eq!(read(&profile), Err("SOURCE_LOCKED"));
    assert!(!prof_dir.join("places.sqlite-wal").exists());
}
