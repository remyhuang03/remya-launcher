use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::desktop::{DesktopCatalog, IndexedAction, IndexedApp, PinyinData, PinyinUnit};
use crate::hyprland::{HyprClient, match_windows};
use crate::protocol::{AppEntry, AppWindow, PackageEntry, SearchData};
use crate::store::Store;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchTier {
    NamePrefix = 1,
    NameWordOrAcronym = 2,
    ExecOrDesktopId = 3,
    GenericName = 4,
    Keyword = 5,
    Comment = 6,
    NameSubsequence = 7,
}

#[derive(Debug, Clone)]
struct MatchInfo {
    tier: MatchTier,
    sub_rank: u32,
    match_kind: &'static str,
    match_text: String,
    action_id: Option<String>,
}

#[derive(Debug)]
struct RankedCandidate<'a> {
    app: &'a IndexedApp,
    windows: Vec<AppWindow>,
    pinned: bool,
    hidden: bool,
    winner_rank: usize,
    tier: u8,
    sub_rank: u32,
    frecency: f64,
    match_kind: Option<&'static str>,
    match_text: Option<String>,
    action_id: Option<String>,
}

pub fn run_search(
    catalog: &DesktopCatalog,
    store: &Store,
    clients: &[HyprClient],
    query: &str,
    now_secs: i64,
    packages_fn: impl FnOnce(&str, &HashSet<String>) -> Vec<PackageEntry>,
    allow_packages: bool,
) -> SearchData {
    let trimmed = query.trim();
    let prefs = store.preferences();
    let hidden_set = store.hidden_ids();
    let all_apps = catalog.apps();

    let total_visible = all_apps
        .iter()
        .filter(|app| !hidden_set.contains(&app.id))
        .count();

    let windows_by_app: HashMap<&str, Vec<AppWindow>> = all_apps
        .iter()
        .map(|app| {
            (
                app.id.as_str(),
                match_windows(app.startup_wm_class.as_deref(), &app.id, clients),
            )
        })
        .collect();

    if trimmed.is_empty() {
        return empty_query_result(
            catalog,
            store,
            &hidden_set,
            &windows_by_app,
            total_visible,
            now_secs,
        );
    }

    let norm_query = trimmed.to_lowercase();
    let query_chars = trimmed.chars().count();
    let mut visible_matches = collect_matches(
        all_apps,
        store,
        &windows_by_app,
        &hidden_set,
        false,
        &norm_query,
        query_chars,
        now_secs,
        prefs.pinyin,
    );

    if !visible_matches.is_empty() {
        sort_ranked(&mut visible_matches);
        let apps: Vec<AppEntry> = visible_matches.into_iter().map(into_app_entry).collect();
        return SearchData {
            total: total_visible,
            matched: apps.len(),
            apps,
            frequent: Vec::new(),
            pinned: Vec::new(),
            packages: Vec::new(),
            hidden: false,
            always_new: prefs.always_new,
        };
    }

    let mut hidden_matches = collect_matches(
        all_apps,
        store,
        &windows_by_app,
        &hidden_set,
        true,
        &norm_query,
        query_chars,
        now_secs,
        prefs.pinyin,
    );
    if !hidden_matches.is_empty() {
        sort_ranked(&mut hidden_matches);
        let apps: Vec<AppEntry> = hidden_matches.into_iter().map(into_app_entry).collect();
        return SearchData {
            total: total_visible,
            matched: apps.len(),
            apps,
            frequent: Vec::new(),
            pinned: Vec::new(),
            packages: Vec::new(),
            hidden: true,
            always_new: prefs.always_new,
        };
    }

    let packages = if allow_packages && prefs.show_packages && query_chars >= 3 {
        let installed_flatpak_ids: HashSet<String> = all_apps
            .iter()
            .filter(|a| a.source.as_deref() == Some("Flatpak"))
            .map(|a| a.desktop_id_stem_lower.clone())
            .collect();
        packages_fn(trimmed, &installed_flatpak_ids)
    } else {
        Vec::new()
    };

    SearchData {
        total: total_visible,
        matched: 0,
        apps: Vec::new(),
        frequent: Vec::new(),
        pinned: Vec::new(),
        packages,
        hidden: false,
        always_new: prefs.always_new,
    }
}

