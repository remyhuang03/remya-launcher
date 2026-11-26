use std::collections::HashSet;
use std::fs;
use std::io::{Read, Seek};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::ffi::ErrorCode;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params};
use url::Url;

use super::{
    Entry, HISTORY_BATCH_SIZE, MAX_HISTORY_ENTRIES, MAX_HISTORY_SCAN_ROWS, MAX_TITLE_CHARS,
    MAX_VISIT_COUNT, SQLITE_BUSY_TIMEOUT_MS, WEBKIT_EPOCH_OFFSET_US, check_owned_file_with_meta,
    map_io_err, open_owned_readonly_file, path_present, read_bounded_prefix, sanitize_text,
};

const MAX_CHROMIUM_FAVICONS_DB_BYTES: u64 = 64 * 1024 * 1024;
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

fn sidecar_path(db_canon: &Path, suffix: &str) -> PathBuf {
    let mut aux_os = db_canon.as_os_str().to_os_string();
    aux_os.push(suffix);
    PathBuf::from(aux_os)
}

fn check_optional_sidecar(
    db_canon: &Path,
    suffix: &str,
) -> Result<Option<fs::Metadata>, &'static str> {
    let aux_path = sidecar_path(db_canon, suffix);
    match fs::symlink_metadata(&aux_path) {
        Ok(_) => check_owned_file_with_meta(&aux_path).map(|(_, meta)| Some(meta)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(map_io_err(err)),
    }
}

