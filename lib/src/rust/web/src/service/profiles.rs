use std::collections::{HashMap, HashSet};
use std::sync::{Condvar, Mutex};

use serde_json::{Value, json};

use crate::browsers::{self, Profile};

use super::WebService;
use super::protocol::{ApiError, MAX_ID_BYTES, MAX_PROFILE_FILTER_LEN, validate_entity_id};
use super::state::save_profiles_meta;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProfileSyncStatus {
    Disabled,
    Idle,
    Syncing,
    Ready,
    Stale,
    Error,
}

impl ProfileSyncStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Idle => "idle",
            Self::Syncing => "syncing",
            Self::Ready => "ready",
            Self::Stale => "stale",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ProfileRecord {
    pub(super) profile: Profile,
    pub(super) enabled: bool,
    pub(super) status: ProfileSyncStatus,
    pub(super) last_good_sync_ms: Option<i64>,
    pub(super) error_code: Option<&'static str>,
    pub(super) present_on_disk: bool,
    pub(super) opt_in_gen: u64,
}

#[derive(Debug, Default)]
pub(super) struct SyncQueue {
    pub(super) pending_all: bool,
    pub(super) pending_profiles: HashSet<String>,
    pub(super) running: bool,
    pub(super) running_all: bool,
    pub(super) running_profiles: HashSet<String>,
    pub(super) next_job_seq: u64,
}

pub(super) struct CoordinatorState {
    pub(super) known_profiles: HashMap<String, ProfileRecord>,
    pub(super) clear_gen: u64,
    pub(super) queue: SyncQueue,
    pub(super) shutdown: bool,
}

pub(super) struct SharedCoordinator {
    pub(super) mutation_lock: Mutex<()>,
    pub(super) state: Mutex<CoordinatorState>,
    pub(super) wakeup: Condvar,
}

impl SharedCoordinator {
    pub(super) fn is_scan_active_for(&self, filter: Option<&[String]>) -> bool {
        let state = self.state.lock().unwrap();
        let any_enabled = state.known_profiles.values().any(|r| r.enabled);
        if !any_enabled {
            return false;
        }
        let q = &state.queue;
        match filter {
            None => {
                q.pending_all
                    || q.running_all
                    || q.pending_profiles
                        .iter()
                        .any(|id| state.known_profiles.get(id).is_some_and(|r| r.enabled))
                    || q.running_profiles
                        .iter()
                        .any(|id| state.known_profiles.get(id).is_some_and(|r| r.enabled))
            }
            Some(ids) => {
                if q.pending_all || q.running_all {
                    return ids
                        .iter()
                        .any(|id| state.known_profiles.get(id).is_some_and(|r| r.enabled));
                }
                ids.iter().any(|id| {
                    state.known_profiles.get(id).is_some_and(|r| r.enabled)
                        && (q.pending_profiles.contains(id) || q.running_profiles.contains(id))
                })
            }
        }
    }

    pub(super) fn is_any_syncing(&self) -> bool {
        let state = self.state.lock().unwrap();
        let any_enabled = state.known_profiles.values().any(|r| r.enabled);
        if !any_enabled {
            return false;
        }
        state.queue.running || state.queue.pending_all || !state.queue.pending_profiles.is_empty()
    }
}

impl WebService {
    fn refresh_discovered_profiles(&self) -> Result<Vec<ProfileRecord>, ApiError> {
        let discovered = browsers::discover();
        let enabled_ids = {
            let idx = self.index.lock().unwrap();
            idx.enabled_ids()
                .map_err(|_| ApiError::internal("Failed to read enabled profiles."))?
        };
        let (records, newly_disabled) = {
            let mut state = self.shared.state.lock().unwrap();
            let newly_disabled =
                reconcile_discovered_locked(&mut state.known_profiles, discovered, &enabled_ids);
            let _ = save_profiles_meta(&self.state_dir, &state.known_profiles);
            (
                sorted_profile_records(&state.known_profiles),
                newly_disabled,
            )
        };
        if !newly_disabled.is_empty() {
            let mut cache = self.favicons.cache.lock().unwrap();
            for pid in newly_disabled {
                cache.invalidate_profile(&pid);
            }
        }
        Ok(records)
    }

    pub(super) fn profiles_list(&self) -> Result<Value, ApiError> {
        let records = self.refresh_discovered_profiles()?;
        let profiles: Vec<Value> = records.iter().map(profile_to_json).collect();
        Ok(Value::Array(profiles))
    }

    pub(super) fn sync_status(&self) -> Result<Value, ApiError> {
        let records = self.refresh_discovered_profiles()?;
        let syncing = self.shared.is_any_syncing();
        let profiles: Vec<Value> = records.iter().map(profile_to_json).collect();
        Ok(json!({
            "syncing": syncing,
            "profiles": profiles,
        }))
    }

