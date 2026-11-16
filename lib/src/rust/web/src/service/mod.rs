mod actions;
mod browser;
mod favicon;
mod prefs;
mod profiles;
mod protocol;
mod scanner;
mod state;

use std::collections::{HashMap, HashSet};
use std::io::{self, BufReader, Read, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use serde_json::{Value, json};

use crate::browsers::{self, Family, Profile};
use crate::index::{self, Index};

use actions::DispatchedEvents;
use browser::{detect_default_browser_private, detect_installed_browsers};
use favicon::FaviconManager;
use profiles::{
    CoordinatorState, ProfileRecord, ProfileSyncStatus, SharedCoordinator, SyncQueue,
    is_gecko_browser,
};
use protocol::{
    ActionsAvailableParams, ApiError, EmptyParams, FaviconReadParams, FaviconResolveParams,
    LineStatus, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, PerformParams, PrefsSetParams, SearchParams,
    SetEnabledParams, SyncRequestParams, is_valid_op_name, is_valid_request_id, parse_params,
    read_bounded_line, serialize_error, serialize_success,
};
use scanner::scanner_loop;
use state::{
    ensure_private_web_state_dir, load_profiles_meta, save_profiles_meta, tighten_db_permissions,
    validate_existing_private_file,
};

pub struct WebService {
    state_dir: PathBuf,
    index: Mutex<Index>,
    shared: Arc<SharedCoordinator>,
    favicons: Arc<FaviconManager>,
    dispatched_events: Mutex<DispatchedEvents>,
    auto_event_seq: AtomicU64,
    scanner_handle: Option<JoinHandle<()>>,
}

impl Drop for WebService {
    fn drop(&mut self) {
        {
            let mut state = self.shared.state.lock().unwrap();
            state.shutdown = true;
            self.shared.wakeup.notify_all();
        }
        if let Some(handle) = self.scanner_handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn serve_stdio(input: impl Read, mut output: impl Write) -> io::Result<()> {
    let service = WebService::new().map_err(|_| io::Error::other("WebService init failed"))?;
    let mut reader = BufReader::new(input);
    let mut line = String::new();

    loop {
        match read_bounded_line(&mut reader, &mut line, MAX_REQUEST_BYTES)? {
            LineStatus::Eof => return Ok(()),
            LineStatus::TooLarge => {
                let reply = serialize_error(
                    Value::Null,
                    ApiError::invalid_request("Request frame exceeds maximum size."),
                );
                writeln!(output, "{reply}")?;
                output.flush()?;
            }
            LineStatus::InvalidUtf8 => {
                let reply = serialize_error(
                    Value::Null,
                    ApiError::invalid_request("Request frame is not valid UTF-8."),
                );
                writeln!(output, "{reply}")?;
                output.flush()?;
            }
            LineStatus::Ok => {
                let reply = service.handle_line(&line);
                writeln!(output, "{reply}")?;
                output.flush()?;
            }
        }
    }
}

impl WebService {
    pub fn new() -> Result<Self, String> {
        let state_dir = ensure_private_web_state_dir()
            .map_err(|_| "Failed to prepare private web state directory.".to_owned())?;
        let db_path = state_dir.join("index.sqlite");
        validate_existing_private_file(&db_path)
            .map_err(|_| "Web index database failed private ownership validation.".to_owned())?;

        let index = Index::open(&db_path)
            .map_err(|_| "Failed to open private web index database.".to_owned())?;
        tighten_db_permissions(&state_dir);

        let enabled_ids = index
            .enabled_ids()
            .map_err(|_| "Failed to read enabled profile state.".to_owned())?;

        let discovered = browsers::discover();
        let persisted_meta = load_profiles_meta(&state_dir);
        let mut known_profiles = HashMap::new();

        for p in discovered {
            let enabled = enabled_ids.contains(&p.id);
            known_profiles.insert(
                p.id.clone(),
                ProfileRecord {
                    profile: p,
                    enabled,
                    status: if enabled {
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

        // Preserve status visibility for previously enabled profiles missing at startup.
        for id in &enabled_ids {
            if !known_profiles.contains_key(id)
                && let Some(meta) = persisted_meta.get(id)
            {
                known_profiles.insert(
                    id.clone(),
                    ProfileRecord {
                        profile: Profile {
                            id: meta.profile_id.clone(),
                            browser_id: meta.browser_id.clone(),
                            browser_label: meta.browser_label.clone(),
                            label: meta.label.clone(),
                            path: PathBuf::new(),
                            family: if is_gecko_browser(&meta.browser_id) {
                                Family::Gecko
                            } else {
                                Family::Chromium
                            },
                        },
                        enabled: true,
                        status: ProfileSyncStatus::Error,
                        last_good_sync_ms: None,
                        error_code: Some("PROFILE_MISSING"),
                        present_on_disk: false,
                        opt_in_gen: 0,
                    },
                );
            }
        }

        let _ = save_profiles_meta(&state_dir, &known_profiles);

        let has_enabled = known_profiles.values().any(|r| r.enabled);
        let shared = Arc::new(SharedCoordinator {
            mutation_lock: Mutex::new(()),
            state: Mutex::new(CoordinatorState {
                known_profiles,
                clear_gen: 0,
                queue: SyncQueue {
                    pending_all: has_enabled,
                    pending_profiles: HashSet::new(),
                    running: false,
                    running_all: false,
                    running_profiles: HashSet::new(),
                    next_job_seq: 1,
                },
                shutdown: false,
            }),
            wakeup: Condvar::new(),
        });
        let favicons = Arc::new(FaviconManager::new());

        let scanner_shared = Arc::clone(&shared);
        let scanner_favicons = Arc::clone(&favicons);
        let scanner_state_dir = state_dir.clone();
        let scanner_db_path = db_path.clone();
        let scanner_handle = thread::Builder::new()
            .name("web-scanner".into())
            .spawn(move || {
                scanner_loop(
                    scanner_state_dir,
                    scanner_db_path,
                    scanner_shared,
                    scanner_favicons,
                );
            })
            .ok();

        Ok(Self {
            state_dir,
            index: Mutex::new(index),
            shared,
            favicons,
            dispatched_events: Mutex::new(DispatchedEvents::new()),
            auto_event_seq: AtomicU64::new(1),
            scanner_handle,
        })
    }

    pub fn handle_line(&self, line: &str) -> String {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return serialize_error(Value::Null, ApiError::invalid_request("Empty request."));
        }

        let raw_value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                return serialize_error(
                    Value::Null,
                    ApiError::invalid_request("Malformed JSON request."),
                );
            }
        };

        let Some(obj) = raw_value.as_object() else {
            return serialize_error(
                Value::Null,
                ApiError::invalid_request("Request must be a JSON object."),
            );
        };

        let Some(id_val) = obj.get("id") else {
            return serialize_error(
                Value::Null,
                ApiError::invalid_request("Missing request id."),
            );
        };
        if !is_valid_request_id(id_val) {
            return serialize_error(
                Value::Null,
                ApiError::invalid_request("Malformed request id."),
            );
        }
        let req_id = id_val.clone();

        let Some(op_str) = obj.get("op").and_then(Value::as_str) else {
            return serialize_error(
                req_id,
                ApiError::invalid_request("Missing or invalid request op."),
            );
        };
        if !is_valid_op_name(op_str) {
            return serialize_error(
                req_id,
                ApiError::invalid_request("Malformed operation name."),
            );
        }

        let op_owned = op_str.to_owned();
        let result = self.dispatch(&op_owned, raw_value);
        let serialized = match result {
            Ok(data) => serialize_success(&req_id, data),
            Err(err) => serialize_error(req_id.clone(), err),
        };

        if serialized.len() > MAX_RESPONSE_BYTES {
            return serialize_error(
                req_id,
                ApiError::internal("Response exceeds maximum frame size."),
            );
        }
        serialized
    }

    fn dispatch(&self, op: &str, raw_value: Value) -> Result<Value, ApiError> {
        match op {
            "web.capabilities" => {
                let _: EmptyParams = parse_params(raw_value)?;
                Ok(self.capabilities())
            }
            "web.profiles.list" => {
                let _: EmptyParams = parse_params(raw_value)?;
                self.profiles_list()
            }
            "web.profiles.setEnabled" => {
                let params: SetEnabledParams = parse_params(raw_value)?;
                self.profiles_set_enabled(&params.profile_id, params.enabled)
            }
            "web.sync.status" => {
                let _: EmptyParams = parse_params(raw_value)?;
                self.sync_status()
            }
            "web.sync.request" => {
                let params: SyncRequestParams = parse_params(raw_value)?;
                self.sync_request(params.profile_ids, params.mode.as_deref())
            }
            "web.engines.list" => {
                let _: EmptyParams = parse_params(raw_value)?;
                Ok(index::engines())
            }
            "web.search" => {
                let params: SearchParams = parse_params(raw_value)?;
                self.search(
                    &params.query,
                    params.engine_id.as_deref(),
                    params.profile_ids,
                    params.limit,
                )
            }
            "web.actions.available" => {
                let params: ActionsAvailableParams = parse_params(raw_value)?;
                self.actions_available(&params.entry_id)
            }
            "web.perform" => {
                let params: PerformParams = parse_params(raw_value)?;
                self.perform(params)
            }
            "web.favicon.resolve" => {
                let params: FaviconResolveParams = parse_params(raw_value)?;
                self.favicon_resolve(&params.url, params.profile_id.as_deref())
            }
            "web.favicon.read" => {
                let params: FaviconReadParams = parse_params(raw_value)?;
                self.favicon_read(&params.icon_key)
            }
            "web.prefs.get" => {
                let _: EmptyParams = parse_params(raw_value)?;
                self.prefs_get()
            }
            "web.prefs.set" => {
                let params: PrefsSetParams = parse_params(raw_value)?;
                self.prefs_set(params.values)
            }
            "web.prefs.clear" => {
                let _: EmptyParams = parse_params(raw_value)?;
                self.prefs_clear()
            }
            _ => Err(ApiError::operation_unsupported("Unsupported operation.")),
        }
    }

    fn capabilities(&self) -> Value {
        let installed = detect_installed_browsers();
        let default_private = detect_default_browser_private(&installed);
        let browsers_json: Vec<Value> = installed
            .iter()
            .map(|b| {
                json!({
                    "browserId": b.def.browser_id,
                    "label": b.def.label,
                    "canOpen": true,
                    "canOpenPrivate": b.supports_private,
                })
            })
            .collect();

        json!({
            "localIndex": true,
            "networkIcons": false,
            "nativeHistory": false,
            "browsers": browsers_json,
            "defaultPrivate": default_private,
        })
    }
}
