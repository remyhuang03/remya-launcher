use std::collections::{HashMap, HashSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use gio::DesktopAppInfo;
use gio::prelude::*;
use inotify::{Inotify, WatchMask};
use pinyin::ToPinyinMulti;

use crate::protocol::AppAction;
use crate::store::absolute_env;

const METADATA_THROTTLE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone)]
pub struct IndexedAction {
    pub id: String,
    pub name: String,
    pub name_lower: String,
    pub acronym: String,
    pub words: Vec<String>,
    pub pinyin: PinyinData,
    pub exec_raw: String,
}

#[derive(Debug, Clone, Default)]
pub struct PinyinData {
    /// Per-character/token reading candidates (empty when no Hanzi is present).
    pub units: Vec<PinyinUnit>,
}

#[derive(Debug, Clone)]
pub enum PinyinUnit {
    Hanzi(Vec<&'static str>),
    Ascii(char),
}

#[derive(Debug, Clone)]
pub struct IndexedApp {
    pub id: String,
    pub desktop_path: PathBuf,
    pub name: String,
    pub name_lower: String,
    pub english_name: String,
    pub english_name_lower: String,
    pub name_acronym: String,
    pub english_acronym: String,
    pub name_words: Vec<String>,
    pub english_words: Vec<String>,
    pub name_pinyin: PinyinData,
    pub english_pinyin: PinyinData,
    pub generic_name: String,
    pub generic_name_lower: String,
    pub generic_acronym: String,
    pub generic_words: Vec<String>,
    pub comment: String,
    pub comment_lower: String,
    pub description: String,
    pub keywords: Vec<(String, String)>, // (original, lower)
    pub icon: Option<String>,
    pub source: Option<String>,
    pub terminal: bool,
    pub startup_wm_class: Option<String>,
    pub exec_raw: String,
    pub try_exec: Option<String>,
    pub exec_basename_lower: String,
    pub desktop_id_stem_lower: String,
    pub actions: Vec<IndexedAction>,
}

pub struct DesktopCatalog {
    apps: Vec<IndexedApp>,
    by_id: HashMap<String, usize>,
    fingerprint: Vec<(PathBuf, Option<SystemTime>, u64)>,
    last_check: Instant,
    dirty: Arc<AtomicBool>,
    aur_desktop_paths: HashSet<PathBuf>,
    _watcher_started: bool,
}

impl DesktopCatalog {
    pub fn load(start_watcher: bool) -> Self {
        let dirty = Arc::new(AtomicBool::new(false));
        let mut catalog = Self {
            apps: Vec::new(),
            by_id: HashMap::new(),
            fingerprint: Vec::new(),
            last_check: Instant::now() - METADATA_THROTTLE,
            dirty: Arc::clone(&dirty),
            aur_desktop_paths: HashSet::new(),
            _watcher_started: false,
        };
        catalog.reload();
        if start_watcher {
            spawn_inotify_watcher(dirty);
            catalog._watcher_started = true;
        }
        catalog
    }

    pub fn apps(&self) -> &[IndexedApp] {
        &self.apps
    }

    pub fn get(&self, id: &str) -> Option<&IndexedApp> {
        self.by_id.get(id).map(|&idx| &self.apps[idx])
    }

    pub fn set_aur_desktop_paths(&mut self, paths: HashSet<PathBuf>) {
        self.aur_desktop_paths = paths;
        for app in &mut self.apps {
            if app.source.is_none() && self.aur_desktop_paths.contains(&app.desktop_path) {
                app.source = Some("AUR".into());
            }
        }
    }

    pub fn refresh_if_needed(&mut self) -> bool {
        let is_dirty = self.dirty.swap(false, Ordering::AcqRel);
        if !is_dirty && self.last_check.elapsed() < METADATA_THROTTLE {
            return false;
        }
        self.last_check = Instant::now();
        if is_dirty {
            self.reload();
            return true;
        }
        let next_fp = compute_tree_fingerprint(&application_roots());
        if next_fp != self.fingerprint {
            self.reload_with_fingerprint(next_fp);
            return true;
        }
        false
    }

