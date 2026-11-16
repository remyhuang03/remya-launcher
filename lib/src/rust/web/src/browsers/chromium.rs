use std::path::Path;

use serde_json::Value;

use super::sqlite::{
    HistoryQuerySpec, chromium_us_to_unix_ms, read_history_table, unix_ms_to_chromium_us,
    with_readonly_sqlite,
};
use super::{
    Entry, MAX_BOOKMARK_SCAN_NODES, MAX_BOOKMARKS, MAX_BOOKMARKS_BYTES, MAX_FOLDER_CHARS,
    MAX_FOLDER_DEPTH, MAX_TITLE_CHARS, NINETY_DAYS_MS, path_present, read_bounded_file,
    sanitize_text,
};

pub(super) fn read_chromium_profile(dir: &Path, now_ms: i64) -> Result<Vec<Entry>, &'static str> {
    let history_path = dir.join("History");
    let bookmarks_path = dir.join("Bookmarks");

    let has_history = path_present(&history_path)?;
    let has_bookmarks = path_present(&bookmarks_path)?;
    if !has_history && !has_bookmarks {
        return Err("SOURCE_UNAVAILABLE");
    }

    let mut entries = if has_history {
        read_chromium_history(&history_path, now_ms)?
    } else {
        Vec::new()
    };

    if has_bookmarks {
        match read_chromium_bookmarks(&bookmarks_path, now_ms) {
            Ok(bookmarks) => entries.extend(bookmarks),
            Err(err) => {
                // Bookmark parse failure must not discard valid History entries.
                if !has_history {
                    return Err(err);
                }
            }
        }
    }

    Ok(entries)
}

fn read_chromium_history(path: &Path, now_ms: i64) -> Result<Vec<Entry>, &'static str> {
    let cutoff_ms = now_ms.saturating_sub(NINETY_DAYS_MS);
    with_readonly_sqlite(path, |tx| {
        read_history_table(
            tx,
            HistoryQuerySpec {
                table: "urls",
                ts_col: "last_visit_time",
                cutoff_raw: unix_ms_to_chromium_us(cutoff_ms),
                cutoff_ms,
                convert_ts: chromium_us_to_unix_ms,
            },
            now_ms,
        )
    })
}

fn read_chromium_bookmarks(path: &Path, now_ms: i64) -> Result<Vec<Entry>, &'static str> {
    let bytes = read_bounded_file(path, MAX_BOOKMARKS_BYTES)?;
    let root_doc: Value = serde_json::from_slice(&bytes).map_err(|_| "UNSUPPORTED_SCHEMA")?;
    let roots = root_doc
        .get("roots")
        .and_then(Value::as_object)
        .ok_or("UNSUPPORTED_SCHEMA")?;

    let mut entries = Vec::new();
    let mut seq = 0usize;
    let mut visited_nodes = 0usize;

    // Iterative explicit stack to bound traversal depth and avoid call-stack recursion.
    let mut stack: Vec<(&Value, Vec<String>)> = Vec::new();
    let mut root_keys: Vec<&String> = roots.keys().collect();
    root_keys.sort();
    for key in root_keys.into_iter().rev() {
        if let Some(node) = roots.get(key) {
            stack.push((node, Vec::new()));
        }
    }

    while let Some((node, folder_path)) = stack.pop() {
        if entries.len() >= MAX_BOOKMARKS || visited_nodes >= MAX_BOOKMARK_SCAN_NODES {
            break;
        }
        visited_nodes += 1;
        let Some(obj) = node.as_object() else {
            continue;
        };
        let node_type = obj.get("type").and_then(Value::as_str).unwrap_or("");
        if node_type == "url" || obj.get("url").is_some() {
            let Some(raw_url) = obj.get("url").and_then(Value::as_str) else {
                continue;
            };
            let Some(valid_url) = crate::index::validate_url(raw_url) else {
                continue;
            };
            seq += 1;
            let raw_id = obj
                .get("guid")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    obj.get("id")
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                });
            let source_id = match raw_id {
                Some(id) => format!("b:{id}"),
                None => format!("b:seq-{seq}"),
            };

            let raw_title = obj.get("name").and_then(Value::as_str).unwrap_or("");
            let mut title = sanitize_text(raw_title, MAX_TITLE_CHARS);
            if title.is_empty() {
                title = sanitize_text(valid_url.as_str(), MAX_TITLE_CHARS);
            }
            let last_visited_ms = parse_chromium_json_timestamp(obj.get("date_added"))
                .and_then(|us| chromium_us_to_unix_ms(us, now_ms));

            entries.push(Entry {
                source_id,
                bookmark: true,
                url: valid_url.to_string(),
                title,
                visited_count: 0,
                last_visited_ms,
                folder_path,
            });
            continue;
        }

        if (node_type == "folder" || obj.get("children").is_some())
            && folder_path.len() < MAX_FOLDER_DEPTH
            && let Some(children) = obj.get("children").and_then(Value::as_array)
        {
            let mut next_path = folder_path;
            if let Some(folder_name) = obj.get("name").and_then(Value::as_str) {
                let cleaned = sanitize_text(folder_name, MAX_FOLDER_CHARS);
                if !cleaned.is_empty() {
                    next_path.push(cleaned);
                }
            }
            for child in children.iter().rev() {
                if stack.len() + visited_nodes >= MAX_BOOKMARK_SCAN_NODES {
                    break;
                }
                stack.push((child, next_path.clone()));
            }
        }
    }

    Ok(entries)
}

fn parse_chromium_json_timestamp(val: Option<&Value>) -> Option<i64> {
    match val? {
        Value::String(s) => s.trim().parse::<i64>().ok(),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}
