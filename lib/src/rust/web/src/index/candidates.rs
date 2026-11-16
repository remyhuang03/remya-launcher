use std::collections::{BTreeMap, HashMap, HashSet};

use super::Index;
use super::dto::{CompletionDto, IndexedRow, MatchTier, OpenStats, RankedGroupEntry, ScoredRow};
use super::engines::host_matches_url_target;
use super::ranking::{compute_frecency_score, dedup_and_rank_group};
use super::sql::{
    build_fts5_query, collect_rows_with_rowid, domains_sql_filter, escape_like_fragment,
    extract_fts_trigram_tokens, query_indexed_rows, sql_placeholders, target_host_sql_filter,
};
use super::util::{max_opt_i64, title_case_first};

const SQL_PREFIX_CANDIDATE_LIMIT: usize = 200;
const SQL_FTS_CANDIDATE_LIMIT: usize = 1500;
const SQL_FTS_PROBE_LIMIT: usize = 4000;
const SQL_FTS_TAIL_LIMIT: usize = 3000;
const SQL_SMALL_QUERY_LIMIT: usize = 1500;
const SQL_COMPLETION_LIMIT: usize = 64;

impl Index {
    pub(super) fn prune_search_candidates(
        &self,
        allowed_profiles: &[String],
        search_text: &str,
        domain_scopes: &[&str],
        allow_broad_sampling: bool,
    ) -> rusqlite::Result<Vec<IndexedRow>> {
        let trimmed = search_text.trim();
        let mut by_rowid: BTreeMap<i64, IndexedRow> = BTreeMap::new();

        if trimmed.is_empty() {
            if !domain_scopes.is_empty() {
                self.collect_domain_candidates(allowed_profiles, domain_scopes, &mut by_rowid)?;
            }
            return Ok(by_rowid.into_values().collect());
        }

        let fts_tokens = extract_fts_trigram_tokens(trimmed);
        if !fts_tokens.is_empty() {
            self.collect_prefix_and_exact_candidates(
                allowed_profiles,
                trimmed,
                domain_scopes,
                &mut by_rowid,
            )?;
            let fts_expr = build_fts5_query(&fts_tokens);
            self.collect_fts_candidates(
                allowed_profiles,
                &fts_expr,
                allow_broad_sampling.then_some(fts_tokens[0]),
                domain_scopes,
                &mut by_rowid,
            )?;
            return Ok(by_rowid.into_values().collect());
        }

        self.collect_small_query_candidates(
            allowed_profiles,
            trimmed,
            domain_scopes,
            &mut by_rowid,
        )?;
        Ok(by_rowid.into_values().collect())
    }

    fn collect_domain_candidates(
        &self,
        allowed_profiles: &[String],
        domain_scopes: &[&str],
        out: &mut BTreeMap<i64, IndexedRow>,
    ) -> rusqlite::Result<()> {
        let placeholders = sql_placeholders(allowed_profiles.len());
        let p_base = allowed_profiles.len();
        let (domain_clause, domain_params) = domains_sql_filter(domain_scopes, p_base + 1);
        let lim_idx = p_base + 1 + domain_params.len();
        let sql = format!(
            "SELECT e.rowid, e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM entries e
             JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE e.profile_id IN ({placeholders})
               {domain_clause}
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 WHERE h.profile_id = e.profile_id
                   AND ((h.scope = 'url' AND h.target = e.normalized_url)
                     OR (h.scope = 'host' AND h.target = e.host))
               )
             ORDER BY (CASE WHEN e.kind = 'bookmark' THEN 1 ELSE 0 END) DESC,
                      COALESCE(e.last_visited_ms, 0) DESC,
                      e.visited_count DESC
             LIMIT ?{lim_idx}",
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        for p in domain_params {
            bound.push(rusqlite::types::Value::Text(p));
        }
        bound.push(rusqlite::types::Value::Integer(
            SQL_FTS_CANDIDATE_LIMIT as i64,
        ));
        collect_rows_with_rowid(&mut stmt, rusqlite::params_from_iter(bound), out)
    }