    pub fn force_reload(&mut self) {
        self.dirty.store(false, Ordering::Release);
        self.last_check = Instant::now();
        self.reload();
    }

    fn reload(&mut self) {
        let fp = compute_tree_fingerprint(&application_roots());
        self.reload_with_fingerprint(fp);
    }

    fn reload_with_fingerprint(&mut self, fingerprint: Vec<(PathBuf, Option<SystemTime>, u64)>) {
        let roots = application_roots();
        let mut seen_ids = HashSet::new();
        let mut loaded = Vec::new();

        for root in &roots {
            let is_flatpak_root = is_flatpak_applications_root(root);
            let entries = collect_desktop_files(root);
            for (desktop_id, path) in entries {
                if !seen_ids.insert(desktop_id.clone()) {
                    // User or higher-priority directory shadows lower-priority ones,
                    // even when hidden or disabled.
                    continue;
                }
                if let Some(mut app) = load_desktop_entry(&desktop_id, &path, is_flatpak_root) {
                    if app.source.is_none() && self.aur_desktop_paths.contains(&app.desktop_path) {
                        app.source = Some("AUR".into());
                    }
                    loaded.push(app);
                }
            }
        }

        loaded.sort_by(|a, b| {
            a.name_lower
                .cmp(&b.name_lower)
                .then_with(|| a.id.cmp(&b.id))
        });
        let mut by_id = HashMap::with_capacity(loaded.len());
        for (idx, app) in loaded.iter().enumerate() {
            by_id.insert(app.id.clone(), idx);
        }
        self.apps = loaded;
        self.by_id = by_id;
        self.fingerprint = fingerprint;
    }
}

pub fn user_applications_dir() -> Option<PathBuf> {
    absolute_env("XDG_DATA_HOME")
        .or_else(|| absolute_env("HOME").map(|h| h.join(".local/share")))
        .map(|d| d.join("applications"))
}

pub fn application_roots() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut seen = HashSet::new();

    let mut push_dir = |dir: PathBuf| {
        let key = fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if seen.insert(key) {
            dirs.push(dir);
        }
    };

    if let Some(user_apps) = user_applications_dir() {
        push_dir(user_apps);
    }

    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| OsString::from("/usr/local/share:/usr/share"));
    let has_system_share = env::split_paths(&data_dirs).any(|p| p == Path::new("/usr/share"));
    for entry in env::split_paths(&data_dirs) {
        if entry.is_absolute() {
            push_dir(entry.join("applications"));
        }
    }

    if let Some(user_data) = absolute_env("XDG_DATA_HOME")
        .or_else(|| absolute_env("HOME").map(|h| h.join(".local/share")))
    {
        push_dir(user_data.join("flatpak/exports/share/applications"));
    }
    if has_system_share {
        push_dir(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    }

    dirs
}

fn is_flatpak_applications_root(root: &Path) -> bool {
    let s = root.to_string_lossy();
    s.contains("/flatpak/exports/share/applications")
}

fn collect_desktop_files(root: &Path) -> Vec<(String, PathBuf)> {
    let mut results = Vec::new();
    if !root.is_dir() {
        return results;
    }
    let mut visited_dirs = HashSet::new();
    walk_desktop_dir(root, root, &mut visited_dirs, &mut results);
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

fn walk_desktop_dir(
    root: &Path,
    current: &Path,
    visited_dirs: &mut HashSet<(u64, u64)>,
    out: &mut Vec<(String, PathBuf)>,
) {
    let Ok(meta) = fs::metadata(current) else {
        return;
    };
    if !visited_dirs.insert((meta.dev(), meta.ino())) {
        return;
    }
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_meta) = fs::metadata(&path) else {
            continue;
        };
        if file_meta.is_dir() {
            walk_desktop_dir(root, &path, visited_dirs, out);
        } else if file_meta.is_file()
            && path.extension().and_then(|e| e.to_str()) == Some("desktop")
            && let Ok(rel) = path.strip_prefix(root)
        {
            let desktop_id = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("-");
            if !desktop_id.is_empty() {
                out.push((desktop_id, path));
            }
        }
    }
}