    pub(super) fn profiles_set_enabled(
        &self,
        profile_id: &str,
        enabled: bool,
    ) -> Result<Value, ApiError> {
        validate_entity_id(profile_id, MAX_ID_BYTES, "profileId")?;

        let _mut_guard = self.shared.mutation_lock.lock().unwrap();

        // Ensure discovery is fresh when enabling so newly-created profiles are found.
        if enabled {
            let discovered = browsers::discover();
            let enabled_ids = {
                let idx = self.index.lock().unwrap();
                idx.enabled_ids()
                    .map_err(|_| ApiError::internal("Failed to read enabled profiles."))?
            };
            let mut state = self.shared.state.lock().unwrap();
            reconcile_discovered_locked(&mut state.known_profiles, discovered, &enabled_ids);
        }

        let exists = {
            let state = self.shared.state.lock().unwrap();
            state.known_profiles.contains_key(profile_id)
        };
        if !exists {
            return Err(ApiError::profile_not_found());
        }

        {
            let mut idx = self.index.lock().unwrap();
            idx.set_enabled(profile_id, enabled)
                .map_err(|_| ApiError::internal("Failed to update profile state."))?;
        }

        let status_str = {
            let mut state = self.shared.state.lock().unwrap();
            let can_scan = if let Some(rec) = state.known_profiles.get_mut(profile_id) {
                rec.opt_in_gen = rec.opt_in_gen.wrapping_add(1);
                rec.enabled = enabled;
                if enabled {
                    rec.error_code = None;
                    if rec.present_on_disk {
                        rec.status = ProfileSyncStatus::Syncing;
                        true
                    } else {
                        rec.status = ProfileSyncStatus::Error;
                        rec.error_code = Some("PROFILE_MISSING");
                        false
                    }
                } else {
                    rec.status = ProfileSyncStatus::Disabled;
                    rec.last_good_sync_ms = None;
                    rec.error_code = None;
                    false
                }
            } else {
                false
            };

            if enabled && can_scan {
                state.queue.pending_profiles.insert(profile_id.to_owned());
                self.shared.wakeup.notify_all();
            } else if !enabled {
                state.queue.pending_profiles.remove(profile_id);
                // Clean up missing disabled profile metadata.
                if state
                    .known_profiles
                    .get(profile_id)
                    .is_some_and(|r| !r.present_on_disk)
                {
                    state.known_profiles.remove(profile_id);
                }
            }
            let _ = save_profiles_meta(&self.state_dir, &state.known_profiles);

            state
                .known_profiles
                .get(profile_id)
                .map_or("disabled", |r| r.status.as_str())
        };

        if !enabled {
            let mut cache = self.favicons.cache.lock().unwrap();
            cache.invalidate_profile(profile_id);
        }

        Ok(json!({
            "profileId": profile_id,
            "enabled": enabled,
            "status": status_str,
        }))
    }

    pub(super) fn sync_request(
        &self,
        profile_ids: Option<Vec<String>>,
        mode: Option<&str>,
    ) -> Result<Value, ApiError> {
        let mode = mode.unwrap_or("incremental");
        if !matches!(mode, "incremental" | "full") {
            return Err(ApiError::invalid_request("Unsupported sync mode."));
        }

        let validated_ids = match profile_ids {
            Some(ids) => {
                if ids.is_empty() || ids.len() > MAX_PROFILE_FILTER_LEN {
                    return Err(ApiError::invalid_request(
                        "profileIds must contain between 1 and 64 items.",
                    ));
                }
                for id in &ids {
                    validate_entity_id(id, MAX_ID_BYTES, "profileId")?;
                }
                Some(ids)
            }
            None => None,
        };

        let _ = self.refresh_discovered_profiles()?;

        let mut state = self.shared.state.lock().unwrap();
        if let Some(ref ids) = validated_ids {
            for id in ids {
                let rec = state
                    .known_profiles
                    .get(id)
                    .ok_or_else(ApiError::profile_not_found)?;
                if !rec.enabled {
                    return Err(ApiError::profile_disabled());
                }
            }
            for id in ids {
                state.queue.pending_profiles.insert(id.clone());
                if let Some(rec) = state.known_profiles.get_mut(id)
                    && rec.present_on_disk
                {
                    rec.status = ProfileSyncStatus::Syncing;
                }
            }
        } else {
            state.queue.pending_all = true;
            for rec in state.known_profiles.values_mut() {
                if rec.enabled && rec.present_on_disk {
                    rec.status = ProfileSyncStatus::Syncing;
                }
            }
        }

        let seq = state.queue.next_job_seq;
        state.queue.next_job_seq = state.queue.next_job_seq.wrapping_add(1);
        self.shared.wakeup.notify_all();

        Ok(json!({
            "accepted": true,
            "jobId": format!("sync-{seq}"),
        }))
    }

