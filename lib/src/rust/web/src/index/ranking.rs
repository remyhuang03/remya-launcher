use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::MAX_FREQUENT_SITES;
use super::dto::{
    EntryKind, FaviconDto, FrequentSiteDto, IndexedRow, MatchTier, OpenStats, RankedGroupEntry,
    RowDto, ScoredRow, SearchSuggestionDto, SourceRefDto, SourceSummaryDto,
};
use super::util::{max_opt_i64, monogram_for};

pub(super) fn build_groups(
    bookmarks: Vec<RowDto>,
    history: Vec<RowDto>,
    suggestions: Vec<SearchSuggestionDto>,
) -> Vec<Value> {
    vec![
        serde_json::json!({ "kind": "bookmarks", "items": bookmarks }),
        serde_json::json!({ "kind": "history", "items": history }),
        serde_json::json!({ "kind": "search", "items": suggestions }),
    ]
}

pub(super) fn dedup_and_rank_group(mut items: Vec<ScoredRow>) -> Vec<RankedGroupEntry> {
    items.sort_by(compare_scored);

    let mut grouped: HashMap<String, Vec<ScoredRow>> = HashMap::new();
    for item in items {
        grouped
            .entry(item.row.normalized_url.clone())
            .or_default()
            .push(item);
    }

    let mut merged: Vec<RankedGroupEntry> = grouped
        .into_values()
        .map(|mut group| {
            group.sort_by(compare_scored);
            let primary = &group[0];
            let best_tier = primary.tier;
            let best_score = primary.score;
            let mut max_effective_last = primary.effective_last_ms;
            let mut max_visited_last = primary.row.last_visited_ms;
            let mut total_visits: i64 = 0;

            let mut seen_sources = HashSet::new();
            let mut sources = Vec::new();
            let mut browser_labels = Vec::new();
            let mut folder_path = primary.row.folder_path.clone();
            let mut title = primary.row.title.clone();

            for item in &group {
                total_visits = total_visits.saturating_add(item.row.visited_count);
                max_effective_last = max_opt_i64(max_effective_last, item.effective_last_ms);
                max_visited_last = max_opt_i64(max_visited_last, item.row.last_visited_ms);
                if folder_path.is_empty() && !item.row.folder_path.is_empty() {
                    folder_path = item.row.folder_path.clone();
                }
                if title.is_empty() && !item.row.title.is_empty() {
                    title = item.row.title.clone();
                }
                let src_key = (item.row.browser_id.clone(), item.row.profile_id.clone());
                if seen_sources.insert(src_key) {
                    sources.push(SourceRefDto {
                        browser_id: item.row.browser_id.clone(),
                        profile_id: item.row.profile_id.clone(),
                    });
                }
                browser_labels.push((item.row.browser_id.clone(), item.row.browser_label.clone()));
            }

            let display_title = if title.is_empty() {
                primary.row.host.clone()
            } else {
                title
            };
            let fallback_text = monogram_for(&display_title, &primary.row.host);

            RankedGroupEntry {
                row: RowDto {
                    entry_id: primary.row.entry_id.clone(),
                    title: display_title,
                    url: primary.row.normalized_url.clone(),
                    hostname: primary.row.host.clone(),
                    folder_path,
                    sources,
                    visited_count: total_visits,
                    last_visited_ms: max_visited_last,
                    favicon: FaviconDto {
                        status: "monogram",
                        fallback_text,
                    },
                },
                browser_labels,
                tier: best_tier,
                score: best_score,
                effective_last_ms: max_effective_last,
            }
        })
        .collect();

    merged.sort_by(|a, b| {
        a.tier
            .cmp(&b.tier)
            .then_with(|| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| b.effective_last_ms.cmp(&a.effective_last_ms))
            .then_with(|| b.row.visited_count.cmp(&a.row.visited_count))
            .then_with(|| a.row.url.cmp(&b.row.url))
            .then_with(|| a.row.entry_id.cmp(&b.row.entry_id))
    });
    merged
}