fn compute_tree_fingerprint(roots: &[PathBuf]) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let mut out = Vec::new();
    let mut visited = HashSet::new();
    for root in roots {
        if !root.is_dir() {
            out.push((root.clone(), None, 0));
            continue;
        }
        collect_dir_mtimes(root, &mut visited, &mut out);
    }
    out
}

fn collect_dir_mtimes(
    dir: &Path,
    visited: &mut HashSet<(u64, u64)>,
    out: &mut Vec<(PathBuf, Option<SystemTime>, u64)>,
) {
    let Ok(meta) = fs::metadata(dir) else {
        return;
    };
    if !visited.insert((meta.dev(), meta.ino())) {
        return;
    }
    let dir_mtime = meta.modified().ok();
    let mut max_child_mtime = dir_mtime;
    let mut count = 0u64;
    let mut subdirs = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(m) = fs::metadata(&path) {
                count += 1;
                if let Ok(mt) = m.modified()
                    && max_child_mtime.is_none_or(|cur| mt > cur)
                {
                    max_child_mtime = Some(mt);
                }
                if m.is_dir() {
                    subdirs.push(path);
                }
            }
        }
    }
    out.push((dir.to_path_buf(), max_child_mtime, count));
    subdirs.sort();
    for sub in subdirs {
        collect_dir_mtimes(&sub, visited, out);
    }
}

fn spawn_inotify_watcher(dirty: Arc<AtomicBool>) {
    std::thread::Builder::new()
        .name("apps-inotify".into())
        .spawn(move || {
            let Ok(mut inotify) = Inotify::init() else {
                return;
            };
            let mask = WatchMask::CREATE
                | WatchMask::DELETE
                | WatchMask::MODIFY
                | WatchMask::MOVED_FROM
                | WatchMask::MOVED_TO
                | WatchMask::CLOSE_WRITE
                | WatchMask::ATTRIB
                | WatchMask::DELETE_SELF
                | WatchMask::MOVE_SELF;

            let mut watched = HashSet::new();
            register_watches(&mut inotify, mask, &mut watched);

            let mut buffer = [0u8; 4096];
            loop {
                match inotify.read_events_blocking(&mut buffer) {
                    Ok(events) => {
                        let mut changed = false;
                        let mut rescan_watches = false;
                        for event in events {
                            changed = true;
                            if event.mask.contains(inotify::EventMask::ISDIR)
                                || event.mask.contains(inotify::EventMask::CREATE)
                                || event.mask.contains(inotify::EventMask::MOVED_TO)
                            {
                                rescan_watches = true;
                            }
                        }
                        if changed {
                            dirty.store(true, Ordering::Release);
                        }
                        if rescan_watches {
                            register_watches(&mut inotify, mask, &mut watched);
                        }
                    }
                    Err(_) => {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
            }
        })
        .ok();
}

fn register_watches(inotify: &mut Inotify, mask: WatchMask, watched: &mut HashSet<PathBuf>) {
    let mut visited = HashSet::new();
    for root in application_roots() {
        if root.is_dir() {
            add_watch_recursive(inotify, mask, &root, &mut visited, watched);
        } else if let Some(parent) = root.parent()
            && parent.is_dir()
            && watched.insert(parent.to_path_buf())
        {
            let _ = inotify.add_watch(parent, WatchMask::CREATE | WatchMask::MOVED_TO);
        }
    }
}

fn add_watch_recursive(
    inotify: &mut Inotify,
    mask: WatchMask,
    dir: &Path,
    visited: &mut HashSet<(u64, u64)>,
    watched: &mut HashSet<PathBuf>,
) {
    let Ok(meta) = fs::metadata(dir) else {
        return;
    };
    if !visited.insert((meta.dev(), meta.ino())) {
        return;
    }
    if watched.insert(dir.to_path_buf()) {
        let _ = inotify.add_watch(dir, mask);
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                add_watch_recursive(inotify, mask, &p, visited, watched);
            }
        }
    }
}

