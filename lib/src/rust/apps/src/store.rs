use std::collections::{BTreeSet, HashMap};
use std::env;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rustix::fs::OFlags;
use rustix::process::geteuid;
use serde::{Deserialize, Serialize};

use crate::protocol::{Preferences, PreferencesPatch};

const MAX_USAGE_RETENTION_SECS: i64 = 365 * 86_400;
const MAX_EVENTS_PER_APP: usize = 256;
const MAX_QUERY_WINNERS: usize = 512;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryWinner {
    pub query: String,
    pub app_id: String,
    pub last_used_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedState {
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
    #[serde(default)]
    pub usage: HashMap<String, Vec<i64>>,
    #[serde(default)]
    pub query_winners: Vec<QueryWinner>,
}

pub struct Store {
    config_path: PathBuf,
    state_path: PathBuf,
    preferences: Preferences,
    state: PersistedState,
    config_mtime: Option<SystemTime>,
    state_mtime: Option<SystemTime>,
}

impl Store {
    pub fn open() -> io::Result<Self> {
        let config_path = config_file_path()?;
        let state_path = state_file_path()?;
        let (preferences, config_mtime) = read_json_or_default::<Preferences>(&config_path)?;
        let (state, state_mtime) = read_json_or_default::<PersistedState>(&state_path)?;
        Ok(Self {
            config_path,
            state_path,
            preferences,
            state,
            config_mtime,
            state_mtime,
        })
    }

    pub fn refresh_if_changed(&mut self) {
        if let Ok(mtime) = file_mtime(&self.config_path)
            && mtime != self.config_mtime
            && let Ok((prefs, new_mtime)) = read_json_or_default::<Preferences>(&self.config_path)
        {
            self.preferences = prefs;
            self.config_mtime = new_mtime;
        }
        if let Ok(mtime) = file_mtime(&self.state_path)
            && mtime != self.state_mtime
            && let Ok((state, new_mtime)) = read_json_or_default::<PersistedState>(&self.state_path)
        {
            self.state = state;
            self.state_mtime = new_mtime;
        }
    }

    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    pub fn update_preferences(&mut self, patch: PreferencesPatch) -> Result<Preferences, String> {
        self.refresh_if_changed();
        let mut next = self.preferences.clone();
        if let Some(always_new) = patch.always_new {
            next.always_new = always_new;
        }
        if let Some(frequent_days) = patch.frequent_days {
            if !(1..=365).contains(&frequent_days) {
                return Err("frequentDays must be between 1 and 365.".into());
            }
            next.frequent_days = frequent_days as u32;
        }
        if let Some(pinyin) = patch.pinyin {
            next.pinyin = pinyin;
        }
        if let Some(terminal) = patch.terminal {
            if terminal.contains(['\0', '\n', '\r']) {
                return Err("terminal contains invalid control characters.".into());
            }
            next.terminal = terminal.trim().to_owned();
        }
        if let Some(show_packages) = patch.show_packages {
            next.show_packages = show_packages;
        }
        if let Some(official) = patch.official {
            next.official = official;
        }
        if let Some(flatpak) = patch.flatpak {
            next.flatpak = flatpak;
        }
        if let Some(aur) = patch.aur {
            next.aur = aur;
        }

        write_json_atomic(&self.config_path, &next)
            .map_err(|e| format!("Failed to save preferences: {e}"))?;
        self.config_mtime = file_mtime(&self.config_path).ok().flatten();
        self.preferences = next.clone();
        Ok(next)
    }

    pub fn is_pinned(&self, app_id: &str) -> bool {
        self.state.pinned.iter().any(|id| id == app_id)
    }

    pub fn pinned_ids(&self) -> &[String] {
        &self.state.pinned
    }

    pub fn is_hidden(&self, app_id: &str) -> bool {
        self.state.hidden.iter().any(|id| id == app_id)
    }

    pub fn hidden_ids(&self) -> BTreeSet<String> {
        self.state.hidden.iter().cloned().collect()
    }

    pub fn toggle_pin(&mut self, app_id: &str) -> Result<bool, String> {
        self.refresh_if_changed();
        let now_pinned = if let Some(pos) = self.state.pinned.iter().position(|id| id == app_id) {
            self.state.pinned.remove(pos);
            false
        } else {
            self.state.pinned.push(app_id.to_owned());
            true
        };
        self.save_state()?;
        Ok(now_pinned)
    }

    pub fn set_hidden(&mut self, app_id: &str, hidden: bool) -> Result<(), String> {
        self.refresh_if_changed();
        if hidden {
            if !self.state.hidden.iter().any(|id| id == app_id) {
                self.state.hidden.push(app_id.to_owned());
            }
        } else {
            self.state.hidden.retain(|id| id != app_id);
        }
        self.save_state()
    }

    pub fn record_usage(&mut self, app_id: &str, query: &str, now_secs: i64) -> Result<(), String> {
        self.refresh_if_changed();
        let cutoff = now_secs - MAX_USAGE_RETENTION_SECS;
        for timestamps in self.state.usage.values_mut() {
            timestamps.retain(|&ts| ts >= cutoff);
        }
        self.state.usage.retain(|_, ts| !ts.is_empty());

        let events = self.state.usage.entry(app_id.to_owned()).or_default();
        events.push(now_secs);
        if events.len() > MAX_EVENTS_PER_APP {
            let drain = events.len() - MAX_EVENTS_PER_APP;
            events.drain(..drain);
        }

        let norm_query = query.trim().to_lowercase();
        if !norm_query.is_empty() {
            self.state
                .query_winners
                .retain(|w| !(w.query == norm_query && w.app_id == app_id));
            self.state.query_winners.push(QueryWinner {
                query: norm_query,
                app_id: app_id.to_owned(),
                last_used_at: now_secs,
            });
            if self.state.query_winners.len() > MAX_QUERY_WINNERS {
                let drain = self.state.query_winners.len() - MAX_QUERY_WINNERS;
                self.state.query_winners.drain(..drain);
            }
        }

        self.save_state()
    }

    pub fn frecency(&self, app_id: &str, now_secs: i64) -> f64 {
        let Some(events) = self.state.usage.get(app_id) else {
            return 0.0;
        };
        let window_secs = i64::from(self.preferences.frequent_days) * 86_400;
        let cutoff = now_secs - window_secs;
        let mut score = 0.0;
        for &ts in events {
            if ts < cutoff {
                continue;
            }
            let age_hours = ((now_secs - ts).max(0) as f64) / 3600.0;
            score += 4.0 / (1.0 + age_hours / 24.0);
        }
        score
    }

    pub fn last_used(&self, app_id: &str, now_secs: i64) -> i64 {
        let cutoff = now_secs - i64::from(self.preferences.frequent_days) * 86_400;
        self.state
            .usage
            .get(app_id)
            .and_then(|events| events.iter().copied().filter(|&ts| ts >= cutoff).max())
            .unwrap_or(0)
    }

    /// Returns recency rank (0 = most recently chosen for `norm_query`) if previously chosen.
    pub fn query_winner_rank(&self, norm_query: &str, app_id: &str) -> Option<usize> {
        if norm_query.is_empty() {
            return None;
        }
        self.state
            .query_winners
            .iter()
            .rev()
            .filter(|w| w.query == norm_query)
            .position(|w| w.app_id == app_id)
    }

    fn save_state(&mut self) -> Result<(), String> {
        write_json_atomic(&self.state_path, &self.state)
            .map_err(|e| format!("Failed to save app state: {e}"))?;
        self.state_mtime = file_mtime(&self.state_path).ok().flatten();
        Ok(())
    }
}

pub fn now_unix_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub fn absolute_env(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn config_file_path() -> io::Result<PathBuf> {
    let base = absolute_env("XDG_CONFIG_HOME")
        .or_else(|| absolute_env("HOME").map(|h| h.join(".config")))
        .ok_or(io::ErrorKind::NotFound)?;
    let dir = base.join("remya/launcher");
    ensure_private_dir(&dir)?;
    Ok(dir.join("apps.json"))
}

fn state_file_path() -> io::Result<PathBuf> {
    let base = absolute_env("XDG_STATE_HOME")
        .or_else(|| absolute_env("HOME").map(|h| h.join(".local/state")))
        .ok_or(io::ErrorKind::NotFound)?;
    let dir = base.join("remya/launcher");
    ensure_private_dir(&dir)?;
    Ok(dir.join("apps.json"))
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != geteuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "directory is not privately owned",
        ));
    }
    if metadata.mode() & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn file_mtime(path: &Path) -> io::Result<Option<SystemTime>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => Ok(meta.modified().ok()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn read_json_or_default<T: for<'de> Deserialize<'de> + Default>(
    path: &Path,
) -> io::Result<(T, Option<SystemTime>)> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.is_file() || meta.uid() != geteuid().as_raw() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "file is not privately owned",
                ));
            }
            if meta.mode() & 0o077 != 0 {
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
            }
            let bytes = fs::read(path)?;
            let parsed = serde_json::from_slice(&bytes).unwrap_or_default();
            Ok((parsed, meta.modified().ok()))
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok((T::default(), None)),
        Err(err) => Err(err),
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let parent = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    ensure_private_dir(parent)?;
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_name = format!(
        ".{}.tmp.{}.{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("apps.json"),
        std::process::id(),
        seq
    );
    let tmp_path = parent.join(tmp_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32)
        .open(&tmp_path)?;
    if let Err(err) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    drop(file);
    if let Err(err) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}