pub(super) fn compute_source_summary(
    bookmarks: &[RankedGroupEntry],
    history: &[RankedGroupEntry],
) -> Vec<SourceSummaryDto> {
    let mut counts: HashMap<String, (String, usize)> = HashMap::new();
    for entry in bookmarks.iter().chain(history.iter()) {
        let mut seen_browsers = HashSet::new();
        for (browser_id, label) in &entry.browser_labels {
            if seen_browsers.insert(browser_id.as_str()) {
                let slot = counts
                    .entry(browser_id.clone())
                    .or_insert_with(|| (label.clone(), 0));
                if slot.0.is_empty() && !label.is_empty() {
                    slot.0 = label.clone();
                }
                slot.1 += 1;
            }
        }
    }

    let mut summary: Vec<SourceSummaryDto> = counts
        .into_iter()
        .map(|(browser_id, (label, count))| SourceSummaryDto {
            label: if label.is_empty() {
                browser_id.clone()
            } else {
                label
            },
            browser_id,
            count,
        })
        .collect();

    summary.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.browser_id.cmp(&b.browser_id))
    });
    summary
}

pub(super) fn compute_frequent_sites(
    rows: &[IndexedRow],
    pinned_rows: &[IndexedRow],
    open_stats: &HashMap<String, OpenStats>,
    now_ms: i64,
) -> Vec<FrequentSiteDto> {
    struct HostBucket<'a> {
        total_score: f64,
        best_row: &'a IndexedRow,
        best_row_score: f64,
        max_last_ms: Option<i64>,
        total_visits: i64,
        seen_urls: HashSet<&'a str>,
        sources: Vec<SourceRefDto>,
        seen_sources: HashSet<(&'a str, &'a str)>,
    }

    let mut out = Vec::with_capacity(MAX_FREQUENT_SITES);
    let mut pinned_urls: HashSet<&str> = HashSet::new();
    let mut pinned_hosts: HashSet<&str> = HashSet::new();

    for pinned in pinned_rows {
        if out.len() >= MAX_FREQUENT_SITES {
            break;
        }
        if !pinned_urls.insert(pinned.normalized_url.as_str()) {
            continue;
        }
        pinned_hosts.insert(pinned.host.as_str());

        let mut total_visits: i64 = 0;
        let mut max_last_ms: Option<i64> = open_stats
            .get(&pinned.normalized_url)
            .and_then(|s| s.last_opened_ms);
        let mut sources = Vec::new();
        let mut seen_sources = HashSet::new();
        let mut seen_entry_ids = HashSet::new();
        let mut title = pinned.title.clone();
        let mut folder_path = pinned.folder_path.clone();

        for row in std::iter::once(pinned)
            .chain(pinned_rows.iter())
            .chain(rows.iter())
        {
            if row.normalized_url != pinned.normalized_url
                || !seen_entry_ids.insert(row.entry_id.as_str())
            {
                continue;
            }
            total_visits = total_visits.saturating_add(row.visited_count);
            max_last_ms = max_opt_i64(max_last_ms, row.last_visited_ms);
            if title.is_empty() && !row.title.is_empty() {
                title = row.title.clone();
            }
            if folder_path.is_empty() && !row.folder_path.is_empty() {
                folder_path = row.folder_path.clone();
            }
            if seen_sources.insert((row.browser_id.as_str(), row.profile_id.as_str())) {
                sources.push(SourceRefDto {
                    browser_id: row.browser_id.clone(),
                    profile_id: row.profile_id.clone(),
                });
            }
        }
        sources.sort_by(|a, b| {
            a.browser_id
                .cmp(&b.browser_id)
                .then_with(|| a.profile_id.cmp(&b.profile_id))
        });

        let display_title = if title.is_empty() {
            pinned.host.clone()
        } else {
            title
        };
        let fallback_text = monogram_for(&display_title, &pinned.host);
        out.push(FrequentSiteDto {
            entry_id: pinned.entry_id.clone(),
            title: display_title,
            url: pinned.normalized_url.clone(),
            representative_url: pinned.normalized_url.clone(),
            origin: pinned.origin.clone(),
            hostname: pinned.host.clone(),
            folder_path,
            sources,
            visited_count: total_visits,
            last_visited_ms: max_last_ms,
            favicon: FaviconDto {
                status: "monogram",
                fallback_text,
            },
        });
    }

    if out.len() >= MAX_FREQUENT_SITES {
        return out;
    }

    let mut by_host: HashMap<&str, HostBucket<'_>> = HashMap::new();
    for row in rows {
        if pinned_hosts.contains(row.host.as_str()) {
            continue;
        }
        let stats = open_stats.get(&row.normalized_url);
        let has_activity = row.visited_count > 0
            || row.last_visited_ms.is_some()
            || stats.is_some_and(|s| s.recent_count > 0);
        if !has_activity {
            continue;
        }

        let row_score = compute_frecency_score(row, stats, now_ms)
            + if row.kind == EntryKind::Bookmark {
                0.35
            } else {
                0.0
            };
        let eff_last = max_opt_i64(row.last_visited_ms, stats.and_then(|s| s.last_opened_ms));

        let bucket = by_host.entry(&row.host).or_insert_with(|| HostBucket {
            total_score: 0.0,
            best_row: row,
            best_row_score: f64::NEG_INFINITY,
            max_last_ms: None,
            total_visits: 0,
            seen_urls: HashSet::new(),
            sources: Vec::new(),
            seen_sources: HashSet::new(),
        });

        if bucket.seen_urls.insert(row.normalized_url.as_str()) {
            bucket.total_score += row_score;
        } else if row_score > bucket.best_row_score {
            bucket.total_score += (row_score - bucket.best_row_score).max(0.0);
        }
        bucket.total_visits = bucket.total_visits.saturating_add(row.visited_count);
        bucket.max_last_ms = max_opt_i64(bucket.max_last_ms, eff_last);
        if bucket
            .seen_sources
            .insert((row.browser_id.as_str(), row.profile_id.as_str()))
        {
            bucket.sources.push(SourceRefDto {
                browser_id: row.browser_id.clone(),
                profile_id: row.profile_id.clone(),
            });
        }

        let is_better_rep = (row_score, eff_last, std::cmp::Reverse(&row.normalized_url))
            > (
                bucket.best_row_score,
                bucket.max_last_ms,
                std::cmp::Reverse(&bucket.best_row.normalized_url),
            );
        if is_better_rep {
            bucket.best_row = row;
            bucket.best_row_score = row_score;
        }
    }

    let mut ranked: Vec<(&str, HostBucket<'_>)> = by_host.into_iter().collect();
    ranked.sort_by(|(host_a, a), (host_b, b)| {
        b.total_score
            .partial_cmp(&a.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.max_last_ms.cmp(&a.max_last_ms))
            .then_with(|| b.total_visits.cmp(&a.total_visits))
            .then_with(|| host_a.cmp(host_b))
    });

    let remaining = MAX_FREQUENT_SITES - out.len();
    out.extend(ranked.into_iter().take(remaining).map(|(_, mut bucket)| {
        bucket.sources.sort_by(|a, b| {
            a.browser_id
                .cmp(&b.browser_id)
                .then_with(|| a.profile_id.cmp(&b.profile_id))
        });
        let rep = bucket.best_row;
        let title = if rep.title.is_empty() {
            rep.host.clone()
        } else {
            rep.title.clone()
        };
        let fallback_text = monogram_for(&title, &rep.host);
        FrequentSiteDto {
            entry_id: rep.entry_id.clone(),
            title,
            url: rep.normalized_url.clone(),
            representative_url: rep.normalized_url.clone(),
            origin: rep.origin.clone(),
            hostname: rep.host.clone(),
            folder_path: rep.folder_path.clone(),
            sources: bucket.sources,
            visited_count: bucket.total_visits,
            last_visited_ms: bucket.max_last_ms,
            favicon: FaviconDto {
                status: "monogram",
                fallback_text,
            },
        }
    }));
    out
}