struct RawDesktopMeta {
    english_name: Option<String>,
    raw_icon: Option<String>,
    raw_exec: Option<String>,
    raw_try_exec: Option<String>,
    has_x_flatpak: bool,
    has_x_appimage: bool,
    action_execs: HashMap<String, String>,
}

fn parse_raw_desktop(path: &Path) -> RawDesktopMeta {
    let mut meta = RawDesktopMeta {
        english_name: None,
        raw_icon: None,
        raw_exec: None,
        raw_try_exec: None,
        has_x_flatpak: false,
        has_x_appimage: false,
        action_execs: HashMap::new(),
    };
    let Ok(text) = fs::read_to_string(path) else {
        return meta;
    };

    enum Section {
        Other,
        DesktopEntry,
        DesktopAction(String),
    }

    let mut section = Section::Other;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let group = &line[1..line.len() - 1];
            section = if group == "Desktop Entry" {
                Section::DesktopEntry
            } else if let Some(action_id) = group.strip_prefix("Desktop Action ") {
                Section::DesktopAction(action_id.to_owned())
            } else {
                Section::Other
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = unescape_desktop_value(value.trim_start());
        match &section {
            Section::DesktopEntry => match key {
                "Name" => meta.english_name = Some(value),
                "Icon" => meta.raw_icon = Some(value),
                "Exec" => meta.raw_exec = Some(value),
                "TryExec" => meta.raw_try_exec = Some(value),
                "X-Flatpak" => meta.has_x_flatpak = true,
                "X-AppImage-Version" => meta.has_x_appimage = true,
                _ => {}
            },
            Section::DesktopAction(action_id) => {
                if key == "Exec" {
                    meta.action_execs.insert(action_id.clone(), value);
                }
            }
            Section::Other => {}
        }
    }
    meta
}

fn unescape_desktop_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('s') => out.push(' '),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn load_desktop_entry(desktop_id: &str, path: &Path, is_flatpak_root: bool) -> Option<IndexedApp> {
    let info = DesktopAppInfo::from_filename(path)?;
    if !info.should_show() || info.is_hidden() {
        return None;
    }

    let raw = parse_raw_desktop(path);
    let localized_name = info.name().to_string();
    let english_name = raw
        .english_name
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| localized_name.clone());

    let generic_name = info
        .generic_name()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let comment = info
        .description()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let description = if !generic_name.is_empty() {
        generic_name.clone()
    } else {
        comment.clone()
    };

    let keywords: Vec<(String, String)> = info
        .keywords()
        .into_iter()
        .map(|k| {
            let s = k.to_string();
            let lower = s.to_lowercase();
            (s, lower)
        })
        .filter(|(s, _)| !s.is_empty())
        .collect();

    let icon = extract_icon(&info, raw.raw_icon.as_deref());
    let startup_wm_class = info
        .startup_wm_class()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());
    let terminal = info.boolean("Terminal");
    let exec_raw = info
        .string("Exec")
        .map(|s| s.to_string())
        .or(raw.raw_exec)
        .unwrap_or_default();
    let try_exec = info
        .string("TryExec")
        .map(|s| s.to_string())
        .or(raw.raw_try_exec)
        .filter(|s| !s.is_empty());

    let exec_basename_lower = extract_exec_basename(&exec_raw).to_lowercase();
    let desktop_id_stem_lower = desktop_id
        .strip_suffix(".desktop")
        .unwrap_or(desktop_id)
        .to_lowercase();

    let source = detect_source(
        is_flatpak_root,
        raw.has_x_flatpak,
        raw.has_x_appimage,
        &exec_raw,
        try_exec.as_deref(),
    );

    let actions = info
        .list_actions()
        .into_iter()
        .map(|action_id| {
            let id = action_id.to_string();
            let name = info.action_name(&id).to_string();
            let name_lower = name.to_lowercase();
            let (acronym, words) = extract_words_and_acronym(&name);
            let pinyin = build_pinyin_data(&name);
            let exec_raw = raw.action_execs.get(&id).cloned().unwrap_or_default();
            IndexedAction {
                id,
                name,
                name_lower,
                acronym,
                words,
                pinyin,
                exec_raw,
            }
        })
        .collect();

    let name_lower = localized_name.to_lowercase();
    let english_name_lower = english_name.to_lowercase();
    let (name_acronym, name_words) = extract_words_and_acronym(&localized_name);
    let (english_acronym, english_words) = extract_words_and_acronym(&english_name);
    let name_pinyin = build_pinyin_data(&localized_name);
    let english_pinyin = if english_name != localized_name {
        build_pinyin_data(&english_name)
    } else {
        PinyinData::default()
    };

    let generic_name_lower = generic_name.to_lowercase();
    let (generic_acronym, generic_words) = extract_words_and_acronym(&generic_name);
    let comment_lower = comment.to_lowercase();

    Some(IndexedApp {
        id: desktop_id.to_owned(),
        desktop_path: path.to_path_buf(),
        name: localized_name,
        name_lower,
        english_name,
        english_name_lower,
        name_acronym,
        english_acronym,
        name_words,
        english_words,
        name_pinyin,
        english_pinyin,
        generic_name,
        generic_name_lower,
        generic_acronym,
        generic_words,
        comment,
        comment_lower,
        description,
        keywords,
        icon,
        source,
        terminal,
        startup_wm_class,
        exec_raw,
        try_exec,
        exec_basename_lower,
        desktop_id_stem_lower,
        actions,
    })
}

