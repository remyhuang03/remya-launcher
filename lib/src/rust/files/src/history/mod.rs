mod providers;
mod xbel;

use crate::model::{Entry, Policy, decode_id};
use providers::selected;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    fs::{self, DirBuilder, File, OpenOptions},
    io::ErrorKind,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use xbel::Book;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Kind {
    File,
    Directory,
}

impl Kind {
    fn of(path: &Path) -> Option<Self> {
        let meta = path.metadata().ok()?;
        if meta.is_dir() {
            Some(Self::Directory)
        } else if meta.is_file() {
            Some(Self::File)
        } else {
            None
        }
    }
    fn filename(self) -> &'static str {
        match self {
            Self::File => "files.xbel",
            Self::Directory => "directories.xbel",
        }
    }
}
const KINDS: [Kind; 2] = [Kind::File, Kind::Directory];

#[derive(Default)]
pub struct Snapshot {
    pub recent: Vec<PathBuf>,
    pub pinned: Vec<PathBuf>,
    pub scores: HashMap<PathBuf, f64>,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn data_home() -> Result<PathBuf, String> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join(".local/share"))
        })
        .ok_or_else(|| "HistoryUnavailable".into())
}

fn private_dir(dir: &Path) -> Result<(), String> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|_| "HistoryUnavailable")?;
    let meta = dir.symlink_metadata().map_err(|_| "HistoryUnavailable")?;
    if !meta.is_dir()
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o077 != 0
    {
        return Err("HistoryDirectoryNotPrivate".into());
    }
    Ok(())
}

fn locked(dir: &Path) -> Result<File, String> {
    private_dir(dir)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(dir.join("history.lock"))
        .map_err(|_| "HistoryUnavailable")?;
    file.lock().map_err(|_| "HistoryUnavailable")?;
    Ok(file)
}

fn db_error(_: rusqlite::Error) -> String {
    "HistoryDatabaseUnavailable".into()
}
fn db(dir: &Path, write: bool) -> Result<Option<Connection>, String> {
    let path = dir.join("history.sqlite3");
    if write {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(_) => return Err("HistoryUnavailable".into()),
        }
    }
    match path.symlink_metadata() {
        Ok(m) if m.is_file() => {}
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        _ => return Err("HistoryUnavailable".into()),
    }
    let flags = if write {
        OpenFlags::SQLITE_OPEN_READ_WRITE
    } else {
        OpenFlags::SQLITE_OPEN_READ_ONLY
    };
    let conn = Connection::open_with_flags(path, flags | OpenFlags::SQLITE_OPEN_NOFOLLOW)
        .map_err(db_error)?;
    if write {
        conn.execute_batch("CREATE TABLE IF NOT EXISTS pins(path BLOB PRIMARY KEY NOT NULL);
        CREATE TABLE IF NOT EXISTS suppressed(path BLOB PRIMARY KEY NOT NULL);
        CREATE TABLE IF NOT EXISTS receipts(id TEXT PRIMARY KEY NOT NULL, stamp INTEGER NOT NULL, local TEXT NOT NULL, external TEXT NOT NULL);").map_err(db_error)?;
    }
    Ok(Some(conn))
}

fn paths(conn: &Connection, query: &str) -> Result<Vec<PathBuf>, String> {
    let mut stmt = conn.prepare(query).map_err(db_error)?;
    stmt.query_map([], |row| {
        row.get::<_, Vec<u8>>(0)
            .map(|b| PathBuf::from(OsString::from_vec(b)))
    })
    .map_err(db_error)?
    .collect::<Result<_, _>>()
    .map_err(db_error)
}

fn cutoff(cfg: &Value, time: i64) -> i64 {
    time - cfg["retention_days"].as_i64().unwrap_or(90).clamp(1, 3650) * 86400
}
fn max_records(cfg: &Value) -> usize {
    cfg["max_records_per_kind"]
        .as_u64()
        .unwrap_or(10_000)
        .clamp(1, 10_000) as usize
}

pub fn snapshot(preferences: &Value, policy: &Policy) -> Result<Snapshot, String> {
    let home = data_home()?;
    collect(&home, &preferences["files-history"], policy)
}

pub fn mark_pins(entries: &mut [Entry]) -> Result<(), String> {
    let Some(conn) = db(&data_home()?.join("remya/launcher/files"), false)? else {
        return Ok(());
    };
    conn.busy_timeout(Duration::ZERO).map_err(db_error)?;
    let mut stmt = conn
        .prepare("SELECT 1 FROM pins WHERE path=?1")
        .map_err(db_error)?;
    for entry in entries {
        let path = decode_id(&entry.id)?;
        if stmt
            .exists([path.as_os_str().as_bytes()])
            .map_err(db_error)?
        {
            entry.pinned = Some(true);
        }
    }
    Ok(())
}