pub fn list_hidden_apps(
    catalog: &DesktopCatalog,
    store: &Store,
    clients: &[HyprClient],
) -> Vec<AppEntry> {
    let hidden_set = store.hidden_ids();
    let mut apps: Vec<AppEntry> = catalog
        .apps()
        .iter()
        .filter(|app| hidden_set.contains(&app.id))
        .map(|app| {
            let windows = match_windows(app.startup_wm_class.as_deref(), &app.id, clients);
            build_plain_entry(app, windows, store.is_pinned(&app.id), true)
        })
        .collect();
    apps.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
    apps
}

fn empty_query_result(
    catalog: &DesktopCatalog,
    store: &Store,
    hidden_set: &BTreeSet<String>,
    windows_by_app: &HashMap<&str, Vec<AppWindow>>,
    total_visible: usize,
    now_secs: i64,
) -> SearchData {
    let all_apps = catalog.apps();
    let visible: Vec<&IndexedApp> = all_apps
        .iter()
        .filter(|app| !hidden_set.contains(&app.id))
        .collect();

    let apps: Vec<AppEntry> = visible
        .iter()
        .map(|app| {
            build_plain_entry(
                app,
                windows_by_app
                    .get(app.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                store.is_pinned(&app.id),
                false,
            )
        })
        .collect();

    let mut freq_candidates: Vec<(&IndexedApp, f64, i64)> = visible
        .iter()
        .filter_map(|&app| {
            let score = store.frecency(&app.id, now_secs);
            if score > 0.0 {
                Some((app, score, store.last_used(&app.id, now_secs)))
            } else {
                None
            }
        })
        .collect();
    freq_candidates.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.2.cmp(&a.2))
            .then_with(|| a.0.name_lower.cmp(&b.0.name_lower))
            .then_with(|| a.0.id.cmp(&b.0.id))
    });
    let frequent: Vec<AppEntry> = freq_candidates
        .into_iter()
        .take(6)
        .map(|(app, _, _)| {
            build_plain_entry(
                app,
                windows_by_app
                    .get(app.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                store.is_pinned(&app.id),
                false,
            )
        })
        .collect();

    let pinned: Vec<AppEntry> = store
        .pinned_ids()
        .iter()
        .filter(|id| !hidden_set.contains(id.as_str()))
        .filter_map(|id| catalog.get(id))
        .map(|app| {
            build_plain_entry(
                app,
                windows_by_app
                    .get(app.id.as_str())
                    .cloned()
                    .unwrap_or_default(),
                true,
                false,
            )
        })
        .collect();

    SearchData {
        total: total_visible,
        matched: apps.len(),
        apps,
        frequent,
        pinned,
        packages: Vec::new(),
        hidden: false,
        always_new: store.preferences().always_new,
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_matches<'a>(
    apps: &'a [IndexedApp],
    store: &Store,
    windows_by_app: &HashMap<&str, Vec<AppWindow>>,
    hidden_set: &BTreeSet<String>,
    want_hidden: bool,
    norm_query: &str,
    query_chars: usize,
    now_secs: i64,
    use_pinyin: bool,
) -> Vec<RankedCandidate<'a>> {
    let tokens: Vec<&str> = norm_query.split_whitespace().collect();
    let mut results = Vec::new();

    for app in apps {
        let is_hidden = hidden_set.contains(&app.id);
        if is_hidden != want_hidden {
            continue;
        }
        let windows = windows_by_app
            .get(app.id.as_str())
            .cloned()
            .unwrap_or_default();
        let pinned = store.is_pinned(&app.id);
        let frecency = store.frecency(&app.id, now_secs);
        let winner_rank = store
            .query_winner_rank(norm_query, &app.id)
            .unwrap_or(usize::MAX);

        if let Some(m) = match_app(app, norm_query, query_chars, use_pinyin) {
            results.push(RankedCandidate {
                app,
                windows: windows.clone(),
                pinned,
                hidden: is_hidden,
                winner_rank,
                tier: m.tier as u8,
                sub_rank: m.sub_rank,
                frecency,
                match_kind: Some(m.match_kind),
                match_text: Some(m.match_text),
                action_id: m.action_id,
            });
        }

        if tokens.len() >= 2 && !app.actions.is_empty() {
            let mut best_by_action: HashMap<&str, MatchInfo> = HashMap::new();
            for split in 1..tokens.len() {
                let app_query = tokens[..split].join(" ");
                let action_query = tokens[split..].join(" ");
                let Some(app_match) = match_app_for_action_split(app, &app_query, use_pinyin)
                else {
                    continue;
                };
                for action in &app.actions {
                    let Some((action_tier, action_sub)) =
                        match_action(action, &action_query, use_pinyin)
                    else {
                        continue;
                    };
                    let combined_tier =
                        if app_match.tier <= MatchTier::NameWordOrAcronym && action_tier <= 2 {
                            app_match.tier
                        } else {
                            app_match.tier.max(MatchTier::NameWordOrAcronym)
                        };
                    let combined_sub = (app_match.sub_rank * 10) + action_sub;
                    let candidate = MatchInfo {
                        tier: combined_tier,
                        sub_rank: combined_sub,
                        match_kind: "action",
                        match_text: action.name.clone(),
                        action_id: Some(action.id.clone()),
                    };
                    best_by_action
                        .entry(action.id.as_str())
                        .and_modify(|existing| {
                            if (candidate.tier, candidate.sub_rank)
                                < (existing.tier, existing.sub_rank)
                            {
                                *existing = candidate.clone();
                            }
                        })
                        .or_insert(candidate);
                }
            }
            for (_, m) in best_by_action {
                results.push(RankedCandidate {
                    app,
                    windows: windows.clone(),
                    pinned,
                    hidden: is_hidden,
                    winner_rank: usize::MAX,
                    tier: m.tier as u8,
                    sub_rank: m.sub_rank,
                    frecency,
                    match_kind: Some(m.match_kind),
                    match_text: Some(m.match_text),
                    action_id: m.action_id,
                });
            }
        }
    }
    results
}

