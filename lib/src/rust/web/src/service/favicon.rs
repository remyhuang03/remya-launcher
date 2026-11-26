use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::browsers::{self, Profile};
use crate::index::{self, MAX_FAVICON_PNG_BYTES};

use super::WebService;
use super::protocol::{ApiError, MAX_ID_BYTES, validate_entity_id, validate_icon_key};

const FAVICON_RESOLVE_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_FAVICON_WORKERS: usize = 4;
const FAVICON_CACHE_MAX_BYTES_PER_PROFILE: usize = 4 * 1024 * 1024;
const FAVICON_CACHE_MAX_ENTRIES_PER_PROFILE: usize = 256;
const FAVICON_CACHE_MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const FAVICON_CACHE_MAX_TOTAL_ENTRIES: usize = 1024;
const PNG_MAGIC: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone)]
struct CachedFavicon {
    profile_id: String,
    normalized_url: String,
    consent_token: String,
    host: String,
    png_bytes: Vec<u8>,
    last_used_seq: u64,
}

pub(super) struct FaviconCache {
    entries: HashMap<String, CachedFavicon>,
    by_profile_url: HashMap<(String, String), String>,
    profile_bytes: HashMap<String, usize>,
    profile_counts: HashMap<String, usize>,
    total_bytes: usize,
    seq: u64,
}

impl FaviconCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            by_profile_url: HashMap::new(),
            profile_bytes: HashMap::new(),
            profile_counts: HashMap::new(),
            total_bytes: 0,
            seq: 0,
        }
    }

    fn get_key_for(
        &mut self,
        profile_id: &str,
        normalized_url: &str,
        consent_token: &str,
    ) -> Option<String> {
        let key = self
            .by_profile_url
            .get(&(profile_id.to_owned(), normalized_url.to_owned()))?
            .clone();
        self.seq = self.seq.wrapping_add(1);
        let entry = self.entries.get_mut(&key)?;
        if entry.consent_token != consent_token {
            return None;
        }
        entry.last_used_seq = self.seq;
        Some(key)
    }

    fn read_by_key(&mut self, icon_key: &str) -> Option<&CachedFavicon> {
        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;
        let entry = self.entries.get_mut(icon_key)?;
        entry.last_used_seq = seq;
        Some(entry)
    }

    fn insert(
        &mut self,
        profile_id: &str,
        normalized_url: &str,
        host: &str,
        consent_token: &str,
        png_bytes: Vec<u8>,
    ) -> String {
        if let Some(old_key) = self
            .by_profile_url
            .remove(&(profile_id.to_owned(), normalized_url.to_owned()))
        {
            self.remove_by_key(&old_key);
        }

        self.seq = self.seq.wrapping_add(1);
        let seq = self.seq;

        while self.profile_counts.get(profile_id).copied().unwrap_or(0)
            >= FAVICON_CACHE_MAX_ENTRIES_PER_PROFILE
            || self.profile_bytes.get(profile_id).copied().unwrap_or(0) + png_bytes.len()
                > FAVICON_CACHE_MAX_BYTES_PER_PROFILE
        {
            if !self.evict_lru_for_profile(profile_id) {
                break;
            }
        }

        while self.entries.len() >= FAVICON_CACHE_MAX_TOTAL_ENTRIES
            || self.total_bytes + png_bytes.len() > FAVICON_CACHE_MAX_TOTAL_BYTES
        {
            if !self.evict_lru_global() {
                break;
            }
        }

        let mut hasher = Sha256::new();
        hasher.update(profile_id.as_bytes());
        hasher.update(b"\0");
        hasher.update(normalized_url.as_bytes());
        hasher.update(b"\0");
        hasher.update(consent_token.as_bytes());
        hasher.update(seq.to_le_bytes());
        let digest = hasher.finalize();
        let mut icon_key = String::with_capacity(3 + 32);
        icon_key.push_str("ic_");
        for byte in &digest[..16] {
            icon_key.push_str(&format!("{byte:02x}"));
        }

        let size = png_bytes.len();
        self.total_bytes += size;
        *self.profile_bytes.entry(profile_id.to_owned()).or_insert(0) += size;
        *self
            .profile_counts
            .entry(profile_id.to_owned())
            .or_insert(0) += 1;

        self.by_profile_url.insert(
            (profile_id.to_owned(), normalized_url.to_owned()),
            icon_key.clone(),
        );
        self.entries.insert(
            icon_key.clone(),
            CachedFavicon {
                profile_id: profile_id.to_owned(),
                normalized_url: normalized_url.to_owned(),
                consent_token: consent_token.to_owned(),
                host: host.to_owned(),
                png_bytes,
                last_used_seq: seq,
            },
        );
        icon_key
    }

    pub(super) fn remove_by_key(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.by_profile_url
                .remove(&(entry.profile_id.clone(), entry.normalized_url));
            self.total_bytes = self.total_bytes.saturating_sub(entry.png_bytes.len());
            if let Some(b) = self.profile_bytes.get_mut(&entry.profile_id) {
                *b = b.saturating_sub(entry.png_bytes.len());
                if *b == 0 {
                    self.profile_bytes.remove(&entry.profile_id);
                }
            }
            if let Some(c) = self.profile_counts.get_mut(&entry.profile_id) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    self.profile_counts.remove(&entry.profile_id);
                }
            }
        }
    }

    fn evict_lru_for_profile(&mut self, profile_id: &str) -> bool {
        let target = self
            .entries
            .iter()
            .filter(|(_, e)| e.profile_id == profile_id)
            .min_by_key(|(_, e)| e.last_used_seq)
            .map(|(k, _)| k.clone());
        if let Some(key) = target {
            self.remove_by_key(&key);
            true
        } else {
            false
        }
    }

    fn evict_lru_global(&mut self) -> bool {
        let target = self
            .entries
            .iter()
            .min_by_key(|(_, e)| e.last_used_seq)
            .map(|(k, _)| k.clone());
        if let Some(key) = target {
            self.remove_by_key(&key);
            true
        } else {
            false
        }
    }

    pub(super) fn invalidate_profile(&mut self, profile_id: &str) {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.profile_id == profile_id)
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            self.remove_by_key(&key);
        }
    }

    pub(super) fn invalidate_url(&mut self, normalized_url: &str) {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.normalized_url == normalized_url)
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            self.remove_by_key(&key);
        }
    }

    pub(super) fn invalidate_host(&mut self, host: &str) {
        let keys: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| e.host.eq_ignore_ascii_case(host))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            self.remove_by_key(&key);
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.by_profile_url.clear();
        self.profile_bytes.clear();
        self.profile_counts.clear();
        self.total_bytes = 0;
    }
}