fn collect(home: &Path, cfg: &Value, policy: &Policy) -> Result<Snapshot, String> {
    let dir = home.join("remya/launcher/files");
    let mut result = Snapshot::default();
    let conn = db(&dir, false)?;
    if let Some(conn) = &conn {
        result.pinned = paths(conn, "SELECT path FROM pins ORDER BY rowid")?
            .into_iter()
            .filter(|p| policy.allowed(p) && Kind::of(p).is_some())
            .collect();
    }
    if cfg["enabled"] != true {
        providers::invalidate();
        return Ok(result);
    }
    let suppressed: HashSet<_> = match conn {
        Some(conn) => paths(&conn, "SELECT path FROM suppressed")?
            .into_iter()
            .collect(),
        None => HashSet::new(),
    };
    let permitted = |p: &Path| !suppressed.contains(p) && policy.allowed(p);
    let time = now();
    let cutoff = cutoff(cfg, time);
    let mut times = HashMap::<PathBuf, i64>::new();
    let mut sources = vec![
        (dir.join(Kind::File.filename()), Some(Kind::File)),
        (dir.join(Kind::Directory.filename()), Some(Kind::Directory)),
    ];
    if cfg["read_system_xbel"] == true {
        sources.push((home.join("recently-used.xbel"), None));
    }
    for (source, source_kind) in sources {
        let Ok(book) = Book::load(&source) else {
            continue;
        };
        let mut entries = book.entries(source_kind.is_none());
        if source_kind.is_some() && entries.len() > max_records(cfg) {
            entries.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            entries.truncate(max_records(cfg));
        }
        for (path, stamp, count) in entries {
            if stamp < cutoff || !permitted(&path) {
                continue;
            }
            let Some(kind) = Kind::of(&path) else {
                continue;
            };
            if source_kind.is_some_and(|k| k != kind) {
                continue;
            }
            let stamp = stamp.min(time);
            times
                .entry(path.clone())
                .and_modify(|s| *s = (*s).max(stamp))
                .or_insert(stamp);
            let recent = 2f64.powf(-(time - stamp) as f64 / (86400.0 * 14.0));
            let score = if count == 0 {
                recent
            } else {
                0.55 * (f64::from(count.min(10_000)).ln_1p() / 10_000f64.ln_1p()) + 0.45 * recent
            };
            result
                .scores
                .entry(path)
                .and_modify(|s| *s = s.max(score))
                .or_insert(score);
        }
    }
    if cfg["external_enabled"] == true {
        for kind in KINDS {
            if selected(cfg, kind) == "xbel" {
                continue;
            }
            if let Ok(rows) = providers::rows(kind, cfg, false) {
                for (path, score, stamp) in rows.iter() {
                    if !permitted(path) || Kind::of(path) != Some(kind) {
                        continue;
                    }
                    result.scores.insert(path.clone(), *score);
                    if let Some(stamp) = stamp.filter(|s| *s >= cutoff).map(|s| s.min(time)) {
                        times
                            .entry(path.clone())
                            .and_modify(|s| *s = (*s).max(stamp))
                            .or_insert(stamp);
                    }
                }
            }
        }
    } else {
        providers::invalidate();
    }
    let mut times: Vec<_> = times.into_iter().collect();
    times.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    result.recent = times.into_iter().map(|(p, _)| p).collect();
    Ok(result)
}

fn status(cfg: &Value, dir: &Path, kind: Kind, refresh: bool) -> Value {
    let selected = selected(cfg, kind);
    let (effective, health, reason) = if cfg["enabled"] != true {
        ("none", "disabled", "history_disabled")
    } else {
        let fallback = if Book::load(&dir.join(kind.filename())).is_ok() {
            "xbel"
        } else {
            "none"
        };
        if selected == "xbel" {
            if fallback == "xbel" {
                ("xbel", "ready", "")
            } else {
                ("none", "unavailable", "invalid_xbel")
            }
        } else if cfg["external_enabled"] != true {
            (fallback, "disabled", "consent_required")
        } else {
            let result = if refresh {
                providers::rows(kind, cfg, true).map(|_| "ready")
            } else if let Some(cached) = providers::cached_health(kind, cfg) {
                cached.map(|_| "ready")
            } else {
                providers::probe(selected).map(|_| "available")
            };
            match result {
                Ok(health) => (selected, health, ""),
                Err(reason) => (fallback, "unavailable", reason),
            }
        }
    };
    json!({"selected":selected,"effective":effective,"health":health,"reason":reason})
}

