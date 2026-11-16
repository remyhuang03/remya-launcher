use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::browsers::{self, Profile};
use crate::index::Index;

use super::favicon::FaviconManager;
use super::profiles::{ProfileSyncStatus, SharedCoordinator, reconcile_discovered_locked};
use super::state::{now_unix_ms, save_profiles_meta, tighten_db_permissions};

const SYNC_INTERVAL: Duration = Duration::from_secs(60);

pub(super) fn scanner_loop(
    state_dir: PathBuf,
    db_path: PathBuf,
    shared: Arc<SharedCoordinator>,
    favicons: Arc<FaviconManager>,
) {
    let mut scanner_index = match Index::open(&db_path) {
        Ok(idx) => idx,
        Err(_) => return,
    };
    tighten_db_permissions(&state_dir);

    loop {
        let (batch_all, batch_profiles) = {
            let mut state = shared.state.lock().unwrap();
            if !state.shutdown
                && !state.queue.pending_all
                && state.queue.pending_profiles.is_empty()
            {
                let (guard, timeout_res) = shared
                    .wakeup
                    .wait_timeout(state, SYNC_INTERVAL)
                    .unwrap_or_else(|e| e.into_inner());
                state = guard;
                if !state.shutdown
                    && timeout_res.timed_out()
                    && !state.queue.pending_all
                    && state.queue.pending_profiles.is_empty()
                {
                    state.queue.pending_all = true;
                }
            }

            if state.shutdown {
                return;
            }

            let all = state.queue.pending_all;
            let profiles = std::mem::take(&mut state.queue.pending_profiles);
            state.queue.pending_all = false;
            state.queue.running = true;
            state.queue.running_all = all;
            state.queue.running_profiles = profiles.clone();
            (all, profiles)
        };

        let discovered = browsers::discover();
        let enabled_ids = {
            let _mut_guard = shared.mutation_lock.lock().unwrap();
            scanner_index.enabled_ids().unwrap_or_default()
        };

        let targets: Vec<Profile> = {
            let mut state = shared.state.lock().unwrap();
            let newly_disabled =
                reconcile_discovered_locked(&mut state.known_profiles, discovered, &enabled_ids);
            let _ = save_profiles_meta(&state_dir, &state.known_profiles);

            let mut list = Vec::new();
            for rec in state.known_profiles.values_mut() {
                if !rec.enabled || !rec.present_on_disk {
                    continue;
                }
                if batch_all || batch_profiles.contains(&rec.profile.id) {
                    rec.status = ProfileSyncStatus::Syncing;
                    list.push(rec.profile.clone());
                }
            }
            state.queue.running_profiles = list.iter().map(|p| p.id.clone()).collect();
            drop(state);
            if !newly_disabled.is_empty() {
                let mut cache = favicons.cache.lock().unwrap();
                for pid in newly_disabled {
                    cache.invalidate_profile(&pid);
                }
            }
            list
        };

        for profile in targets {
            let snapshot_gens = {
                let state = shared.state.lock().unwrap();
                if state.shutdown {
                    return;
                }
                state.known_profiles.get(&profile.id).and_then(|rec| {
                    (rec.enabled && rec.present_on_disk)
                        .then_some((rec.opt_in_gen, state.clear_gen))
                })
            };
            let Some((opt_gen, clear_gen)) = snapshot_gens else {
                continue;
            };

            // Capture immutable persisted consent token just before source read.
            let token_res = {
                let _mut_guard = shared.mutation_lock.lock().unwrap();
                scanner_index.consent_token(&profile.id)
            };
            let consent_token = match token_res {
                Ok(Some(token)) => token,
                Ok(None) => {
                    {
                        let mut state = shared.state.lock().unwrap();
                        if state.clear_gen == clear_gen
                            && let Some(rec) = state.known_profiles.get_mut(&profile.id)
                            && rec.opt_in_gen == opt_gen
                        {
                            rec.enabled = false;
                            rec.status = ProfileSyncStatus::Disabled;
                            rec.last_good_sync_ms = None;
                            rec.error_code = None;
                        }
                    }
                    let mut cache = favicons.cache.lock().unwrap();
                    cache.invalidate_profile(&profile.id);
                    continue;
                }
                Err(_) => {
                    let mut state = shared.state.lock().unwrap();
                    if state.clear_gen == clear_gen
                        && let Some(rec) = state.known_profiles.get_mut(&profile.id)
                        && rec.enabled
                        && rec.opt_in_gen == opt_gen
                    {
                        rec.status = if rec.last_good_sync_ms.is_some() {
                            ProfileSyncStatus::Stale
                        } else {
                            ProfileSyncStatus::Error
                        };
                        rec.error_code = Some("SYNC_FAILED");
                    }
                    continue;
                }
            };

            let read_result = browsers::read(&profile);

            let _mut_guard = shared.mutation_lock.lock().unwrap();
            let still_valid = {
                let state = shared.state.lock().unwrap();
                !state.shutdown
                    && state.clear_gen == clear_gen
                    && state.known_profiles.get(&profile.id).is_some_and(|rec| {
                        rec.enabled && rec.opt_in_gen == opt_gen && rec.present_on_disk
                    })
            };
            if !still_valid {
                continue;
            }

            match read_result {
                Ok(entries) => {
                    let replace_res =
                        scanner_index.replace_if_current(&profile, entries, &consent_token);
                    tighten_db_permissions(&state_dir);

                    match replace_res {
                        Ok(true) => {
                            let mut state = shared.state.lock().unwrap();
                            if state.clear_gen != clear_gen {
                                continue;
                            }
                            if let Some(rec) = state.known_profiles.get_mut(&profile.id) {
                                if !rec.enabled || rec.opt_in_gen != opt_gen {
                                    continue;
                                }
                                rec.last_good_sync_ms = Some(now_unix_ms());
                                rec.status = ProfileSyncStatus::Ready;
                                rec.error_code = None;
                                drop(state);
                                let mut cache = favicons.cache.lock().unwrap();
                                cache.invalidate_profile(&profile.id);
                            }
                        }
                        Ok(false) => {
                            // Persisted consent token changed or was revoked while reading;
                            // never mark stale snapshot as ready.
                            let current_token = scanner_index.consent_token(&profile.id);
                            let mut state = shared.state.lock().unwrap();
                            if state.clear_gen != clear_gen {
                                continue;
                            }
                            if let Some(rec) = state.known_profiles.get_mut(&profile.id) {
                                if !rec.enabled || rec.opt_in_gen != opt_gen {
                                    continue;
                                }
                                match current_token {
                                    Ok(None) => {
                                        rec.enabled = false;
                                        rec.status = ProfileSyncStatus::Disabled;
                                        rec.last_good_sync_ms = None;
                                        rec.error_code = None;
                                    }
                                    Ok(Some(_)) => {
                                        rec.enabled = true;
                                        rec.last_good_sync_ms = None;
                                        rec.error_code = None;
                                        rec.status = ProfileSyncStatus::Syncing;
                                        state.queue.pending_profiles.insert(profile.id.clone());
                                        shared.wakeup.notify_all();
                                    }
                                    Err(_) => {
                                        rec.status = ProfileSyncStatus::Error;
                                        rec.last_good_sync_ms = None;
                                        rec.error_code = Some("SYNC_FAILED");
                                    }
                                }
                            }
                            drop(state);
                            let mut cache = favicons.cache.lock().unwrap();
                            cache.invalidate_profile(&profile.id);
                        }
                        Err(_) => {
                            let mut state = shared.state.lock().unwrap();
                            if state.clear_gen != clear_gen {
                                continue;
                            }
                            if let Some(rec) = state.known_profiles.get_mut(&profile.id) {
                                if !rec.enabled || rec.opt_in_gen != opt_gen {
                                    continue;
                                }
                                rec.status = if rec.last_good_sync_ms.is_some() {
                                    ProfileSyncStatus::Stale
                                } else {
                                    ProfileSyncStatus::Error
                                };
                                rec.error_code = Some("SYNC_FAILED");
                            }
                        }
                    }
                }
                Err(code) => {
                    let current_token = scanner_index.consent_token(&profile.id);
                    let mut state = shared.state.lock().unwrap();
                    if state.clear_gen != clear_gen {
                        continue;
                    }
                    if let Some(rec) = state.known_profiles.get_mut(&profile.id) {
                        if !rec.enabled || rec.opt_in_gen != opt_gen {
                            continue;
                        }
                        match current_token {
                            Ok(None) => {
                                rec.enabled = false;
                                rec.status = ProfileSyncStatus::Disabled;
                                rec.last_good_sync_ms = None;
                                rec.error_code = None;
                                drop(state);
                                let mut cache = favicons.cache.lock().unwrap();
                                cache.invalidate_profile(&profile.id);
                            }
                            Ok(Some(ref cur)) if cur != &consent_token => {
                                rec.enabled = true;
                                rec.last_good_sync_ms = None;
                                rec.error_code = None;
                                rec.status = ProfileSyncStatus::Syncing;
                                state.queue.pending_profiles.insert(profile.id.clone());
                                shared.wakeup.notify_all();
                                drop(state);
                                let mut cache = favicons.cache.lock().unwrap();
                                cache.invalidate_profile(&profile.id);
                            }
                            _ => {
                                rec.status = if rec.last_good_sync_ms.is_some() {
                                    ProfileSyncStatus::Stale
                                } else {
                                    ProfileSyncStatus::Error
                                };
                                rec.error_code = Some(sanitize_reader_error(code));
                            }
                        }
                    }
                }
            }
        }

        let mut state = shared.state.lock().unwrap();
        state.queue.running = false;
        state.queue.running_all = false;
        state.queue.running_profiles.clear();
    }
}

fn sanitize_reader_error(code: &'static str) -> &'static str {
    if !code.is_empty()
        && code.len() <= 64
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
    {
        code
    } else {
        "SYNC_FAILED"
    }
}
