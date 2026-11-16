use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Transaction, params};

use super::sqlite::{
    HistoryQuerySpec, classify_sqlite_error, prtime_to_unix_ms, read_history_table,
    require_columns, row_i64, row_opt_string, with_readonly_sqlite,
};
use super::{
    Entry, HISTORY_BATCH_SIZE, MAX_BOOKMARK_FOLDERS, MAX_BOOKMARK_SCAN_ROWS, MAX_BOOKMARKS,
    MAX_FOLDER_CHARS, MAX_FOLDER_DEPTH, MAX_TITLE_CHARS, NINETY_DAYS_MS, sanitize_text,
};

pub(super) fn read_gecko_profile(dir: &Path, now_ms: i64) -> Result<Vec<Entry>, &'static str> {
    let cutoff_ms = now_ms.saturating_sub(NINETY_DAYS_MS);
    with_readonly_sqlite(&dir.join("places.sqlite"), |tx| {
        let mut entries = read_history_table(
            tx,
            HistoryQuerySpec {
                table: "moz_places",
                ts_col: "last_visit_date",
                cutoff_raw: cutoff_ms.saturating_mul(1000),
                cutoff_ms,
                convert_ts: prtime_to_unix_ms,
            },
            now_ms,
        )?;

        if require_columns(
            tx,
            "moz_bookmarks",
            &["id", "fk", "parent", "type", "title", "dateAdded"],
        )
        .is_ok()
        {
            let bookmarks = read_gecko_bookmarks(tx, now_ms)?;
            entries.extend(bookmarks);
        }
        Ok(entries)
    })
}

fn read_gecko_bookmarks(tx: &Transaction<'_>, now_ms: i64) -> Result<Vec<Entry>, &'static str> {
    let mut folder_stmt = tx
        .prepare(
            "SELECT id, COALESCE(parent, 0), COALESCE(title, '') \
             FROM moz_bookmarks \
             WHERE type = 2 AND TYPEOF(id) = 'integer' \
             ORDER BY id ASC \
             LIMIT ?1",
        )
        .map_err(|err| classify_sqlite_error(&err))?;
    let mut folder_rows = folder_stmt
        .query(params![MAX_BOOKMARK_FOLDERS as i64])
        .map_err(|err| classify_sqlite_error(&err))?;

    let mut folders: HashMap<i64, (i64, String)> = HashMap::new();
    while let Some(row) = folder_rows
        .next()
        .map_err(|err| classify_sqlite_error(&err))?
    {
        let Some(id) = row_i64(row, 0) else {
            continue;
        };
        let parent = row_i64(row, 1).unwrap_or(0);
        let raw_title = row_opt_string(row, 2).unwrap_or_default();
        let title = sanitize_text(&raw_title, MAX_FOLDER_CHARS);
        folders.insert(id, (parent, title));
    }

    let mut stmt = tx
        .prepare(
            "SELECT b.id, p.url, COALESCE(b.title, p.title, ''), COALESCE(b.parent, 0), b.dateAdded \
             FROM moz_bookmarks AS b \
             JOIN moz_places AS p ON p.id = b.fk \
             WHERE b.type = 1 AND TYPEOF(b.id) = 'integer' AND b.id > ?1 \
             ORDER BY b.id ASC \
             LIMIT ?2",
        )
        .map_err(|err| classify_sqlite_error(&err))?;

    let mut bookmarks = Vec::new();
    let mut last_id = 0i64;
    let mut scanned = 0usize;

    while bookmarks.len() < MAX_BOOKMARKS && scanned < MAX_BOOKMARK_SCAN_ROWS {
        let prev_last_id = last_id;
        let remaining = (MAX_BOOKMARKS - bookmarks.len()) as i64;
        let limit = remaining.min(HISTORY_BATCH_SIZE);
        let mut rows = stmt
            .query(params![last_id, limit])
            .map_err(|err| classify_sqlite_error(&err))?;

        let mut fetched = 0i64;
        while let Some(row) = rows.next().map_err(|err| classify_sqlite_error(&err))? {
            fetched += 1;
            scanned += 1;
            let Some(id) = row_i64(row, 0) else {
                continue;
            };
            last_id = id;

            let Some(raw_url) = row_opt_string(row, 1) else {
                continue;
            };
            let Some(valid_url) = crate::index::validate_url(&raw_url) else {
                continue;
            };
            let parent_id = row_i64(row, 3).unwrap_or(0);
            let Some(folder_path) = resolve_gecko_folder_path(parent_id, &folders) else {
                continue;
            };

            let raw_title = row_opt_string(row, 2).unwrap_or_default();
            let mut title = sanitize_text(&raw_title, MAX_TITLE_CHARS);
            if title.is_empty() {
                title = sanitize_text(valid_url.as_str(), MAX_TITLE_CHARS);
            }
            let last_visited_ms = row_i64(row, 4).and_then(|us| prtime_to_unix_ms(us, now_ms));

            bookmarks.push(Entry {
                source_id: format!("b:{id}"),
                bookmark: true,
                url: valid_url.to_string(),
                title,
                visited_count: 0,
                last_visited_ms,
                folder_path,
            });
        }
        if fetched < limit || last_id == prev_last_id {
            break;
        }
    }

    Ok(bookmarks)
}

fn resolve_gecko_folder_path(
    mut current_id: i64,
    folders: &HashMap<i64, (i64, String)>,
) -> Option<Vec<String>> {
    let mut path = Vec::new();
    let mut visited = Vec::new();

    while current_id > 0 {
        if visited.len() >= MAX_FOLDER_DEPTH || visited.contains(&current_id) {
            return None;
        }
        visited.push(current_id);
        let Some((parent_id, title)) = folders.get(&current_id) else {
            break;
        };
        if !title.is_empty() {
            path.push(title.clone());
        }
        current_id = *parent_id;
    }

    path.reverse();
    Some(path)
}