pub fn dispatch(req: &Value, policy: &Policy) -> Result<Value, String> {
    let home = data_home()?;
    dispatch_at(req, policy, &home)
}

fn dispatch_at(req: &Value, policy: &Policy, home: &Path) -> Result<Value, String> {
    let dir = home.join("remya/launcher/files");
    let op = req["op"].as_str().ok_or("InvalidRequest")?;
    let cfg = &req["preferences"]["files-history"];
    if matches!(op, "history.status" | "history.refresh") {
        let refresh = op == "history.refresh";
        return Ok(
            json!({"file":status(cfg,&dir,Kind::File,refresh),"directory":status(cfg,&dir,Kind::Directory,refresh)}),
        );
    }
    let _lock = locked(&dir)?;
    let conn = db(&dir, true)?.ok_or("HistoryUnavailable")?;
    if op == "history.clear" {
        match req["scope"].as_str() {
            Some("launcher") => {
                for kind in KINDS {
                    match fs::remove_file(dir.join(kind.filename())) {
                        Ok(()) => {}
                        Err(e) if e.kind() == ErrorKind::NotFound => {}
                        Err(_) => return Err("HistoryWriteFailed".into()),
                    }
                }
                // Receipts contain only opaque operation IDs. Retain them to prevent duplicate writes.
            }
            Some("suppression_rules") => {
                conn.execute("DELETE FROM suppressed", [])
                    .map_err(db_error)?;
            }
            _ => return Err("InvalidHistoryScope".into()),
        }
        providers::invalidate();
        return Ok(json!({"cleared":true}));
    }
    let path = decode_id(req["entryId"].as_str().ok_or("InvalidEntry")?)?;
    if !policy.allowed(&path) {
        return Err("PathExcluded".into());
    }
    let key = path.as_os_str().as_bytes();
    match op {
        "history.pin" => {
            Kind::of(&path).ok_or("EntryExpired")?;
            conn.execute("INSERT OR IGNORE INTO pins(path) VALUES(?1)", [key])
                .map_err(db_error)?;
            Ok(json!({"pinned":true}))
        }
        "history.unpin" => {
            conn.execute("DELETE FROM pins WHERE path=?1", [key])
                .map_err(db_error)?;
            Ok(json!({"pinned":false}))
        }
        "history.forget" => {
            conn.execute("INSERT OR IGNORE INTO suppressed(path) VALUES(?1)", [key])
                .map_err(db_error)?;
            for kind in KINDS {
                let file = dir.join(kind.filename());
                let mut book = Book::load(&file)?;
                if book.move_item(&path, None)? {
                    book.save(&file)?;
                }
            }
            Ok(json!({"forgotten":true}))
        }
        _ => Err("UnknownHistoryOperation".into()),
    }
}

fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err("InvalidOperationId".into());
    }
    Ok(())
}

pub fn record(
    preferences: &Value,
    policy: &Policy,
    path: &Path,
    operation_id: &str,
    editor: bool,
) -> Result<Value, String> {
    let cfg = &preferences["files-history"];
    if cfg["enabled"] != true || cfg["record_local"] != true || !policy.allowed(path) {
        return Ok(json!({"local":"skipped","external":"skipped"}));
    }
    let dir = data_home()?.join("remya/launcher/files");
    record_at(&dir, cfg, path, operation_id, editor)
}