pub(super) fn with_readonly_sqlite<T>(
    path: &Path,
    op: impl FnOnce(&Transaction<'_>) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    with_readonly_sqlite_timeout(path, Duration::from_millis(SQLITE_BUSY_TIMEOUT_MS), op)
}

fn with_readonly_sqlite_timeout<T>(
    path: &Path,
    busy_timeout: Duration,
    op: impl FnOnce(&Transaction<'_>) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    let (canon_db, pre_meta) = check_owned_file_with_meta(path)?;
    let wal_meta = check_optional_sidecar(&canon_db, "-wal")?;
    let shm_meta = check_optional_sidecar(&canon_db, "-shm")?;
    let journal_meta = check_optional_sidecar(&canon_db, "-journal")?;

    // Reject active or unrecovered rollback journals rather than reading torn pages.
    if journal_meta.as_ref().is_some_and(|m| m.len() > 0) {
        return Err("SOURCE_LOCKED");
    }

    let header = read_bounded_prefix(&canon_db, 100)?;
    let has_sqlite_header = header.len() >= 20 && header.starts_with(SQLITE_HEADER);
    if !has_sqlite_header && !(header.is_empty() && wal_meta.is_some() && shm_meta.is_some()) {
        return Err("UNSUPPORTED_SCHEMA");
    }
    let is_wal_header = has_sqlite_header && header[18] == 2 && header[19] == 2;

    // Never allow SQLite OPEN_READ_ONLY to synthesize -shm or -wal in the browser directory:
    // - Live WAL+SHM: use normal read-only transaction directly on existing WAL+SHM.
    // - Nonempty WAL without SHM, or SHM without WAL: reject as retryable SOURCE_LOCKED.
    // - Closed/stale WAL with no active WAL and no SHM: open with immutable=1 and post-verify.
    let use_immutable = match (wal_meta.as_ref(), shm_meta.as_ref()) {
        (Some(_), Some(_)) => false,
        (Some(wm), None) if wm.len() > 0 => return Err("SOURCE_LOCKED"),
        (None, Some(_)) => return Err("SOURCE_LOCKED"),
        (_, None) => is_wal_header,
    };

    let mut conn = if use_immutable {
        let mut uri = Url::from_file_path(&canon_db).map_err(|()| "SOURCE_UNAVAILABLE")?;
        uri.set_query(Some("immutable=1"));
        Connection::open_with_flags(
            uri.as_str(),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
    } else {
        Connection::open_with_flags(
            &canon_db,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NO_MUTEX
                | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
    }
    .map_err(|err| classify_sqlite_error(&err))?;

    conn.busy_timeout(busy_timeout)
        .map_err(|err| classify_sqlite_error(&err))?;
    let result = in_readonly_transaction(&mut conn, op);
    drop(conn);

    if use_immutable {
        let post_db_ok = check_owned_file_with_meta(&canon_db).is_ok_and(|(_, post_meta)| {
            (post_meta.dev(), post_meta.ino(), post_meta.len())
                == (pre_meta.dev(), pre_meta.ino(), pre_meta.len())
                && post_meta.modified().ok() == pre_meta.modified().ok()
        });
        let post_shm_absent = matches!(check_optional_sidecar(&canon_db, "-shm"), Ok(None));
        let post_wal_inactive = match check_optional_sidecar(&canon_db, "-wal") {
            Ok(None) => true,
            Ok(Some(m)) => m.len() == 0,
            Err(_) => false,
        };
        let post_journal_inactive = match check_optional_sidecar(&canon_db, "-journal") {
            Ok(None) => true,
            Ok(Some(m)) => m.len() == 0,
            Err(_) => false,
        };
        if !post_db_ok || !post_shm_absent || !post_wal_inactive || !post_journal_inactive {
            return Err("SOURCE_LOCKED");
        }
    }

    result
}

pub(super) fn with_chromium_favicons<T>(
    path: &Path,
    op: impl Fn(&Transaction<'_>) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    match with_readonly_sqlite_timeout(path, Duration::ZERO, &op) {
        Err("SOURCE_LOCKED") => {
            let mut snapshot = chromium_favicons_snapshot(path)?;
            in_readonly_transaction(&mut snapshot, op)
        }
        result => result,
    }
}

fn chromium_favicons_snapshot(path: &Path) -> Result<Connection, &'static str> {
    let (mut source, meta) = open_owned_readonly_file(path)?;
    let mut header = [0; 100];
    source.read_exact(&mut header).map_err(map_io_err)?;
    if !header.starts_with(SQLITE_HEADER) || header[18..20] != [1, 1] {
        return Err("SOURCE_LOCKED");
    }
    if meta.len() > MAX_CHROMIUM_FAVICONS_DB_BYTES {
        return Err("SOURCE_TOO_LARGE");
    }
    let journal = chromium_rollback_state(path)?;

    source.rewind().map_err(map_io_err)?;
    let mut snapshot = Connection::open_in_memory().map_err(|err| classify_sqlite_error(&err))?;
    snapshot
        .deserialize_read_exact("main", source, meta.len() as usize, true)
        .map_err(|err| classify_sqlite_error(&err))?;

    let (mut current, current_meta) = open_owned_readonly_file(path)?;
    let mut current_header = [0; 100];
    current
        .read_exact(&mut current_header)
        .map_err(map_io_err)?;
    if header != current_header
        || file_stamp(&meta) != file_stamp(&current_meta)
        || journal != chromium_rollback_state(path)?
    {
        return Err("SOURCE_LOCKED");
    }
    Ok(snapshot)
}

#[derive(PartialEq, Eq)]
struct FileStamp {
    identity: (u64, u64),
    len: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

fn file_stamp(meta: &fs::Metadata) -> FileStamp {
    FileStamp {
        identity: (meta.dev(), meta.ino()),
        len: meta.len(),
        modified: (meta.mtime(), meta.mtime_nsec()),
        changed: (meta.ctime(), meta.ctime_nsec()),
    }
}

fn chromium_rollback_state(path: &Path) -> Result<Option<(FileStamp, [u8; 28])>, &'static str> {
    if path_present(&sidecar_path(path, "-wal"))? || path_present(&sidecar_path(path, "-shm"))? {
        return Err("SOURCE_LOCKED");
    }
    let journal_path = sidecar_path(path, "-journal");
    if !path_present(&journal_path)? {
        return Ok(None);
    }
    let (mut journal, meta) = open_owned_readonly_file(&journal_path)?;
    let mut header = [0; 28];
    if meta.len() != 0 {
        journal.read_exact(&mut header).map_err(map_io_err)?;
        // With Chromium's synced rollback journal, SQLite sets the magic and page count before writing DB pages.
        if header[..12] != [0; 12] {
            return Err("SOURCE_LOCKED");
        }
    }
    Ok(Some((file_stamp(&meta), header)))
}

fn in_readonly_transaction<T>(
    conn: &mut Connection,
    op: impl FnOnce(&Transaction<'_>) -> Result<T, &'static str>,
) -> Result<T, &'static str> {
    conn.pragma_update(None, "query_only", "ON")
        .map_err(|err| classify_sqlite_error(&err))?;
    conn.pragma_update(None, "trusted_schema", "OFF")
        .map_err(|err| classify_sqlite_error(&err))?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Deferred)
        .map_err(|err| classify_sqlite_error(&err))?;
    op(&tx)
}

pub(super) fn classify_sqlite_error(err: &rusqlite::Error) -> &'static str {
    match err {
        rusqlite::Error::SqliteFailure(ffi_err, _) => match ffi_err.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => "SOURCE_LOCKED",
            _ if ffi_err.extended_code == rusqlite::ffi::SQLITE_PROTOCOL => "SOURCE_LOCKED",
            ErrorCode::PermissionDenied
            | ErrorCode::AuthorizationForStatementDenied
            | ErrorCode::ReadOnly => "PERMISSION_DENIED",
            ErrorCode::CannotOpen => "SOURCE_UNAVAILABLE",
            _ => "UNSUPPORTED_SCHEMA",
        },
        rusqlite::Error::SqlInputError { .. }
        | rusqlite::Error::InvalidColumnName(_)
        | rusqlite::Error::InvalidColumnIndex(_)
        | rusqlite::Error::InvalidColumnType(_, _, _) => "UNSUPPORTED_SCHEMA",
        _ => "UNSUPPORTED_SCHEMA",
    }
}