pub(super) fn compute_recent_items(
    rows: &[IndexedRow],
    open_stats: &HashMap<String, OpenStats>,
    limit: usize,
) -> Vec<RowDto> {
    struct RecentBucket<'a> {
        primary: &'a IndexedRow,
        last_ms: i64,
        total_visits: i64,
        sources: Vec<SourceRefDto>,
        seen_sources: HashSet<(&'a str, &'a str)>,
    }

    let mut by_url: HashMap<&str, RecentBucket<'_>> = HashMap::new();
    for row in rows {
        let stats = open_stats.get(&row.normalized_url);
        let Some(eff_last) = max_opt_i64(row.last_visited_ms, stats.and_then(|s| s.last_opened_ms))
        else {
            continue;
        };

        let bucket = by_url
            .entry(row.normalized_url.as_str())
            .or_insert_with(|| RecentBucket {
                primary: row,
                last_ms: eff_last,
                total_visits: 0,
                sources: Vec::new(),
                seen_sources: HashSet::new(),
            });
        bucket.total_visits = bucket.total_visits.saturating_add(row.visited_count);
        if eff_last > bucket.last_ms
            || (eff_last == bucket.last_ms && row.kind == EntryKind::Bookmark)
        {
            bucket.last_ms = eff_last;
            bucket.primary = row;
        }
        if bucket
            .seen_sources
            .insert((row.browser_id.as_str(), row.profile_id.as_str()))
        {
            bucket.sources.push(SourceRefDto {
                browser_id: row.browser_id.clone(),
                profile_id: row.profile_id.clone(),
            });
        }
    }

    let mut list: Vec<RecentBucket<'_>> = by_url.into_values().collect();
    list.sort_by(|a, b| {
        b.last_ms
            .cmp(&a.last_ms)
            .then_with(|| b.total_visits.cmp(&a.total_visits))
            .then_with(|| a.primary.normalized_url.cmp(&b.primary.normalized_url))
            .then_with(|| a.primary.entry_id.cmp(&b.primary.entry_id))
    });

    list.into_iter()
        .take(limit)
        .map(|mut bucket| {
            bucket.sources.sort_by(|a, b| {
                a.browser_id
                    .cmp(&b.browser_id)
                    .then_with(|| a.profile_id.cmp(&b.profile_id))
            });
            let row = bucket.primary;
            let title = if row.title.is_empty() {
                row.host.clone()
            } else {
                row.title.clone()
            };
            let fallback_text = monogram_for(&title, &row.host);
            RowDto {
                entry_id: row.entry_id.clone(),
                title,
                url: row.normalized_url.clone(),
                hostname: row.host.clone(),
                folder_path: row.folder_path.clone(),
                sources: bucket.sources,
                visited_count: bucket.total_visits,
                last_visited_ms: Some(bucket.last_ms),
                favicon: FaviconDto {
                    status: "monogram",
                    fallback_text,
                },
            }
        })
        .collect()
}