pub(super) struct FaviconManager {
    pub(super) cache: Mutex<FaviconCache>,
    active_workers: Arc<AtomicUsize>,
}

impl FaviconManager {
    pub(super) fn new() -> Self {
        Self {
            cache: Mutex::new(FaviconCache::new()),
            active_workers: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn fetch_bounded(
        &self,
        profile: Profile,
        url_str: String,
        timeout: Duration,
    ) -> Option<Vec<u8>> {
        if timeout.is_zero() {
            return None;
        }
        let prev = self.active_workers.fetch_add(1, Ordering::AcqRel);
        if prev >= MAX_FAVICON_WORKERS {
            self.active_workers.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        let active = Arc::clone(&self.active_workers);
        let (tx, rx) = mpsc::sync_channel(1);
        let spawn_res = thread::Builder::new()
            .name("web-favicon".into())
            .spawn(move || {
                let bytes = browsers::favicon(&profile, &url_str);
                let _ = tx.send(bytes);
                active.fetch_sub(1, Ordering::AcqRel);
            });
        if spawn_res.is_err() {
            self.active_workers.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        rx.recv_timeout(timeout).ok().flatten()
    }
}

impl WebService {
    fn is_favicon_candidate_eligible(
        &self,
        profile_id: &str,
        norm_url: &str,
        opt_gen: u64,
        clear_gen: u64,
        consent_token: &str,
    ) -> bool {
        let still_allowed = {
            let state = self.shared.state.lock().unwrap();
            state.clear_gen == clear_gen
                && state
                    .known_profiles
                    .get(profile_id)
                    .is_some_and(|r| r.present_on_disk && r.opt_in_gen == opt_gen)
        };
        if !still_allowed {
            return false;
        }
        let idx = self.index.lock().unwrap();
        idx.visible_url_token(profile_id, norm_url)
            .ok()
            .flatten()
            .as_deref()
            == Some(consent_token)
    }

    pub(super) fn favicon_resolve(
        &self,
        raw_url: &str,
        profile_id: Option<&str>,
    ) -> Result<Value, ApiError> {
        let deadline = Instant::now() + FAVICON_RESOLVE_TIMEOUT;
        let parsed = index::validate_url(raw_url)
            .ok_or_else(|| ApiError::invalid_url("Invalid or disallowed URL."))?;
        let fallback_text = monogram_for_url(&parsed);
        let norm_url = parsed.as_str().to_owned();
        let host = parsed.host_str().unwrap_or_default().to_owned();

        let candidate_profiles: Vec<(Profile, u64, u64, String)> = if let Some(pid) = profile_id {
            validate_entity_id(pid, MAX_ID_BYTES, "profileId")?;
            let (profile, present_on_disk, opt_in_gen, clear_gen) = {
                let state = self.shared.state.lock().unwrap();
                let rec = state
                    .known_profiles
                    .get(pid)
                    .ok_or_else(ApiError::profile_not_found)?;
                (
                    rec.profile.clone(),
                    rec.present_on_disk,
                    rec.opt_in_gen,
                    state.clear_gen,
                )
            };
            let token = {
                let idx = self.index.lock().unwrap();
                idx.consent_token(pid)
                    .map_err(|_| ApiError::internal("Failed to check profile opt-in state."))?
            };
            let Some(token) = token else {
                let mut cache = self.favicons.cache.lock().unwrap();
                cache.invalidate_profile(pid);
                return Err(ApiError::profile_disabled());
            };
            if !present_on_disk {
                return Ok(json!({
                    "status": "monogram",
                    "fallbackText": fallback_text,
                }));
            }
            vec![(profile, opt_in_gen, clear_gen, token)]
        } else {
            let enabled_ids = {
                let idx = self.index.lock().unwrap();
                idx.enabled_ids()
                    .map_err(|_| ApiError::internal("Failed to read enabled profiles."))?
            };
            let raw_candidates: Vec<(Profile, u64, u64)> = {
                let state = self.shared.state.lock().unwrap();
                let mut list: Vec<(Profile, u64, u64)> = state
                    .known_profiles
                    .values()
                    .filter(|r| r.present_on_disk && enabled_ids.contains(&r.profile.id))
                    .map(|r| (r.profile.clone(), r.opt_in_gen, state.clear_gen))
                    .collect();
                list.sort_by(|a, b| a.0.id.cmp(&b.0.id));
                list
            };
            let idx = self.index.lock().unwrap();
            let mut list = Vec::with_capacity(raw_candidates.len());
            for (profile, opt_in_gen, clear_gen) in raw_candidates {
                if let Some(token) = idx
                    .consent_token(&profile.id)
                    .map_err(|_| ApiError::internal("Failed to check profile opt-in state."))?
                {
                    list.push((profile, opt_in_gen, clear_gen, token));
                }
            }
            list
        };

        // Only enabled visible URLs already in the index may trigger favicon access.
        let mut authorized_profiles = Vec::new();
        {
            let idx = self.index.lock().unwrap();
            for item in candidate_profiles {
                let token = idx
                    .visible_url_token(&item.0.id, &norm_url)
                    .map_err(|_| ApiError::internal("Failed to check index visibility."))?;
                if token.as_deref() == Some(item.3.as_str()) {
                    authorized_profiles.push(item);
                }
            }
        }

        if authorized_profiles.is_empty() {
            return Ok(json!({
                "status": "monogram",
                "fallbackText": fallback_text,
            }));
        }

        // Check per-profile cache first, and re-validate eligibility before returning a cached key.
        for (profile, opt_gen, clear_gen, token) in &authorized_profiles {
            let cached_key = {
                let mut cache = self.favicons.cache.lock().unwrap();
                cache.get_key_for(&profile.id, &norm_url, token)
            };
            if let Some(icon_key) = cached_key {
                if self.is_favicon_candidate_eligible(
                    &profile.id,
                    &norm_url,
                    *opt_gen,
                    *clear_gen,
                    token,
                ) {
                    return Ok(json!({
                        "status": "ready",
                        "iconKey": icon_key,
                        "fallbackText": fallback_text,
                    }));
                }
                let mut cache = self.favicons.cache.lock().unwrap();
                cache.remove_by_key(&icon_key);
            }
        }

        // Browser I/O happens outside the index and mutation locks.
        for (profile, opt_gen, clear_gen, token) in authorized_profiles {
            let stored = self
                .index
                .lock()
                .unwrap()
                .stored_favicon(&profile.id, &norm_url, &token)
                .ok()
                .flatten()
                .filter(|icon| icon.png.starts_with(PNG_MAGIC));
            let refreshed = if stored.as_ref().is_none_or(|icon| icon.needs_refresh) {
                self.favicons
                    .fetch_bounded(
                        profile.clone(),
                        norm_url.clone(),
                        deadline.saturating_duration_since(Instant::now()),
                    )
                    .filter(|png| png.len() <= MAX_FAVICON_PNG_BYTES && png.starts_with(PNG_MAGIC))
            } else {
                None
            };
            let from_browser = refreshed.is_some();
            let Some(png) = refreshed.or_else(|| stored.map(|icon| icon.png)) else {
                continue;
            };
            let _mutation = self.shared.mutation_lock.lock().unwrap();
            if !self.is_favicon_candidate_eligible(
                &profile.id,
                &norm_url,
                opt_gen,
                clear_gen,
                &token,
            ) {
                continue;
            }
            if from_browser {
                let _ =
                    self.index
                        .lock()
                        .unwrap()
                        .store_favicon(&profile.id, &parsed, &token, &png);
            }
            let mut cache = self.favicons.cache.lock().unwrap();
            let icon_key = cache.insert(&profile.id, &norm_url, &host, &token, png);
            return Ok(json!({
                "status": "ready",
                "iconKey": icon_key,
                "fallbackText": fallback_text,
            }));
        }

        Ok(json!({
            "status": "monogram",
            "fallbackText": fallback_text,
        }))
    }

    pub(super) fn favicon_read(&self, icon_key: &str) -> Result<Value, ApiError> {
        validate_icon_key(icon_key)?;

        let cached = {
            let mut cache = self.favicons.cache.lock().unwrap();
            cache.read_by_key(icon_key).cloned()
        };
        let Some(cached) = cached else {
            return Err(ApiError::icon_not_found());
        };

        // Always verify against the shared SQLite projection (enabled, indexed, and unhidden),
        // even when our in-memory cache and map state still look enabled.
        let visible_token = {
            let idx = self.index.lock().unwrap();
            idx.visible_url_token(&cached.profile_id, &cached.normalized_url)
                .map_err(|_| ApiError::internal("Failed to check favicon visibility."))?
        };
        if visible_token.as_deref() != Some(cached.consent_token.as_str()) {
            let mut cache = self.favicons.cache.lock().unwrap();
            cache.remove_by_key(icon_key);
            return Err(ApiError::icon_not_found());
        }

        let png_base64 = BASE64_STANDARD.encode(&cached.png_bytes);
        Ok(json!({ "pngBase64": png_base64 }))
    }
}

fn monogram_for_url(url: &url::Url) -> String {
    let host = url.host_str().unwrap_or_default();
    let trimmed = host.strip_prefix("www.").unwrap_or(host);
    for ch in trimmed.chars() {
        if ch.is_alphanumeric() {
            return ch.to_uppercase().collect();
        }
    }
    "W".to_owned()
}