fn table_exists(tx: &Transaction<'_>, table: &str) -> Result<bool, &'static str> {
    let mut stmt = tx
        .prepare(
            "SELECT 1 FROM sqlite_master WHERE type IN ('table', 'view') AND name = ?1 LIMIT 1",
        )
        .map_err(|err| classify_sqlite_error(&err))?;
    let mut rows = stmt
        .query(params![table])
        .map_err(|err| classify_sqlite_error(&err))?;
    match rows.next() {
        Ok(Some(_)) => Ok(true),
        Ok(None) => Ok(false),
        Err(err) => Err(classify_sqlite_error(&err)),
    }
}

fn table_columns(
    tx: &Transaction<'_>,
    allowed_table: &str,
) -> Result<HashSet<String>, &'static str> {
    // Allowlist table identifiers so PRAGMA table_info never formats arbitrary input.
    let pragma_sql = match allowed_table {
        "moz_places" => "PRAGMA table_info('moz_places')",
        "moz_bookmarks" => "PRAGMA table_info('moz_bookmarks')",
        "moz_pages_w_icons" => "PRAGMA table_info('moz_pages_w_icons')",
        "moz_icons_to_pages" => "PRAGMA table_info('moz_icons_to_pages')",
        "moz_icons" => "PRAGMA table_info('moz_icons')",
        "urls" => "PRAGMA table_info('urls')",
        "icon_mapping" => "PRAGMA table_info('icon_mapping')",
        "favicons" => "PRAGMA table_info('favicons')",
        "favicon_bitmaps" => "PRAGMA table_info('favicon_bitmaps')",
        _ => return Err("UNSUPPORTED_SCHEMA"),
    };
    let mut stmt = tx
        .prepare(pragma_sql)
        .map_err(|err| classify_sqlite_error(&err))?;
    let mut rows = stmt.query([]).map_err(|err| classify_sqlite_error(&err))?;
    let mut cols = HashSet::new();
    while let Some(row) = rows.next().map_err(|err| classify_sqlite_error(&err))? {
        if let Ok(name) = row.get::<_, String>(1) {
            cols.insert(name);
        }
    }
    Ok(cols)
}

pub(super) fn require_columns(
    tx: &Transaction<'_>,
    table: &str,
    required: &[&str],
) -> Result<HashSet<String>, &'static str> {
    if !table_exists(tx, table)? {
        return Err("UNSUPPORTED_SCHEMA");
    }
    let cols = table_columns(tx, table)?;
    if required.iter().any(|col| !cols.contains(*col)) {
        return Err("UNSUPPORTED_SCHEMA");
    }
    Ok(cols)
}

pub(super) struct HistoryQuerySpec {
    pub(super) table: &'static str,
    pub(super) ts_col: &'static str,
    pub(super) cutoff_raw: i64,
    pub(super) cutoff_ms: i64,
    pub(super) convert_ts: fn(i64, i64) -> Option<i64>,
}

