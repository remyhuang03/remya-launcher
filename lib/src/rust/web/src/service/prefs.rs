use serde_json::{Value, json};

use crate::index;

use super::WebService;
use super::profiles::ProfileSyncStatus;
use super::protocol::{ApiError, validate_slug_id};
use super::state::remove_profiles_meta;

impl WebService {
    pub(super) fn prefs_get(&self) -> Result<Value, ApiError> {
        let idx = self.index.lock().unwrap();
        let default_engine_id = idx
            .default_engine_id()
            .map_err(|_| ApiError::internal("Failed to read web preferences."))?;
        Ok(prefs_json(&default_engine_id))
    }

    pub(super) fn prefs_set(&self, values: Value) -> Result<Value, ApiError> {
        let Some(map) = values.as_object() else {
            return Err(ApiError::invalid_request(
                "prefs.set values must be an object.",
            ));
        };

        let mut next_default_engine_id: Option<&str> = None;

        for (key, val) in map {
            match key.as_str() {
                "localIndex" => {
                    if val.as_bool() != Some(true) {
                        return Err(ApiError::operation_unsupported(
                            "localIndex cannot be disabled via preferences.",
                        ));
                    }
                }
                "networkIcons" => {
                    if val.as_bool() != Some(false) {
                        return Err(ApiError::operation_unsupported(
                            "Network icon fetching is not supported.",
                        ));
                    }
                }
                "nativeHistory" => {
                    if val.as_bool() != Some(false) {
                        return Err(ApiError::operation_unsupported(
                            "Native browser history writes are not supported.",
                        ));
                    }
                }
                "syncIntervalSecs" => {
                    if val.as_u64() != Some(60) {
                        return Err(ApiError::operation_unsupported(
                            "Custom syncIntervalSecs is not supported.",
                        ));
                    }
                }
                "defaultEngineId" => {
                    let Some(engine_id) = val.as_str() else {
                        return Err(ApiError::invalid_request(
                            "defaultEngineId must be a string.",
                        ));
                    };
                    validate_slug_id(engine_id, 64, "defaultEngineId")?;
                    if !index::is_supported_default_engine(engine_id) {
                        return Err(ApiError::invalid_request(
                            "defaultEngineId must be 'duckduckgo' or 'google'.",
                        ));
                    }
                    next_default_engine_id = Some(engine_id);
                }
                _ => {
                    return Err(ApiError::operation_unsupported(
                        "Unsupported web preference key.",
                    ));
                }
            }
        }

        if let Some(engine_id) = next_default_engine_id {
            let _mut_guard = self.shared.mutation_lock.lock().unwrap();
            let mut idx = self.index.lock().unwrap();
            idx.set_default_engine_id(engine_id)
                .map_err(|_| ApiError::internal("Failed to persist web preferences."))?;
        }

        self.prefs_get()
    }

    pub(super) fn prefs_clear(&self) -> Result<Value, ApiError> {
        let _mut_guard = self.shared.mutation_lock.lock().unwrap();

        {
            let mut idx = self.index.lock().unwrap();
            idx.clear()
                .map_err(|_| ApiError::internal("Failed to clear local web index."))?;
        }

        {
            let mut cache = self.favicons.cache.lock().unwrap();
            cache.clear();
        }
        {
            let mut events = self.dispatched_events.lock().unwrap();
            events.clear();
        }

        {
            let mut state = self.shared.state.lock().unwrap();
            state.clear_gen = state.clear_gen.wrapping_add(1);
            state.queue.pending_all = false;
            state.queue.pending_profiles.clear();
            state.known_profiles.retain(|_, rec| rec.present_on_disk);
            for rec in state.known_profiles.values_mut() {
                rec.opt_in_gen = rec.opt_in_gen.wrapping_add(1);
                rec.enabled = false;
                rec.status = ProfileSyncStatus::Disabled;
                rec.last_good_sync_ms = None;
                rec.error_code = None;
            }
        }

        remove_profiles_meta(&self.state_dir);

        Ok(json!({ "cleared": true }))
    }
}

fn prefs_json(default_engine_id: &str) -> Value {
    json!({
        "localIndex": true,
        "networkIcons": false,
        "nativeHistory": false,
        "syncIntervalSecs": 60,
        "defaultEngineId": default_engine_id,
    })
}