pub(super) fn score_candidate(
    row: IndexedRow,
    search_text: &str,
    open_stats: Option<&OpenStats>,
    now_ms: i64,
) -> Option<ScoredRow> {
    let trimmed = search_text.trim();
    let tier = if trimmed.is_empty() {
        MatchTier::Substring
    } else {
        classify_match_tier(&row, trimmed)?
    };

    let mut score = compute_frecency_score(&row, open_stats, now_ms);
    if row.kind == EntryKind::Bookmark {
        score += 1.5;
    }
    if !trimmed.is_empty() {
        let q_lower = trimmed.to_lowercase();
        if row.title.to_lowercase().contains(&q_lower) {
            score += 0.6;
        }
        if row.host.contains(&q_lower) {
            score += 0.5;
        }
    }

    let effective_last_ms = max_opt_i64(
        row.last_visited_ms,
        open_stats.and_then(|s| s.last_opened_ms),
    );

    Some(ScoredRow {
        row,
        tier,
        score,
        effective_last_ms,
    })
}

fn classify_match_tier(row: &IndexedRow, query: &str) -> Option<MatchTier> {
    let q_lower = query.to_lowercase();
    let url_lower = row.normalized_url.to_lowercase();
    let url_no_scheme = url_lower
        .strip_prefix("https://")
        .or_else(|| url_lower.strip_prefix("http://"))
        .unwrap_or(&url_lower);
    let url_no_trailing = url_no_scheme.strip_suffix('/').unwrap_or(url_no_scheme);
    let title_lower = row.title.to_lowercase();
    let folder_lower = row.folder_path.join(" ").to_lowercase();

    if url_lower == q_lower || url_no_scheme == q_lower || url_no_trailing == q_lower {
        return Some(MatchTier::ExactUrl);
    }

    if row.host.starts_with(&q_lower) || url_no_scheme.starts_with(&q_lower) {
        return Some(MatchTier::HostOrUrlPrefix);
    }

    if title_lower.starts_with(&q_lower) {
        return Some(MatchTier::TitlePrefix);
    }

    let tokens: Vec<String> = q_lower.split_whitespace().map(str::to_owned).collect();
    if tokens.is_empty() {
        return None;
    }

    let haystack_fields = [&title_lower, &row.host, &url_lower, &folder_lower];
    for tok in &tokens {
        if !haystack_fields
            .iter()
            .any(|field| field.contains(tok.as_str()))
        {
            return None;
        }
    }

    let all_word_boundary = tokens.iter().all(|tok| {
        has_word_prefix(&title_lower, tok)
            || has_word_prefix(&row.host, tok)
            || has_word_prefix(url_no_scheme, tok)
            || has_word_prefix(&folder_lower, tok)
    });

    if all_word_boundary {
        Some(MatchTier::WordBoundary)
    } else {
        Some(MatchTier::Substring)
    }
}

