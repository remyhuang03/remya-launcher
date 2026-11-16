use std::collections::{HashMap, HashSet};

use serde_json::Value;
use url::Url;

use crate::MAX_QUERY_BYTES;

use super::dto::{EntryKind, IndexedRow, InputDto, OpenStats, RowDto, SearchResponseDto, SyncDto};
use super::engines::{
    build_suggestions, host_matches_domain, host_matches_url_target, parse_input,
};
use super::ranking::{
    build_groups, compute_frequent_sites, compute_recent_items, compute_source_summary,
    dedup_and_rank_group, score_candidate,
};
use super::sql::{query_indexed_rows, sql_placeholders};
use super::util::{current_time_ms, truncate_utf8};
use super::{Index, OPEN_RANK_WINDOW_MS, Search};

const SQL_EMPTY_QUERY_CANDIDATE_LIMIT: usize = 2000;
const DEFAULT_SEARCH_LIMIT: usize = 20;
const MAX_SEARCH_LIMIT: usize = 100;

impl Index {
    /// Queries the local projection index and formats the deterministic search DTO.
    pub fn search(&self, request: Search<'_>, stale: bool) -> rusqlite::Result<Value> {
        self.search_at(request, stale, current_time_ms())
    }

    pub(super) fn search_at(
        &self,
        request: Search<'_>,
        stale: bool,
        now_ms: i64,
    ) -> rusqlite::Result<Value> {
        let enabled_profiles = self.enabled_ids()?;
        let allowed_profiles = resolve_allowed_profiles(&enabled_profiles, request.profile_ids);
        let effective_limit = if request.limit == 0 {
            DEFAULT_SEARCH_LIMIT
        } else {
            request.limit.min(MAX_SEARCH_LIMIT)
        };

        let bounded_raw = truncate_utf8(request.query, MAX_QUERY_BYTES);
        let parsed_input = parse_input(bounded_raw, request.engine_id);

        if parsed_input.is_empty_non_engine() {
            return self.empty_query_response(
                &allowed_profiles,
                effective_limit,
                stale,
                now_ms,
                parsed_input.to_dto(None),
            );
        }

        let mut domain_scopes: Vec<&str> = Vec::with_capacity(2);
        if let Some(engine) = parsed_input.active_engine {
            domain_scopes.push(engine.domain_scope);
        }
        if let Some(site) = parsed_input.site_domain.as_deref()
            && !domain_scopes.contains(&site)
        {
            domain_scopes.push(site);
        }
        let search_text = parsed_input.search_text.as_str();
        let site_domain = parsed_input.site_domain.as_deref();
        let default_engine_id = self.default_engine_id()?;

        if allowed_profiles.is_empty() {
            let suggestions = build_suggestions(
                parsed_input.active_engine,
                site_domain,
                search_text,
                &default_engine_id,
            );
            let dto = SearchResponseDto {
                matched_count: 0,
                source_summary: Vec::new(),
                groups: build_groups(Vec::new(), Vec::new(), suggestions),
                frequent_sites: Vec::new(),
                recent: Vec::new(),
                input: parsed_input.to_dto(None),
                sync: SyncDto {
                    stale,
                    enabled_profile_count: 0,
                },
            };
            return Ok(serde_json::to_value(dto).unwrap_or(Value::Null));
        }

        let target_host = parsed_input
            .direct_target
            .as_deref()
            .and_then(|target| Url::parse(target).ok())
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase));

        let candidates = self.prune_search_candidates(
            &allowed_profiles,
            search_text,
            &domain_scopes,
            request.profile_ids.is_none() && target_host.is_none(),
        )?;
        let open_stats = self.load_open_stats(&allowed_profiles, now_ms)?;

        let mut scored_bookmarks = Vec::new();
        let mut scored_history = Vec::new();

        for row in candidates {
            if !domain_scopes
                .iter()
                .all(|domain| host_matches_domain(&row.host, domain))
            {
                continue;
            }
            if let Some(ref thost) = target_host
                && !host_matches_url_target(&row.host, thost)
            {
                continue;
            }
            let stats = open_stats.get(&row.normalized_url);
            let Some(scored) = score_candidate(row, search_text, stats, now_ms) else {
                continue;
            };
            match scored.row.kind {
                EntryKind::Bookmark => scored_bookmarks.push(scored),
                EntryKind::History => scored_history.push(scored),
            }
        }

        let all_bookmarks = dedup_and_rank_group(scored_bookmarks);
        let mut all_history = dedup_and_rank_group(scored_history);

        if let Some(ref thost) = target_host {
            let bookmark_take = all_bookmarks.len().min(effective_limit);
            let used = bookmark_take + all_history.len();
            if used < effective_limit {
                let need = effective_limit - used;
                let existing_urls: HashSet<String> = all_history
                    .iter()
                    .map(|item| item.row.url.clone())
                    .collect();
                let extra = self.fill_same_host_history(
                    &allowed_profiles,
                    thost,
                    &existing_urls,
                    &open_stats,
                    need,
                )?;
                all_history.extend(extra);
            }
        }

        let matched_count = all_bookmarks.len() + all_history.len();
        let source_summary = compute_source_summary(&all_bookmarks, &all_history);

        let completion = match (
            parsed_input.direct_target.as_deref(),
            target_host.as_deref(),
        ) {
            (Some(target), Some(thost)) => {
                self.find_best_completion(&allowed_profiles, target, thost, &open_stats, now_ms)?
            }
            _ => None,
        };

        let bookmark_take = all_bookmarks.len().min(effective_limit);
        let remaining = effective_limit.saturating_sub(bookmark_take);
        let history_take = all_history.len().min(remaining);

        let bookmarks_limited: Vec<RowDto> = all_bookmarks
            .into_iter()
            .take(bookmark_take)
            .map(|item| item.row)
            .collect();
        let history_limited: Vec<RowDto> = all_history
            .into_iter()
            .take(history_take)
            .map(|item| item.row)
            .collect();

        let suggestions = build_suggestions(
            parsed_input.active_engine,
            site_domain,
            search_text,
            &default_engine_id,
        );
        let dto = SearchResponseDto {
            matched_count,
            source_summary,
            groups: build_groups(bookmarks_limited, history_limited, suggestions),
            frequent_sites: Vec::new(),
            recent: Vec::new(),
            input: parsed_input.to_dto(completion),
            sync: SyncDto {
                stale,
                enabled_profile_count: allowed_profiles.len(),
            },
        };
        Ok(serde_json::to_value(dto).unwrap_or(Value::Null))
    }

    fn empty_query_response(
        &self,
        allowed_profiles: &[String],
        effective_limit: usize,
        stale: bool,
        now_ms: i64,
        input: InputDto,
    ) -> rusqlite::Result<Value> {
        if allowed_profiles.is_empty() {
            let dto = SearchResponseDto {
                matched_count: 0,
                source_summary: Vec::new(),
                groups: build_groups(Vec::new(), Vec::new(), Vec::new()),
                frequent_sites: Vec::new(),
                recent: Vec::new(),
                input,
                sync: SyncDto {
                    stale,
                    enabled_profile_count: 0,
                },
            };
            return Ok(serde_json::to_value(dto).unwrap_or(Value::Null));
        }

        let pinned_rows = self.load_pinned_rows(allowed_profiles)?;
        let rows = self.load_empty_query_candidates(allowed_profiles)?;
        let open_stats = self.load_open_stats(allowed_profiles, now_ms)?;

        let frequent_sites = compute_frequent_sites(&rows, &pinned_rows, &open_stats, now_ms);
        let recent = compute_recent_items(&rows, &open_stats, effective_limit);

        let dto = SearchResponseDto {
            matched_count: 0,
            source_summary: Vec::new(),
            groups: build_groups(Vec::new(), Vec::new(), Vec::new()),
            frequent_sites,
            recent,
            input,
            sync: SyncDto {
                stale,
                enabled_profile_count: allowed_profiles.len(),
            },
        };
        Ok(serde_json::to_value(dto).unwrap_or(Value::Null))
    }

    fn load_pinned_rows(&self, allowed_profiles: &[String]) -> rusqlite::Result<Vec<IndexedRow>> {
        let placeholders = sql_placeholders(allowed_profiles.len());
        let sql = format!(
            "SELECT e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM pins p
             JOIN entries e ON e.entry_id = p.entry_id
             JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE e.profile_id IN ({placeholders})
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 WHERE h.profile_id = e.profile_id
                   AND ((h.scope = 'url' AND h.target = e.normalized_url)
                     OR (h.scope = 'host' AND h.target = e.host))
               )
             ORDER BY p.rowid ASC",
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        query_indexed_rows(&mut stmt, rusqlite::params_from_iter(bound))
    }

    fn load_empty_query_candidates(
        &self,
        allowed_profiles: &[String],
    ) -> rusqlite::Result<Vec<IndexedRow>> {
        let placeholders = sql_placeholders(allowed_profiles.len());
        let sql = format!(
            "SELECT e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM entries e INDEXED BY idx_entries_global_recent
             CROSS JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE e.profile_id IN ({placeholders})
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 WHERE h.profile_id = e.profile_id
                   AND ((h.scope = 'url' AND h.target = e.normalized_url)
                     OR (h.scope = 'host' AND h.target = e.host))
               )
             ORDER BY COALESCE(e.last_visited_ms, 0) DESC,
                      e.visited_count DESC,
                      e.kind ASC,
                      e.entry_id ASC
             LIMIT ?{}",
            allowed_profiles.len() + 1
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        bound.push(rusqlite::types::Value::Integer(
            SQL_EMPTY_QUERY_CANDIDATE_LIMIT as i64,
        ));
        query_indexed_rows(&mut stmt, rusqlite::params_from_iter(bound))
    }

    fn load_open_stats(
        &self,
        allowed_profiles: &[String],
        now_ms: i64,
    ) -> rusqlite::Result<HashMap<String, OpenStats>> {
        let placeholders = sql_placeholders(allowed_profiles.len());
        let window_start = now_ms.saturating_sub(OPEN_RANK_WINDOW_MS);
        let p_base = allowed_profiles.len();
        let sql = format!(
            "SELECT l.normalized_url, COUNT(*), MAX(l.opened_ms)
             FROM launcher_opens l
             WHERE (l.profile_id IS NULL OR l.profile_id IN ({placeholders}))
               AND l.opened_ms >= ?{win}
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 JOIN opt_ins o ON o.profile_id = h.profile_id AND o.enabled = 1
                 WHERE h.profile_id IN ({placeholders})
                   AND ((h.scope = 'url' AND h.target = l.normalized_url)
                     OR (h.scope = 'host' AND h.target = l.host))
               )
             GROUP BY l.normalized_url",
            win = p_base + 1,
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        bound.push(rusqlite::types::Value::Integer(window_start));
        let rows = stmt.query_map(rusqlite::params_from_iter(bound), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?;
        let mut map = HashMap::new();
        for item in rows {
            let (url, count, last_ms) = item?;
            map.insert(
                url,
                OpenStats {
                    recent_count: count,
                    last_opened_ms: last_ms,
                },
            );
        }
        Ok(map)
    }
}

fn resolve_allowed_profiles(
    enabled_ids: &HashSet<String>,
    requested: Option<&[String]>,
) -> Vec<String> {
    let mut out: Vec<String> = match requested {
        None => enabled_ids.iter().cloned().collect(),
        Some(filter) => filter
            .iter()
            .filter(|id| enabled_ids.contains(id.as_str()))
            .cloned()
            .collect(),
    };
    out.sort();
    out.dedup();
    out
}