fn extract_icon(info: &DesktopAppInfo, raw_icon: Option<&str>) -> Option<String> {
    if let Some(icon) = info.icon() {
        if let Some(file_icon) = icon.downcast_ref::<gio::FileIcon>()
            && let Some(path) = file_icon.file().path()
            && path.is_file()
        {
            return Some(path.to_string_lossy().into_owned());
        }
        if let Some(themed) = icon.downcast_ref::<gio::ThemedIcon>()
            && let Some(first) = themed.names().first()
        {
            let name = first.to_string();
            if !name.is_empty() {
                if name.starts_with('/') {
                    return Path::new(&name).is_file().then_some(name);
                }
                return Some(name);
            }
        }
    }
    let raw = raw_icon?.trim();
    if raw.is_empty() {
        return None;
    }
    if raw.starts_with('/') {
        Path::new(raw).is_file().then(|| raw.to_owned())
    } else {
        Some(raw.to_owned())
    }
}

fn detect_source(
    is_flatpak_root: bool,
    has_x_flatpak: bool,
    has_x_appimage: bool,
    exec_raw: &str,
    try_exec: Option<&str>,
) -> Option<String> {
    if is_flatpak_root || has_x_flatpak || extract_exec_basename(exec_raw) == "flatpak" {
        return Some("Flatpak".into());
    }
    let has_appimage_ext = |s: &str| s.to_ascii_lowercase().contains(".appimage");
    if has_x_appimage || has_appimage_ext(exec_raw) || try_exec.is_some_and(has_appimage_ext) {
        return Some("AppImage".into());
    }
    None
}

pub fn extract_exec_basename(exec_raw: &str) -> String {
    let tokens = tokenize_exec_raw(exec_raw);
    for token in tokens {
        let trimmed = token.trim();
        if trimmed.is_empty() || trimmed == "env" || trimmed.contains('=') {
            continue;
        }
        return Path::new(trimmed)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(trimmed)
            .to_owned();
    }
    String::new()
}

pub fn tokenize_exec_raw(exec: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut has_token = false;
    let mut chars = exec.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' => in_quotes = false,
                '\\' => {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                other => current.push(other),
            }
        } else {
            match ch {
                '"' => {
                    in_quotes = true;
                    has_token = true;
                }
                c if c.is_whitespace() => {
                    if has_token {
                        tokens.push(std::mem::take(&mut current));
                        has_token = false;
                    }
                }
                '\\' => {
                    has_token = true;
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                }
                other => {
                    has_token = true;
                    current.push(other);
                }
            }
        }
    }
    if has_token {
        tokens.push(current);
    }
    tokens
}