fn sort_ranked(items: &mut [RankedCandidate<'_>]) {
    items.sort_by(|a, b| {
        a.winner_rank
            .cmp(&b.winner_rank)
            .then_with(|| a.tier.cmp(&b.tier))
            .then_with(|| a.sub_rank.cmp(&b.sub_rank))
            .then_with(|| {
                b.frecency
                    .partial_cmp(&a.frecency)
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| b.action_id.is_none().cmp(&a.action_id.is_none()))
            .then_with(|| a.app.name_lower.cmp(&b.app.name_lower))
            .then_with(|| a.app.id.cmp(&b.app.id))
            .then_with(|| a.action_id.cmp(&b.action_id))
    });
}

fn match_app_for_action_split(
    app: &IndexedApp,
    query: &str,
    use_pinyin: bool,
) -> Option<MatchInfo> {
    let q_chars = query.chars().count();
    let m = match_app(app, query, q_chars, use_pinyin)?;
    if matches!(
        m.tier,
        MatchTier::NamePrefix
            | MatchTier::NameWordOrAcronym
            | MatchTier::ExecOrDesktopId
            | MatchTier::NameSubsequence
    ) {
        Some(m)
    } else {
        None
    }
}

fn match_app(
    app: &IndexedApp,
    query: &str,
    query_chars: usize,
    use_pinyin: bool,
) -> Option<MatchInfo> {
    // Tier 1: Name exact prefix (localized or English) or full pinyin prefix.
    if app.name_lower == query {
        return Some(name_match(MatchTier::NamePrefix, 0, &app.name));
    }
    if app.english_name_lower == query {
        return Some(name_match(MatchTier::NamePrefix, 1, &app.english_name));
    }
    if app.name_lower.starts_with(query) {
        return Some(name_match(MatchTier::NamePrefix, 2, &app.name));
    }
    if app.english_name_lower.starts_with(query) {
        return Some(name_match(MatchTier::NamePrefix, 3, &app.english_name));
    }
    if use_pinyin && query.is_ascii() {
        let compact = compact_ascii(query);
        if !compact.is_empty() {
            if match_pinyin_full(&app.name_pinyin, &compact) {
                return Some(name_match(MatchTier::NamePrefix, 4, &app.name));
            }
            if match_pinyin_full(&app.english_pinyin, &compact) {
                return Some(name_match(MatchTier::NamePrefix, 5, &app.english_name));
            }
        }
    }

    // Tier 2: Name word initials / camelCase / hyphen acronyms, word prefix, pinyin initials.
    if let Some(sub) = match_words_or_acronym(&app.name_acronym, &app.name_words, query) {
        return Some(name_match(MatchTier::NameWordOrAcronym, sub, &app.name));
    }
    if let Some(sub) = match_words_or_acronym(&app.english_acronym, &app.english_words, query) {
        return Some(name_match(
            MatchTier::NameWordOrAcronym,
            sub + 10,
            &app.english_name,
        ));
    }
    if use_pinyin && query.is_ascii() {
        let compact = compact_ascii(query);
        if !compact.is_empty() {
            if match_pinyin_initials(&app.name_pinyin, &compact) {
                return Some(name_match(MatchTier::NameWordOrAcronym, 20, &app.name));
            }
            if match_pinyin_initials(&app.english_pinyin, &compact) {
                return Some(name_match(
                    MatchTier::NameWordOrAcronym,
                    21,
                    &app.english_name,
                ));
            }
        }
    }

    // Tier 3: Executable filename & desktop ID.
    if !app.exec_basename_lower.is_empty() {
        if app.exec_basename_lower == query {
            return Some(name_match(
                MatchTier::ExecOrDesktopId,
                0,
                &app.exec_basename_lower,
            ));
        }
        if app.exec_basename_lower.starts_with(query) {
            return Some(name_match(
                MatchTier::ExecOrDesktopId,
                1,
                &app.exec_basename_lower,
            ));
        }
    }
    if app.desktop_id_stem_lower == query {
        return Some(name_match(MatchTier::ExecOrDesktopId, 2, &app.id));
    }
    if app.desktop_id_stem_lower.starts_with(query) {
        return Some(name_match(MatchTier::ExecOrDesktopId, 3, &app.id));
    }
    if !app.exec_basename_lower.is_empty() && app.exec_basename_lower.contains(query) {
        return Some(name_match(
            MatchTier::ExecOrDesktopId,
            4,
            &app.exec_basename_lower,
        ));
    }
    if app.desktop_id_stem_lower.contains(query) {
        return Some(name_match(MatchTier::ExecOrDesktopId, 5, &app.id));
    }

    // Tier 4: GenericName word starts / prefix / substring.
    if !app.generic_name_lower.is_empty()
        && (app.generic_name_lower.starts_with(query)
            || match_words_or_acronym(&app.generic_acronym, &app.generic_words, query).is_some()
            || (query_chars >= 2 && app.generic_name_lower.contains(query)))
    {
        return Some(MatchInfo {
            tier: MatchTier::GenericName,
            sub_rank: if app.generic_name_lower.starts_with(query) {
                0
            } else {
                1
            },
            match_kind: "kind",
            match_text: app.generic_name.clone(),
            action_id: None,
        });
    }

    // Tier 5: Keywords.
    for (orig, lower) in &app.keywords {
        if lower == query || lower.starts_with(query) || (query_chars >= 2 && lower.contains(query))
        {
            return Some(MatchInfo {
                tier: MatchTier::Keyword,
                sub_rank: if lower.starts_with(query) { 0 } else { 1 },
                match_kind: "keyword",
                match_text: orig.clone(),
                action_id: None,
            });
        }
    }

    // Tier 6: Comment (only when query >= 3 chars).
    if query_chars >= 3 && !app.comment_lower.is_empty() && app.comment_lower.contains(query) {
        return Some(MatchInfo {
            tier: MatchTier::Comment,
            sub_rank: 0,
            match_kind: "comment",
            match_text: app.comment.clone(),
            action_id: None,
        });
    }

    // Tier 7: Name subsequence fuzzy match.
    let compact_q = compact_query(query);
    if !compact_q.is_empty() {
        if let Some(penalty) = subsequence_score(&app.name_lower, &compact_q) {
            return Some(name_match(MatchTier::NameSubsequence, penalty, &app.name));
        }
        if let Some(penalty) = subsequence_score(&app.english_name_lower, &compact_q) {
            return Some(name_match(
                MatchTier::NameSubsequence,
                penalty + 50,
                &app.english_name,
            ));
        }
    }

    None
}

fn match_action(action: &IndexedAction, query: &str, use_pinyin: bool) -> Option<(u8, u32)> {
    if action.name_lower == query {
        return Some((1, 0));
    }
    if action.name_lower.starts_with(query) {
        return Some((1, 1));
    }
    if use_pinyin && query.is_ascii() {
        let compact = compact_ascii(query);
        if !compact.is_empty() && match_pinyin_full(&action.pinyin, &compact) {
            return Some((1, 2));
        }
    }
    if let Some(sub) = match_words_or_acronym(&action.acronym, &action.words, query) {
        return Some((2, sub));
    }
    if use_pinyin && query.is_ascii() {
        let compact = compact_ascii(query);
        if !compact.is_empty() && match_pinyin_initials(&action.pinyin, &compact) {
            return Some((2, 10));
        }
    }
    if action.name_lower.contains(query) {
        return Some((3, 0));
    }
    let compact_q = compact_query(query);
    if !compact_q.is_empty()
        && let Some(p) = subsequence_score(&action.name_lower, &compact_q)
    {
        return Some((4, p));
    }
    None
}

fn name_match(tier: MatchTier, sub_rank: u32, text: &str) -> MatchInfo {
    MatchInfo {
        tier,
        sub_rank,
        match_kind: "name",
        match_text: text.to_owned(),
        action_id: None,
    }
}

fn match_words_or_acronym(acronym: &str, words: &[String], query: &str) -> Option<u32> {
    let compact = compact_query(query);
    if compact.is_empty() {
        return None;
    }
    if acronym.starts_with(&compact) {
        return Some(0);
    }
    if words.iter().any(|w| w.starts_with(query)) {
        return Some(1);
    }
    if acronym.contains(&compact) {
        return Some(2);
    }
    if words.iter().any(|w| w.starts_with(&compact)) {
        return Some(3);
    }
    // Multi-word query tokens matching ordered words by prefix.
    let q_tokens: Vec<&str> = query.split_whitespace().collect();
    if q_tokens.len() > 1 {
        let mut w_idx = 0;
        let mut matched_all = true;
        for tok in q_tokens {
            if let Some(pos) = words[w_idx..].iter().position(|w| w.starts_with(tok)) {
                w_idx += pos + 1;
            } else {
                matched_all = false;
                break;
            }
        }
        if matched_all {
            return Some(4);
        }
    }
    None
}

fn compact_query(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn compact_ascii(s: &str) -> String {
    s.bytes()
        .filter(|b| b.is_ascii_alphanumeric())
        .map(|b| b.to_ascii_lowercase() as char)
        .collect()
}

fn subsequence_score(haystack: &str, needle: &str) -> Option<u32> {
    let hay: Vec<char> = haystack.chars().collect();
    let ned: Vec<char> = needle.chars().collect();
    if ned.is_empty() || ned.len() > hay.len() {
        return None;
    }
    let mut hi = 0usize;
    let mut first_pos = None;
    let mut last_pos = 0usize;
    let mut gaps = 0u32;

    for &nc in &ned {
        let mut found = false;
        while hi < hay.len() {
            if hay[hi] == nc {
                if first_pos.is_none() {
                    first_pos = Some(hi);
                } else if hi > last_pos + 1 {
                    gaps += (hi - last_pos - 1) as u32;
                }
                last_pos = hi;
                hi += 1;
                found = true;
                break;
            }
            hi += 1;
        }
        if !found {
            return None;
        }
    }
    let start_penalty = first_pos.unwrap_or(0) as u32;
    Some(gaps * 2 + start_penalty)
}

fn match_pinyin_full(data: &PinyinData, query: &str) -> bool {
    if data.units.is_empty() || query.is_empty() {
        return false;
    }
    fn dfs(units: &[PinyinUnit], ui: usize, rem: &str) -> bool {
        if rem.is_empty() {
            return true;
        }
        if ui >= units.len() {
            return false;
        }
        match &units[ui] {
            PinyinUnit::Ascii(ch) => {
                if let Some(rest) = rem.strip_prefix(*ch) {
                    dfs(units, ui + 1, rest)
                } else {
                    false
                }
            }
            PinyinUnit::Hanzi(readings) => {
                for &reading in readings {
                    if let Some(rest) = rem.strip_prefix(reading) {
                        if dfs(units, ui + 1, rest) {
                            return true;
                        }
                    } else if reading.starts_with(rem) {
                        return true;
                    }
                }
                false
            }
        }
    }
    dfs(&data.units, 0, query)
}

fn match_pinyin_initials(data: &PinyinData, query: &str) -> bool {
    if data.units.is_empty() || query.is_empty() {
        return false;
    }
    let q: Vec<u8> = query.bytes().collect();
    // Allow prefix match of initials starting at unit 0, or matching initials at any start.
    fn dfs(units: &[PinyinUnit], ui: usize, qi: usize, q: &[u8]) -> bool {
        if qi == q.len() {
            return true;
        }
        if ui >= units.len() {
            return false;
        }
        let target = q[qi];
        match &units[ui] {
            PinyinUnit::Ascii(ch) => {
                if *ch as u8 == target && dfs(units, ui + 1, qi + 1, q) {
                    return true;
                }
            }
            PinyinUnit::Hanzi(readings) => {
                if readings
                    .iter()
                    .any(|r| r.as_bytes().first().copied() == Some(target))
                    && dfs(units, ui + 1, qi + 1, q)
                {
                    return true;
                }
            }
        }
        false
    }
    for start in 0..data.units.len() {
        if dfs(&data.units, start, 0, &q) {
            return true;
        }
    }
    false
}

fn build_plain_entry(
    app: &IndexedApp,
    windows: Vec<AppWindow>,
    pinned: bool,
    hidden: bool,
) -> AppEntry {
    AppEntry {
        id: app.id.clone(),
        name: app.name.clone(),
        description: app.description.clone(),
        icon: app.icon.clone(),
        source: app.source.clone(),
        terminal: app.terminal,
        windows,
        actions: app.desktop_actions(),
        match_kind: None,
        match_text: None,
        action_id: None,
        pinned,
        hidden,
    }
}

fn into_app_entry(c: RankedCandidate<'_>) -> AppEntry {
    AppEntry {
        id: c.app.id.clone(),
        name: c.app.name.clone(),
        description: c.app.description.clone(),
        icon: c.app.icon.clone(),
        source: c.app.source.clone(),
        terminal: c.app.terminal,
        windows: c.windows,
        actions: c.app.desktop_actions(),
        match_kind: c.match_kind.map(str::to_owned),
        match_text: c.match_text,
        action_id: c.action_id,
        pinned: c.pinned,
        hidden: c.hidden,
    }
}