fn record_at(
    dir: &Path,
    cfg: &Value,
    path: &Path,
    id: &str,
    editor: bool,
) -> Result<Value, String> {
    validate_id(id)?;
    let kind = Kind::of(path).ok_or("EntryExpired")?;
    let _lock = locked(dir)?;
    let conn = db(dir, true)?.ok_or("HistoryUnavailable")?;
    let existing = conn
        .query_row(
            "SELECT local,external FROM receipts WHERE id=?1",
            [id],
            |r| Ok(json!({"local":r.get::<_,String>(0)?,"external":r.get::<_,String>(1)?})),
        )
        .optional()
        .map_err(db_error)?;
    if let Some(receipt) = existing {
        return Ok(receipt);
    }
    let time = now();
    // Claim before either side effect. A process that dies here leaves an unknown outcome.
    conn.execute(
        "INSERT INTO receipts VALUES(?1,?2,'unknown','skipped')",
        params![id, time],
    )
    .map_err(db_error)?;
    let filename = dir.join(kind.filename());
    let mut book = Book::load(&filename)?;
    let suppressed: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM suppressed WHERE path=?1)",
            [path.as_os_str().as_bytes()],
            |row| row.get(0),
        )
        .map_err(db_error)?;
    if suppressed {
        // A prior forget may have stopped after persisting suppression.
        book.move_item(path, None)?;
    }
    book.record(path, time, cutoff(cfg, time), max_records(cfg))?;
    book.save(&filename)?;
    conn.execute(
        "DELETE FROM suppressed WHERE path=?1",
        [path.as_os_str().as_bytes()],
    )
    .map_err(db_error)?;
    conn.execute(
        "UPDATE receipts SET local='recorded',external='unknown' WHERE id=?1",
        [id],
    )
    .map_err(db_error)?;
    let external = providers::note(kind, cfg, path, editor);
    conn.execute(
        "UPDATE receipts SET external=?2 WHERE id=?1",
        [id, external],
    )
    .map_err(db_error)?;
    conn.execute("DELETE FROM receipts WHERE stamp < ?1", [time - 7 * 86400])
        .map_err(db_error)?;
    providers::invalidate();
    Ok(json!({"local":"recorded","external":external}))
}