    pub(super) fn fill_same_host_history(
        &self,
        allowed_profiles: &[String],
        target_host: &str,
        existing_urls: &HashSet<String>,
        open_stats: &HashMap<String, OpenStats>,
        need: usize,
    ) -> rusqlite::Result<Vec<RankedGroupEntry>> {
        if need == 0 || allowed_profiles.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = sql_placeholders(allowed_profiles.len());
        let p_base = allowed_profiles.len();
        let (host_clause, host_params) = target_host_sql_filter(target_host, p_base + 1);
        let lim_idx = p_base + 1 + host_params.len();
        let sql = format!(
            "SELECT e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM entries e
             JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE e.profile_id IN ({placeholders})
               AND e.kind = 'history'
               {host_clause}
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 WHERE h.profile_id = e.profile_id
                   AND ((h.scope = 'url' AND h.target = e.normalized_url)
                     OR (h.scope = 'host' AND h.target = e.host))
               )
             ORDER BY COALESCE(e.last_visited_ms, 0) DESC,
                      e.visited_count DESC,
                      e.entry_id ASC
             LIMIT ?{lim_idx}",
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        for p in host_params {
            bound.push(rusqlite::types::Value::Text(p));
        }
        bound.push(rusqlite::types::Value::Integer(
            SQL_FTS_CANDIDATE_LIMIT as i64,
        ));
        let rows = query_indexed_rows(&mut stmt, rusqlite::params_from_iter(bound))?;

        let mut scored = Vec::new();
        for row in rows {
            if existing_urls.contains(&row.normalized_url)
                || !host_matches_url_target(&row.host, target_host)
            {
                continue;
            }
            let stats = open_stats.get(&row.normalized_url);
            let effective_last_ms =
                max_opt_i64(row.last_visited_ms, stats.and_then(|s| s.last_opened_ms));
            scored.push(ScoredRow {
                row,
                tier: MatchTier::Substring,
                score: 0.0,
                effective_last_ms,
            });
        }

        let mut grouped = dedup_and_rank_group(scored);
        grouped.sort_by(|a, b| {
            b.effective_last_ms
                .cmp(&a.effective_last_ms)
                .then_with(|| b.row.visited_count.cmp(&a.row.visited_count))
                .then_with(|| a.row.url.cmp(&b.row.url))
                .then_with(|| a.row.entry_id.cmp(&b.row.entry_id))
        });
        grouped.truncate(need);
        Ok(grouped)
    }