fn has_word_prefix(text: &str, token: &str) -> bool {
    if text.starts_with(token) {
        return true;
    }
    let mut prev_sep = false;
    for (idx, ch) in text.char_indices() {
        if prev_sep && text[idx..].starts_with(token) {
            return true;
        }
        prev_sep = !ch.is_alphanumeric();
    }
    false
}

pub(super) fn compute_frecency_score(
    row: &IndexedRow,
    open_stats: Option<&OpenStats>,
    now_ms: i64,
) -> f64 {
    let visit_freq = 0.8 * ((row.visited_count.clamp(0, 10_000)) as f64).ln_1p();
    let visit_recency = row
        .last_visited_ms
        .map(|ts| {
            let age_days = ((now_ms - ts).max(0) as f64) / 86_400_000.0;
            2.0 * (-age_days / 14.0).exp()
        })
        .unwrap_or(0.0);

    let launcher_boost = match open_stats {
        Some(stats) if stats.recent_count > 0 => {
            let freq = 1.6 * (stats.recent_count as f64).ln_1p();
            let recency = stats
                .last_opened_ms
                .map(|ts| {
                    let age_days = ((now_ms - ts).max(0) as f64) / 86_400_000.0;
                    1.2 * (-age_days / 7.0).exp()
                })
                .unwrap_or(0.0);
            freq + recency
        }
        _ => 0.0,
    };

    visit_freq + visit_recency + launcher_boost
}

fn compare_scored(a: &ScoredRow, b: &ScoredRow) -> std::cmp::Ordering {
    a.tier
        .cmp(&b.tier)
        .then_with(|| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| b.effective_last_ms.cmp(&a.effective_last_ms))
        .then_with(|| b.row.visited_count.cmp(&a.row.visited_count))
        .then_with(|| a.row.normalized_url.cmp(&b.row.normalized_url))
        .then_with(|| a.row.entry_id.cmp(&b.row.entry_id))
}