pub fn renamed(from: &Path, to: &Path, preferences: &Value) -> Result<(), String> {
    let dir = data_home()?.join("remya/launcher/files");
    if !dir.try_exists().map_err(|_| "HistoryUnavailable")? {
        return Ok(());
    }
    let _lock = locked(&dir)?;
    let mut conn = db(&dir, true)?.ok_or("HistoryUnavailable")?;
    let tx = conn.transaction().map_err(db_error)?;
    let enabled = preferences["files-history"]["enabled"] == true;
    for table in ["pins", "suppressed"] {
        if table == "suppressed" && !enabled {
            continue;
        }
        for path in paths(&tx, &format!("SELECT path FROM {table}"))? {
            if let Ok(relative) = path.strip_prefix(from) {
                let dest = if relative.as_os_str().is_empty() {
                    to.to_owned()
                } else {
                    to.join(relative)
                };
                tx.execute(
                    &format!("INSERT OR IGNORE INTO {table} VALUES(?1)"),
                    [dest.as_os_str().as_bytes()],
                )
                .map_err(db_error)?;
                tx.execute(
                    &format!("DELETE FROM {table} WHERE path=?1"),
                    [path.as_os_str().as_bytes()],
                )
                .map_err(db_error)?;
            }
        }
    }
    tx.commit().map_err(db_error)?;
    if enabled {
        for kind in KINDS {
            let file = dir.join(kind.filename());
            let mut book = Book::load(&file)?;
            let mut changed = false;
            for (path, _, _) in book.entries(false) {
                if let Ok(relative) = path.strip_prefix(from) {
                    let dest = if relative.as_os_str().is_empty() {
                        to.to_owned()
                    } else {
                        to.join(relative)
                    };
                    changed |= book.move_item(&path, Some(&dest))?;
                }
            }
            if changed {
                book.save(&file)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::path_id;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[test]
    fn local_actions_suppression_and_pins_survive_reopening() {
        let root = tempdir().unwrap();
        let dir = root.path().join("remya/launcher/files");
        let name = OsString::from_vec(vec![b'd', 0xff, b'\n', b'a']);
        let path = root.path().join(name);
        fs::write(&path, []).unwrap();
        let cfg = json!({"enabled":true,"record_local":true,"external_enabled":false});
        let prefs = json!({"files-history":cfg});
        let policy = Policy::new(&prefs);
        let event = |op: &str| json!({"op":op,"preferences":prefs,"entryId":path_id(&path)});
        let record = |id: &str| record_at(&dir, &cfg, &path, id, false).unwrap();
        assert_eq!(record("first")["external"], "skipped");
        assert_eq!(record("first")["local"], "recorded");
        assert_eq!(
            Book::load(&dir.join("files.xbel")).unwrap().entries(false)[0].2,
            1
        );
        record("second");
        assert_eq!(
            Book::load(&dir.join("files.xbel")).unwrap().entries(false)[0].2,
            2
        );
        assert_eq!(
            fs::metadata(dir.join("files.xbel"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let folder = root.path().join("folder");
        fs::create_dir(&folder).unwrap();
        record_at(&dir, &cfg, &folder, "dir", false).unwrap();
        assert_eq!(
            Book::load(&dir.join("directories.xbel"))
                .unwrap()
                .entries(false)[0]
                .0,
            folder
        );
        dispatch_at(&event("history.pin"), &policy, root.path()).unwrap();
        let off = collect(root.path(), &json!({"enabled":false}), &policy).unwrap();
        assert_eq!(off.pinned, std::slice::from_ref(&path));
        assert!(off.recent.is_empty() && off.scores.is_empty());
        let shared = root.path().join("recently-used.xbel");
        fs::copy(dir.join("files.xbel"), &shared).unwrap();
        let bytes = fs::read(&shared).unwrap();
        let shared_cfg = json!({"enabled":true,"read_system_xbel":true});
        dispatch_at(&event("history.forget"), &policy, root.path()).unwrap();
        assert!(
            !collect(root.path(), &shared_cfg, &policy)
                .unwrap()
                .scores
                .contains_key(&path)
        );
        record("third");
        assert!(
            collect(root.path(), &cfg, &policy)
                .unwrap()
                .recent
                .contains(&path)
        );
        assert_eq!(
            Book::load(&dir.join("files.xbel")).unwrap().entries(false)[0].2,
            1
        );
        dispatch_at(
            &json!({"op":"history.clear","scope":"launcher"}),
            &policy,
            root.path(),
        )
        .unwrap();
        let snap = collect(root.path(), &cfg, &policy).unwrap();
        assert!(snap.recent.is_empty());
        assert_eq!(snap.pinned, std::slice::from_ref(&path));
        assert_eq!(
            collect(root.path(), &shared_cfg, &policy).unwrap().recent,
            std::slice::from_ref(&path)
        );
        assert_eq!(fs::read(&shared).unwrap(), bytes);
        dispatch_at(&event("history.unpin"), &policy, root.path()).unwrap();
        assert!(
            collect(root.path(), &cfg, &policy)
                .unwrap()
                .pinned
                .is_empty()
        );
    }

    #[test]
    fn shared_xbel_rejects_private_remote_and_excluded_entries() {
        let root = tempdir().unwrap();
        let cfg = json!({"enabled":true,"read_system_xbel":true});
        let policy =
            Policy::new(&json!({"files-search":{"exclusions":[root.path().join("excluded")]}}));
        let mut bookmarks = String::new();
        for (name, private) in [("public", false), ("private", true), ("excluded", false)] {
            let path = root.path().join(name);
            fs::write(&path, []).unwrap();
            let uri = url::Url::from_file_path(path).unwrap();
            let flag = if private { "<bookmark:private/>" } else { "" };
            bookmarks.push_str(&format!(r#"<bookmark href="{uri}" visited="2090-11-18T10:15:00Z"><info><metadata owner="http://freedesktop.org">{flag}</metadata></info></bookmark>"#));
        }
        bookmarks
            .push_str(r#"<bookmark href="sftp://remote/path" visited="2090-11-18T10:15:00Z"/>"#);
        fs::write(root.path().join("recently-used.xbel"),format!(r#"<?xml version="1.0"?><xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">{bookmarks}</xbel>"#)).unwrap();
        assert_eq!(
            collect(root.path(), &cfg, &policy).unwrap().recent,
            [root.path().join("public")]
        );
        assert!(
            collect(root.path(), &json!({}), &policy)
                .unwrap()
                .recent
                .is_empty()
        );
        assert!(!root.path().join("remya").exists());
    }

    #[test]
    fn corrupt_projection_is_preserved_and_uncertain_operation_is_not_replayed() {
        let root = tempdir().unwrap();
        let dir = root.path().join("history");
        private_dir(&dir).unwrap();
        let path = root.path().join("doc");
        fs::write(&path, []).unwrap();
        let book = dir.join("files.xbel");
        fs::write(&book, b"corrupt own XBEL").unwrap();
        let cfg = json!({"enabled":true,"record_local":true});
        assert!(record_at(&dir, &cfg, &path, "same", false).is_err());
        assert_eq!(fs::read(&book).unwrap(), b"corrupt own XBEL");
        assert_eq!(
            record_at(&dir, &cfg, &path, "same", false).unwrap()["local"],
            "unknown"
        );
        fs::remove_file(book).unwrap();
        assert_eq!(
            record_at(&dir, &cfg, &path, "same", false).unwrap()["local"],
            "unknown"
        );
        assert!(dir.join("files.xbel").try_exists().is_ok_and(|e| !e));
    }
}