    pub(super) fn require_enabled_profile(&self, profile_id: &str) -> Result<Profile, ApiError> {
        let (profile, present_on_disk) = {
            let state = self.shared.state.lock().unwrap();
            let rec = state
                .known_profiles
                .get(profile_id)
                .ok_or_else(ApiError::profile_not_found)?;
            (rec.profile.clone(), rec.present_on_disk)
        };
        let enabled_in_db = {
            let idx = self.index.lock().unwrap();
            idx.consent_token(profile_id)
                .map_err(|_| ApiError::internal("Failed to check profile opt-in state."))?
                .is_some()
        };
        if !enabled_in_db {
            {
                let mut state = self.shared.state.lock().unwrap();
                if let Some(rec) = state.known_profiles.get_mut(profile_id) {
                    rec.enabled = false;
                    rec.status = ProfileSyncStatus::Disabled;
                    rec.last_good_sync_ms = None;
                    rec.error_code = None;
                }
            }
            let mut cache = self.favicons.cache.lock().unwrap();
            cache.invalidate_profile(profile_id);
            return Err(ApiError::profile_disabled());
        }
        if !present_on_disk {
            return Err(ApiError::profile_permission_denied());
        }
        Ok(profile)
    }
}

pub(super) fn reconcile_discovered_locked(
    known: &mut HashMap<String, ProfileRecord>,
    discovered: Vec<Profile>,
    enabled_ids: &HashSet<String>,
) -> Vec<String> {
    let mut newly_disabled = Vec::new();
    let mut seen = HashSet::with_capacity(discovered.len());
    for profile in discovered {
        seen.insert(profile.id.clone());
        let is_enabled = enabled_ids.contains(&profile.id);
        match known.get_mut(&profile.id) {
            Some(rec) => {
                let was_enabled = rec.enabled;
                rec.profile = profile;
                rec.present_on_disk = true;
                rec.enabled = is_enabled;
                if !is_enabled {
                    if was_enabled {
                        newly_disabled.push(rec.profile.id.clone());
                    }
                    rec.status = ProfileSyncStatus::Disabled;
                    rec.last_good_sync_ms = None;
                    rec.error_code = None;
                } else if !was_enabled {
                    rec.error_code = None;
                    rec.last_good_sync_ms = None;
                    rec.status = ProfileSyncStatus::Idle;
                } else if rec.error_code == Some("PROFILE_MISSING") {
                    rec.error_code = None;
                    rec.status = if rec.last_good_sync_ms.is_some() {
                        ProfileSyncStatus::Stale
                    } else {
                        ProfileSyncStatus::Idle
                    };
                }
            }
            None => {
                known.insert(
                    profile.id.clone(),
                    ProfileRecord {
                        profile,
                        enabled: is_enabled,
                        status: if is_enabled {
                            ProfileSyncStatus::Idle
                        } else {
                            ProfileSyncStatus::Disabled
                        },
                        last_good_sync_ms: None,
                        error_code: None,
                        present_on_disk: true,
                        opt_in_gen: 0,
                    },
                );
            }
        }
    }

    let missing_ids: Vec<String> = known
        .keys()
        .filter(|id| !seen.contains(*id))
        .cloned()
        .collect();
    for id in missing_ids {
        let is_enabled = enabled_ids.contains(&id);
        if is_enabled {
            if let Some(rec) = known.get_mut(&id) {
                rec.enabled = true;
                rec.present_on_disk = false;
                rec.status = if rec.last_good_sync_ms.is_some() {
                    ProfileSyncStatus::Stale
                } else {
                    ProfileSyncStatus::Error
                };
                rec.error_code = Some("PROFILE_MISSING");
            }
        } else {
            newly_disabled.push(id.clone());
            known.remove(&id);
        }
    }
    newly_disabled
}

fn sorted_profile_records(known: &HashMap<String, ProfileRecord>) -> Vec<ProfileRecord> {
    let mut items: Vec<ProfileRecord> = known.values().cloned().collect();
    items.sort_by(|a, b| {
        a.profile
            .browser_id
            .cmp(&b.profile.browser_id)
            .then_with(|| a.profile.label.cmp(&b.profile.label))
            .then_with(|| a.profile.id.cmp(&b.profile.id))
    });
    items
}

fn profile_to_json(rec: &ProfileRecord) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("profileId".into(), json!(rec.profile.id));
    obj.insert("browserId".into(), json!(rec.profile.browser_id));
    obj.insert("browserLabel".into(), json!(rec.profile.browser_label));
    obj.insert("label".into(), json!(rec.profile.label));
    obj.insert("enabled".into(), json!(rec.enabled));
    obj.insert("status".into(), json!(rec.status.as_str()));
    if let Some(ts) = rec.last_good_sync_ms {
        obj.insert("lastGoodSyncMs".into(), json!(ts));
    }
    if let Some(err) = rec.error_code {
        obj.insert("errorCode".into(), json!(err));
    }
    Value::Object(obj)
}

pub(super) fn is_gecko_browser(browser_id: &str) -> bool {
    matches!(
        browser_id,
        "firefox" | "librewolf" | "zen" | "floorp" | "waterfox"
    )
}