pub(super) fn read_history_table(
    tx: &Transaction<'_>,
    spec: HistoryQuerySpec,
    now_ms: i64,
) -> Result<Vec<Entry>, &'static str> {
    let cols = require_columns(
        tx,
        spec.table,
        &["id", "url", "title", "visit_count", spec.ts_col],
    )?;
    let hidden_clause = if cols.contains("hidden") {
        "AND COALESCE(hidden, 0) = 0"
    } else {
        ""
    };
    let table = spec.table;
    let ts_col = spec.ts_col;
    let sql = format!(
        "SELECT id, url, COALESCE(title, ''), COALESCE(visit_count, 0), {ts_col} \
         FROM {table} \
         WHERE visit_count > 0 \
           AND TYPEOF(id) = 'integer' \
           AND TYPEOF({ts_col}) = 'integer' \
           AND {ts_col} >= ?1 \
           {hidden_clause} \
           AND (?2 IS NULL OR {ts_col} < ?2 OR ({ts_col} = ?2 AND id < ?3)) \
         ORDER BY {ts_col} DESC, id DESC \
         LIMIT ?4"
    );
    let mut stmt = tx
        .prepare(&sql)
        .map_err(|err| classify_sqlite_error(&err))?;

    let mut entries = Vec::new();
    let mut cursor_ts: Option<i64> = None;
    let mut cursor_id: Option<i64> = None;
    let mut scanned = 0usize;

    while entries.len() < MAX_HISTORY_ENTRIES && scanned < MAX_HISTORY_SCAN_ROWS {
        let prev_cursor = (cursor_ts, cursor_id);
        let remaining = (MAX_HISTORY_ENTRIES - entries.len()) as i64;
        let limit = remaining.min(HISTORY_BATCH_SIZE);
        let mut rows = stmt
            .query(params![spec.cutoff_raw, cursor_ts, cursor_id, limit])
            .map_err(|err| classify_sqlite_error(&err))?;

        let mut fetched = 0i64;
        while let Some(row) = rows.next().map_err(|err| classify_sqlite_error(&err))? {
            fetched += 1;
            scanned += 1;
            let Some(id) = row_i64(row, 0) else {
                continue;
            };
            let Some(raw_ts) = row_i64(row, 4) else {
                continue;
            };
            cursor_ts = Some(raw_ts);
            cursor_id = Some(id);

            let Some(raw_url) = row_opt_string(row, 1) else {
                continue;
            };
            let Some(valid_url) = crate::index::validate_url(&raw_url) else {
                continue;
            };
            let Some(last_visited_ms) =
                (spec.convert_ts)(raw_ts, now_ms).filter(|ts| *ts >= spec.cutoff_ms)
            else {
                continue;
            };
            let raw_title = row_opt_string(row, 2).unwrap_or_default();
            let title = sanitize_text(&raw_title, MAX_TITLE_CHARS);
            let visited_count = row_i64(row, 3).unwrap_or(1).clamp(1, MAX_VISIT_COUNT);

            entries.push(Entry {
                source_id: format!("h:{id}"),
                bookmark: false,
                url: valid_url.to_string(),
                title,
                visited_count,
                last_visited_ms: Some(last_visited_ms),
                folder_path: Vec::new(),
            });
        }
        if fetched < limit || (cursor_ts, cursor_id) == prev_cursor {
            break;
        }
    }

    Ok(entries)
}

pub(super) fn prtime_to_unix_ms(us: i64, now_ms: i64) -> Option<i64> {
    if us <= 0 {
        return None;
    }
    let ms = us / 1000;
    if ms < 0 {
        return None;
    }
    Some(ms.min(now_ms))
}

pub(super) fn chromium_us_to_unix_ms(us: i64, now_ms: i64) -> Option<i64> {
    if us <= 0 {
        return None;
    }
    let ms = (i128::from(us) - WEBKIT_EPOCH_OFFSET_US) / 1000;
    i64::try_from(ms)
        .ok()
        .filter(|v| *v >= 0)
        .map(|v| v.min(now_ms))
}

pub(super) fn unix_ms_to_chromium_us(ms: i64) -> i64 {
    let us = i128::from(ms.max(0)) * 1000 + WEBKIT_EPOCH_OFFSET_US;
    i64::try_from(us).unwrap_or(i64::MAX)
}

pub(super) fn row_i64(row: &rusqlite::Row<'_>, idx: usize) -> Option<i64> {
    match row.get_ref(idx).ok()? {
        ValueRef::Integer(v) => Some(v),
        _ => None,
    }
}

pub(super) fn row_opt_string(row: &rusqlite::Row<'_>, idx: usize) -> Option<String> {
    match row.get_ref(idx).ok()? {
        ValueRef::Text(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        ValueRef::Null => None,
        _ => None,
    }
}