    fn collect_prefix_and_exact_candidates(
        &self,
        allowed_profiles: &[String],
        trimmed: &str,
        domain_scopes: &[&str],
        out: &mut BTreeMap<i64, IndexedRow>,
    ) -> rusqlite::Result<()> {
        let placeholders = sql_placeholders(allowed_profiles.len());
        let q_lower = trimmed.to_lowercase();
        let like_prefix = format!("{}%", escape_like_fragment(&q_lower));
        let url_https_prefix = format!("https://{}%", escape_like_fragment(&q_lower));
        let url_http_prefix = format!("http://{}%", escape_like_fragment(&q_lower));

        let p_base = allowed_profiles.len();
        let (domain_clause, domain_params) = domains_sql_filter(domain_scopes, p_base + 4);
        let lim_idx = p_base + 4 + domain_params.len();

        let sql = format!(
            "WITH prefix(rowid) AS (
               SELECT rowid FROM entries
               WHERE normalized_url LIKE ?{lp} ESCAPE '\\'
                  OR normalized_url LIKE ?{uhs} ESCAPE '\\'
                  OR normalized_url LIKE ?{uh} ESCAPE '\\'
                  OR host LIKE ?{lp} ESCAPE '\\'
                  OR title LIKE ?{lp} ESCAPE '\\'
             )
             SELECT e.rowid, e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM prefix
             CROSS JOIN entries e ON e.rowid = prefix.rowid
             JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE e.profile_id IN ({placeholders})
               {domain_clause}
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 WHERE h.profile_id = e.profile_id
                   AND ((h.scope = 'url' AND h.target = e.normalized_url)
                     OR (h.scope = 'host' AND h.target = e.host))
               )
             ORDER BY e.kind ASC,
                      COALESCE(e.last_visited_ms, 0) DESC,
                      e.visited_count DESC,
                      e.rowid ASC
             LIMIT ?{lim_idx}",
            lp = p_base + 1,
            uhs = p_base + 2,
            uh = p_base + 3,
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        bound.push(rusqlite::types::Value::Text(like_prefix));
        bound.push(rusqlite::types::Value::Text(url_https_prefix));
        bound.push(rusqlite::types::Value::Text(url_http_prefix));
        for p in domain_params {
            bound.push(rusqlite::types::Value::Text(p));
        }
        bound.push(rusqlite::types::Value::Integer(
            SQL_PREFIX_CANDIDATE_LIMIT as i64,
        ));
        collect_rows_with_rowid(&mut stmt, rusqlite::params_from_iter(bound), out)
    }

    fn collect_fts_candidates(
        &self,
        allowed_profiles: &[String],
        fts_expr: &str,
        sampling_token: Option<&str>,
        domain_scopes: &[&str],
        out: &mut BTreeMap<i64, IndexedRow>,
    ) -> rusqlite::Result<()> {
        if allowed_profiles.is_empty() {
            return Ok(());
        }
        let placeholders = sql_placeholders(allowed_profiles.len());
        let p_base = allowed_profiles.len();

        let Some(first_tok) = sampling_token.filter(|_| domain_scopes.is_empty()) else {
            let (domain_clause, domain_params) = domains_sql_filter(domain_scopes, p_base + 2);
            let lim_idx = p_base + 2 + domain_params.len();
            let sql = format!(
                "WITH ranked(rowid) AS MATERIALIZED (
                   SELECT e.rowid
                   FROM entries_fts
                   JOIN entries e ON e.rowid = entries_fts.rowid
                   JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
                   WHERE entries_fts MATCH ?{fts}
                     AND e.profile_id IN ({placeholders})
                     {domain_clause}
                     AND NOT EXISTS (
                       SELECT 1 FROM hidden h
                       WHERE h.profile_id = e.profile_id
                         AND ((h.scope = 'url' AND h.target = e.normalized_url)
                           OR (h.scope = 'host' AND h.target = e.host))
                     )
                   ORDER BY e.kind ASC,
                            COALESCE(e.last_visited_ms, 0) DESC,
                            e.visited_count DESC,
                            e.rowid ASC
                   LIMIT ?{lim_idx}
                 )
                 SELECT e.rowid, e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                        e.kind, e.normalized_url, e.host, e.origin,
                        e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
                 FROM ranked
                 CROSS JOIN entries e ON e.rowid = ranked.rowid",
                fts = p_base + 1,
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
                .iter()
                .map(|id| rusqlite::types::Value::Text(id.clone()))
                .collect();
            bound.push(rusqlite::types::Value::Text(fts_expr.to_owned()));
            for p in domain_params {
                bound.push(rusqlite::types::Value::Text(p));
            }
            bound.push(rusqlite::types::Value::Integer(
                SQL_FTS_CANDIDATE_LIMIT as i64,
            ));
            return collect_rows_with_rowid(&mut stmt, rusqlite::params_from_iter(bound), out);
        };

        // shortcut: sample broad unscoped matches until the index can rank all hits within the query budget.
        let prof_count = allowed_profiles.len();
        let per_prof_recent = (3000 / prof_count).clamp(300, 1500) as i64;
        let per_prof_bkm = (1500 / prof_count).clamp(150, 1500) as i64;

        let fts_idx = p_base + 1;
        let probe_lim_idx = p_base + 2;
        let probe_cap_idx = p_base + 3;
        let tail_lim_idx = p_base + 4;
        let recent_lim_idx = p_base + 5;
        let bkm_lim_idx = p_base + 6;

        let per_prof_recent_sql = (1..=prof_count)
            .map(|i| {
                format!(
                    "SELECT rowid FROM (
                       SELECT e.rowid FROM entries e
                       WHERE (SELECT flag FROM is_broad) = 1 AND e.profile_id = ?{i}
                       ORDER BY e.last_visited_ms DESC
                       LIMIT ?{recent_lim_idx}
                     )"
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL ");

        let per_prof_bkm_sql = (1..=prof_count)
            .map(|i| {
                format!(
                    "SELECT rowid FROM (
                       SELECT e.rowid FROM entries e
                       WHERE (SELECT flag FROM is_broad) = 1
                         AND e.profile_id = ?{i}
                         AND e.kind = 'bookmark'
                       LIMIT ?{bkm_lim_idx}
                     )"
                )
            })
            .collect::<Vec<_>>()
            .join(" UNION ALL ");

        let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        bound.push(rusqlite::types::Value::Text(fts_expr.to_owned()));
        bound.push(rusqlite::types::Value::Integer(
            (SQL_FTS_PROBE_LIMIT + 1) as i64,
        ));
        bound.push(rusqlite::types::Value::Integer(SQL_FTS_PROBE_LIMIT as i64));
        bound.push(rusqlite::types::Value::Integer(SQL_FTS_TAIL_LIMIT as i64));
        bound.push(rusqlite::types::Value::Integer(per_prof_recent));
        bound.push(rusqlite::types::Value::Integer(per_prof_bkm));

        let (tok_filter, lim_idx) = if first_tok.is_ascii() {
            let pat_idx = p_base + 7;
            bound.push(rusqlite::types::Value::Text(format!(
                "%{}%",
                escape_like_fragment(&first_tok.to_ascii_lowercase())
            )));
            (
                format!(
                    "e.title LIKE ?{pat_idx} ESCAPE '\\'
                     OR e.normalized_url LIKE ?{pat_idx} ESCAPE '\\'
                     OR e.host LIKE ?{pat_idx} ESCAPE '\\'
                     OR e.folder_path_text LIKE ?{pat_idx} ESCAPE '\\'"
                ),
                p_base + 8,
            )
        } else {
            let p1 = p_base + 7;
            let p2 = p_base + 8;
            let p3 = p_base + 9;
            let p4 = p_base + 10;
            bound.push(rusqlite::types::Value::Text(first_tok.to_owned()));
            bound.push(rusqlite::types::Value::Text(first_tok.to_lowercase()));
            bound.push(rusqlite::types::Value::Text(first_tok.to_uppercase()));
            bound.push(rusqlite::types::Value::Text(title_case_first(first_tok)));
            (
                format!(
                    "INSTR(e.title, ?{p1}) > 0 OR INSTR(e.title, ?{p2}) > 0
                     OR INSTR(e.title, ?{p3}) > 0 OR INSTR(e.title, ?{p4}) > 0
                     OR INSTR(e.normalized_url, ?{p1}) > 0 OR INSTR(e.normalized_url, ?{p2}) > 0
                     OR INSTR(e.folder_path_text, ?{p1}) > 0 OR INSTR(e.folder_path_text, ?{p2}) > 0"
                ),
                p_base + 11,
            )
        };
        bound.push(rusqlite::types::Value::Integer(
            SQL_FTS_CANDIDATE_LIMIT as i64,
        ));

        let sql = format!(
            "WITH fts_probe(rowid) AS MATERIALIZED (
               SELECT rowid FROM entries_fts
               WHERE entries_fts MATCH ?{fts_idx}
               LIMIT ?{probe_lim_idx}
             ),
             is_broad(flag) AS MATERIALIZED (
               SELECT CASE WHEN count(*) > ?{probe_cap_idx} THEN 1 ELSE 0 END FROM fts_probe
             ),
             broad_extra(rowid) AS (
               SELECT rowid FROM (
                 SELECT rowid FROM entries_fts
                 WHERE (SELECT flag FROM is_broad) = 1 AND entries_fts MATCH ?{fts_idx}
                 ORDER BY rowid DESC
                 LIMIT ?{tail_lim_idx}
               )
               UNION ALL
               SELECT e.rowid FROM (
                 {per_prof_recent_sql}
                 UNION ALL
                 {per_prof_bkm_sql}
               ) w
               JOIN entries e ON e.rowid = w.rowid
               WHERE {tok_filter}
             ),
             cand(rowid) AS (
               SELECT rowid FROM fts_probe
               UNION
               SELECT rowid FROM broad_extra
             ),
             ranked(rowid) AS MATERIALIZED (
               SELECT e.rowid
               FROM cand
               CROSS JOIN entries e ON e.rowid = cand.rowid
               JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
               WHERE e.profile_id IN ({placeholders})
                 AND NOT EXISTS (
                   SELECT 1 FROM hidden h
                   WHERE h.profile_id = e.profile_id
                     AND ((h.scope = 'url' AND h.target = e.normalized_url)
                       OR (h.scope = 'host' AND h.target = e.host))
                 )
               ORDER BY e.kind ASC,
                        COALESCE(e.last_visited_ms, 0) DESC,
                        e.visited_count DESC,
                        e.rowid ASC
               LIMIT ?{lim_idx}
             )
             SELECT e.rowid, e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM ranked
             CROSS JOIN entries e ON e.rowid = ranked.rowid"
        );

        let mut stmt = self.conn.prepare(&sql)?;
        collect_rows_with_rowid(&mut stmt, rusqlite::params_from_iter(bound), out)
    }

    fn collect_small_query_candidates(
        &self,
        allowed_profiles: &[String],
        trimmed: &str,
        domain_scopes: &[&str],
        out: &mut BTreeMap<i64, IndexedRow>,
    ) -> rusqlite::Result<()> {
        let placeholders = sql_placeholders(allowed_profiles.len());
        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        let first_tok = tokens.first().copied().unwrap_or(trimmed);
        let all_ascii = trimmed.is_ascii();

        let p_base = allowed_profiles.len();
        if all_ascii {
            let pattern = format!(
                "%{}%",
                escape_like_fragment(&first_tok.to_ascii_lowercase())
            );
            let (domain_clause, domain_params) = domains_sql_filter(domain_scopes, p_base + 2);
            let lim_idx = p_base + 2 + domain_params.len();
            let sql = format!(
                "SELECT e.rowid, e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                        e.kind, e.normalized_url, e.host, e.origin,
                        e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
                 FROM entries e
                 JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
                 WHERE e.profile_id IN ({placeholders})
                   AND (
                     e.title LIKE ?{pat} ESCAPE '\\'
                     OR e.normalized_url LIKE ?{pat} ESCAPE '\\'
                     OR e.host LIKE ?{pat} ESCAPE '\\'
                     OR e.folder_path_text LIKE ?{pat} ESCAPE '\\'
                   )
                   {domain_clause}
                   AND NOT EXISTS (
                     SELECT 1 FROM hidden h
                     WHERE h.profile_id = e.profile_id
                       AND ((h.scope = 'url' AND h.target = e.normalized_url)
                         OR (h.scope = 'host' AND h.target = e.host))
                   )
                 ORDER BY (CASE WHEN e.kind = 'bookmark' THEN 1 ELSE 0 END) DESC,
                          COALESCE(e.last_visited_ms, 0) DESC,
                          e.visited_count DESC
                 LIMIT ?{lim_idx}",
                pat = p_base + 1,
            );
            let mut stmt = self.conn.prepare(&sql)?;
            let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
                .iter()
                .map(|id| rusqlite::types::Value::Text(id.clone()))
                .collect();
            bound.push(rusqlite::types::Value::Text(pattern));
            for p in domain_params {
                bound.push(rusqlite::types::Value::Text(p));
            }
            bound.push(rusqlite::types::Value::Integer(
                SQL_SMALL_QUERY_LIMIT as i64,
            ));
            return collect_rows_with_rowid(&mut stmt, rusqlite::params_from_iter(bound), out);
        }

        // Unicode small query (<3 chars per token with non-ASCII):
        // SQLite LIKE/INSTR is case-sensitive on non-ASCII, so retrieve exact/cased INSTR hits
        // plus a bounded recent+bookmark window and perform full Unicode case-folding in Rust.
        let tok_lower = first_tok.to_lowercase();
        let tok_upper = first_tok.to_uppercase();
        let tok_title = title_case_first(first_tok);

        let (domain_clause, domain_params) = domains_sql_filter(domain_scopes, p_base + 5);
        let lim_idx = p_base + 5 + domain_params.len();
        let sql_exact = format!(
            "SELECT e.rowid, e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM entries e
             JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE e.profile_id IN ({placeholders})
               AND (
                 INSTR(e.title, ?{p1}) > 0 OR INSTR(e.title, ?{p2}) > 0
                 OR INSTR(e.title, ?{p3}) > 0 OR INSTR(e.title, ?{p4}) > 0
                 OR INSTR(e.normalized_url, ?{p1}) > 0 OR INSTR(e.normalized_url, ?{p2}) > 0
                 OR INSTR(e.folder_path_text, ?{p1}) > 0 OR INSTR(e.folder_path_text, ?{p2}) > 0
               )
               {domain_clause}
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 WHERE h.profile_id = e.profile_id
                   AND ((h.scope = 'url' AND h.target = e.normalized_url)
                     OR (h.scope = 'host' AND h.target = e.host))
               )
             ORDER BY (CASE WHEN e.kind = 'bookmark' THEN 1 ELSE 0 END) DESC,
                      COALESCE(e.last_visited_ms, 0) DESC,
                      e.visited_count DESC
             LIMIT ?{lim_idx}",
            p1 = p_base + 1,
            p2 = p_base + 2,
            p3 = p_base + 3,
            p4 = p_base + 4,
        );
        let mut stmt = self.conn.prepare(&sql_exact)?;
        let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        bound.push(rusqlite::types::Value::Text(first_tok.to_owned()));
        bound.push(rusqlite::types::Value::Text(tok_lower));
        bound.push(rusqlite::types::Value::Text(tok_upper));
        bound.push(rusqlite::types::Value::Text(tok_title));
        for p in domain_params {
            bound.push(rusqlite::types::Value::Text(p));
        }
        bound.push(rusqlite::types::Value::Integer(
            SQL_SMALL_QUERY_LIMIT as i64,
        ));
        collect_rows_with_rowid(&mut stmt, rusqlite::params_from_iter(bound), out)?;

        // Bounded recent/bookmark fallback scan for mixed-case non-ASCII words.
        let (domain_clause2, domain_params2) = domains_sql_filter(domain_scopes, p_base + 1);
        let lim_idx2 = p_base + 1 + domain_params2.len();
        let sql_recent = format!(
            "SELECT e.rowid, e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM entries e
             JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE e.profile_id IN ({placeholders})
               {domain_clause2}
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 WHERE h.profile_id = e.profile_id
                   AND ((h.scope = 'url' AND h.target = e.normalized_url)
                     OR (h.scope = 'host' AND h.target = e.host))
               )
             ORDER BY (CASE WHEN e.kind = 'bookmark' THEN 1 ELSE 0 END) DESC,
                      COALESCE(e.last_visited_ms, 0) DESC,
                      e.visited_count DESC
             LIMIT ?{lim_idx2}"
        );
        let mut stmt2 = self.conn.prepare(&sql_recent)?;
        let mut bound2: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        for p in domain_params2 {
            bound2.push(rusqlite::types::Value::Text(p));
        }
        bound2.push(rusqlite::types::Value::Integer(
            SQL_SMALL_QUERY_LIMIT as i64,
        ));
        collect_rows_with_rowid(&mut stmt2, rusqlite::params_from_iter(bound2), out)
    }

    pub(super) fn find_best_completion(
        &self,
        allowed_profiles: &[String],
        direct_target: &str,
        target_host: &str,
        open_stats: &HashMap<String, OpenStats>,
        now_ms: i64,
    ) -> rusqlite::Result<Option<CompletionDto>> {
        let placeholders = sql_placeholders(allowed_profiles.len());
        let https_prefix = format!("{}%", escape_like_fragment(direct_target));
        let alt_prefix = if let Some(rest) = direct_target.strip_prefix("https://") {
            format!("http://{}%", escape_like_fragment(rest))
        } else {
            https_prefix.clone()
        };

        let p_base = allowed_profiles.len();
        let (host_clause, host_params) = target_host_sql_filter(target_host, p_base + 4);
        let lim_idx = p_base + 4 + host_params.len();
        let sql = format!(
            "WITH url_prefix(rowid) AS (
               SELECT rowid FROM entries
               WHERE normalized_url LIKE ?{p1} ESCAPE '\\'
                  OR normalized_url LIKE ?{p2} ESCAPE '\\'
             )
             SELECT e.entry_id, e.profile_id, e.browser_id, e.browser_label,
                    e.kind, e.normalized_url, e.host, e.origin,
                    e.title, e.folder_path_json, e.visited_count, e.last_visited_ms
             FROM url_prefix
             CROSS JOIN entries e ON e.rowid = url_prefix.rowid
             JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE e.profile_id IN ({placeholders})
               AND e.normalized_url <> ?{p3}
               {host_clause}
               AND NOT EXISTS (
                 SELECT 1 FROM hidden h
                 WHERE h.profile_id = e.profile_id
                   AND ((h.scope = 'url' AND h.target = e.normalized_url)
                     OR (h.scope = 'host' AND h.target = e.host))
               )
             LIMIT ?{lim_idx}",
            p1 = p_base + 1,
            p2 = p_base + 2,
            p3 = p_base + 3,
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let mut bound: Vec<rusqlite::types::Value> = allowed_profiles
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone()))
            .collect();
        bound.push(rusqlite::types::Value::Text(https_prefix));
        bound.push(rusqlite::types::Value::Text(alt_prefix));
        bound.push(rusqlite::types::Value::Text(direct_target.to_owned()));
        for p in host_params {
            bound.push(rusqlite::types::Value::Text(p));
        }
        bound.push(rusqlite::types::Value::Integer(SQL_COMPLETION_LIMIT as i64));

        let rows = query_indexed_rows(&mut stmt, rusqlite::params_from_iter(bound))?;
        let mut best: Option<(f64, i64, String, String)> = None;

        for row in rows {
            if row.normalized_url.len() <= direct_target.len()
                || !host_matches_url_target(&row.host, target_host)
            {
                continue;
            }
            let stats = open_stats.get(&row.normalized_url);
            let frecency = compute_frecency_score(&row, stats, now_ms);
            let effective_last =
                max_opt_i64(row.last_visited_ms, stats.and_then(|s| s.last_opened_ms))
                    .unwrap_or(i64::MIN);

            let replace = match &best {
                None => true,
                Some((best_score, best_last, best_url, best_id)) => {
                    (
                        frecency,
                        effective_last,
                        std::cmp::Reverse(&row.normalized_url),
                        std::cmp::Reverse(&row.entry_id),
                    ) > (
                        *best_score,
                        *best_last,
                        std::cmp::Reverse(best_url),
                        std::cmp::Reverse(best_id),
                    )
                }
            };
            if replace {
                best = Some((frecency, effective_last, row.normalized_url, row.entry_id));
            }
        }

        Ok(
            best.map(|(_, _, suggested_url, source_entry_id)| CompletionDto {
                suggested_url,
                source_entry_id,
            }),
        )
    }
}