pub fn extract_words_and_acronym(text: &str) -> (String, Vec<String>) {
    let chars: Vec<char> = text.chars().collect();
    let mut words = Vec::new();
    let mut acronym = String::new();
    let mut current = String::new();

    for (i, &ch) in chars.iter().enumerate() {
        if !ch.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }

        let prev = i.checked_sub(1).map(|idx| chars[idx]);
        let next = chars.get(i + 1).copied();
        let is_camel_boundary = prev.is_some_and(|p| {
            (p.is_lowercase() && ch.is_uppercase())
                || (p.is_uppercase() && ch.is_uppercase() && next.is_some_and(char::is_lowercase))
        });

        if is_camel_boundary && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }

        if current.is_empty() {
            for lc in ch.to_lowercase() {
                acronym.push(lc);
            }
        }
        for lc in ch.to_lowercase() {
            current.push(lc);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    (acronym, words)
}

pub fn build_pinyin_data(text: &str) -> PinyinData {
    let mut has_hanzi = false;
    let mut units = Vec::new();
    for ch in text.chars() {
        if let Some(multi) = ch.to_pinyin_multi() {
            has_hanzi = true;
            let mut readings = Vec::new();
            for item in multi {
                let plain = item.plain();
                if !plain.is_empty() && !readings.contains(&plain) {
                    readings.push(plain);
                }
            }
            if !readings.is_empty() {
                units.push(PinyinUnit::Hanzi(readings));
            }
        } else if ch.is_ascii_alphanumeric() {
            units.push(PinyinUnit::Ascii(ch.to_ascii_lowercase()));
        }
    }
    if !has_hanzi {
        return PinyinData::default();
    }
    PinyinData { units }
}

impl IndexedApp {
    pub fn desktop_actions(&self) -> Vec<AppAction> {
        self.actions
            .iter()
            .map(|a| AppAction {
                id: a.id.clone(),
                name: a.name.clone(),
            })
            .collect()
    }
}

pub fn resolve_installed_aur_desktops(official_names: &HashSet<String>) -> HashSet<PathBuf> {
    let mut aur_paths = HashSet::new();
    let local_dir = env::var_os("REMYA_APPS_PACMAN_LOCAL_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            let data_dirs = env::var_os("XDG_DATA_DIRS").filter(|v| !v.is_empty());
            let has_sys = data_dirs
                .as_ref()
                .is_none_or(|v| env::split_paths(v).any(|p| p == Path::new("/usr/share")));
            has_sys.then(|| PathBuf::from("/var/lib/pacman/local"))
        });
    let Some(local_dir) = local_dir else {
        return aur_paths;
    };
    let Ok(entries) = fs::read_dir(local_dir) else {
        return aur_paths;
    };
    for entry in entries.flatten() {
        let dir_name = entry.file_name();
        let Some(name_str) = dir_name.to_str() else {
            continue;
        };
        let Some(pkg_name) = pacman_dir_to_pkgname(name_str) else {
            continue;
        };
        if official_names.contains(pkg_name) {
            continue;
        }
        let files_path = entry.path().join("files");
        let Ok(content) = fs::read_to_string(files_path) else {
            continue;
        };
        for line in content.lines() {
            if line.ends_with(".desktop") && line.contains("applications/") {
                aur_paths.insert(PathBuf::from("/").join(line));
            }
        }
    }
    aur_paths
}

fn pacman_dir_to_pkgname(dir_name: &str) -> Option<&str> {
    // Format: <pkgname>-<pkgver>-<pkgrel>
    let (rest, _pkgrel) = dir_name.rsplit_once('-')?;
    let (pkgname, _pkgver) = rest.rsplit_once('-')?;
    (!pkgname.is_empty()).then_some(pkgname)
}
