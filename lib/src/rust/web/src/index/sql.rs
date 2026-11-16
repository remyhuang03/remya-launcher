use std::collections::BTreeMap;

use super::dto::{EntryKind, IndexedRow};
use super::engines::is_valid_domain_host;

const MAX_FTS_TOKENS: usize = 8;

pub(super) fn extract_fts_trigram_tokens(query: &str) -> Vec<&str> {
    query
        .split_whitespace()
        .filter(|tok| tok.chars().count() >= 3)
        .take(MAX_FTS_TOKENS)
        .collect()
}

pub(super) fn build_fts5_query(tokens: &[&str]) -> String {
    let mut parts = Vec::with_capacity(tokens.len());
    for tok in tokens {
        let mut quoted = String::with_capacity(tok.len() + 2);
        quoted.push('"');
        for ch in tok.chars() {
            if ch == '"' {
                quoted.push('"');
            }
            quoted.push(ch);
        }
        quoted.push('"');
        parts.push(quoted);
    }
    parts.join(" ")
}

pub(super) fn escape_like_fragment(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub(super) fn domains_sql_filter(
    domains: &[&str],
    start_param_idx: usize,
) -> (String, Vec<String>) {
    if domains.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut clauses = Vec::with_capacity(domains.len());
    let mut params = Vec::with_capacity(domains.len() * 2);
    let mut idx = start_param_idx;
    for &domain in domains {
        let sub_pattern = format!("%.{}", escape_like_fragment(domain));
        clauses.push(format!(
            "AND (e.host = ?{idx} OR e.host LIKE ?{} ESCAPE '\\')",
            idx + 1
        ));
        params.push(domain.to_owned());
        params.push(sub_pattern);
        idx += 2;
    }
    (clauses.join(" "), params)
}

pub(super) fn target_host_sql_filter(
    target_host: &str,
    start_param_idx: usize,
) -> (String, Vec<String>) {
    if is_valid_domain_host(target_host) {
        domains_sql_filter(&[target_host], start_param_idx)
    } else {
        (
            format!("AND e.host = ?{start_param_idx}"),
            vec![target_host.to_ascii_lowercase()],
        )
    }
}

pub(super) fn sql_placeholders(count: usize) -> String {
    (1..=count)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn query_indexed_rows<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
) -> rusqlite::Result<Vec<IndexedRow>> {
    let rows = stmt.query_map(params, |row| {
        let kind_str: String = row.get(4)?;
        let folder_json: String = row.get(9)?;
        let folder_path: Vec<String> = serde_json::from_str(&folder_json).unwrap_or_default();
        Ok(IndexedRow {
            entry_id: row.get(0)?,
            profile_id: row.get(1)?,
            browser_id: row.get(2)?,
            browser_label: row.get(3)?,
            kind: EntryKind::from_db(&kind_str).unwrap_or(EntryKind::History),
            normalized_url: row.get(5)?,
            host: row.get(6)?,
            origin: row.get(7)?,
            title: row.get(8)?,
            folder_path,
            visited_count: row.get(10)?,
            last_visited_ms: row.get(11)?,
        })
    })?;

    let mut out = Vec::new();
    for item in rows {
        out.push(item?);
    }
    Ok(out)
}

pub(super) fn collect_rows_with_rowid<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
    out: &mut BTreeMap<i64, IndexedRow>,
) -> rusqlite::Result<()> {
    let rows = stmt.query_map(params, |row| {
        let rowid: i64 = row.get(0)?;
        let kind_str: String = row.get(5)?;
        let folder_json: String = row.get(10)?;
        let folder_path: Vec<String> = serde_json::from_str(&folder_json).unwrap_or_default();
        Ok((
            rowid,
            IndexedRow {
                entry_id: row.get(1)?,
                profile_id: row.get(2)?,
                browser_id: row.get(3)?,
                browser_label: row.get(4)?,
                kind: EntryKind::from_db(&kind_str).unwrap_or(EntryKind::History),
                normalized_url: row.get(6)?,
                host: row.get(7)?,
                origin: row.get(8)?,
                title: row.get(9)?,
                folder_path,
                visited_count: row.get(11)?,
                last_visited_ms: row.get(12)?,
            },
        ))
    })?;

    for item in rows {
        let (rowid, entry) = item?;
        out.entry(rowid).or_insert(entry);
    }
    Ok(())
}
