use std::collections::{HashMap, VecDeque};
use std::sync::atomic::Ordering;

use serde_json::{Value, json};

use crate::MAX_QUERY_BYTES;
use crate::index::{self, Search};

use super::WebService;
use super::browser::{
    default_uri_handler, detect_default_browser_private, detect_installed_browsers,
    dispatch_allowlisted_browser, dispatch_default_browser, find_installed_browser,
};
use super::protocol::{
    ApiError, DEFAULT_SEARCH_LIMIT, MAX_ID_BYTES, MAX_PROFILE_FILTER_LEN, MAX_REQ_ID_BYTES,
    MAX_SEARCH_LIMIT, PerformParams, PerformTarget, validate_entity_id, validate_slug_id,
};

const DISPATCHED_EVENT_CACHE_CAP: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EventFingerprint {
    action: String,
    url: String,
    browser_id: Option<String>,
    profile_id: Option<String>,
    is_private: bool,
}

#[derive(Debug, Clone)]
struct CachedDispatch {
    fingerprint: EventFingerprint,
    response: Value,
}

pub(super) struct DispatchedEvents {
    order: VecDeque<String>,
    entries: HashMap<String, CachedDispatch>,
}

impl DispatchedEvents {
    pub(super) fn new() -> Self {
        Self {
            order: VecDeque::new(),
            entries: HashMap::new(),
        }
    }

    fn get(&self, event_id: &str) -> Option<&CachedDispatch> {
        self.entries.get(event_id)
    }

    fn insert(&mut self, event_id: String, dispatch: CachedDispatch) {
        if let std::collections::hash_map::Entry::Occupied(mut e) =
            self.entries.entry(event_id.clone())
        {
            e.insert(dispatch);
            return;
        }
        if self.order.len() >= DISPATCHED_EVENT_CACHE_CAP
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.order.push_back(event_id.clone());
        self.entries.insert(event_id, dispatch);
    }

    pub(super) fn clear(&mut self) {
        self.order.clear();
        self.entries.clear();
    }
}

impl WebService {
    pub(super) fn search(
        &self,
        query: &str,
        engine_id: Option<&str>,
        profile_ids: Option<Vec<String>>,
        limit: Option<usize>,
    ) -> Result<Value, ApiError> {
        if query.len() > MAX_QUERY_BYTES {
            return Err(ApiError::invalid_query("Query exceeds maximum length."));
        }
        if query.chars().any(char::is_control) {
            return Err(ApiError::invalid_query(
                "Query contains invalid control characters.",
            ));
        }

        if let Some(eid) = engine_id {
            validate_slug_id(eid, 64, "engineId")?;
            if index::engine_url(eid, "").is_none() {
                return Err(ApiError::invalid_request("Unknown search engineId."));
            }
        }

        if let Some(ref ids) = profile_ids {
            if ids.is_empty() || ids.len() > MAX_PROFILE_FILTER_LEN {
                return Err(ApiError::invalid_request(
                    "profileIds must contain between 1 and 64 items.",
                ));
            }
            for id in ids {
                validate_entity_id(id, MAX_ID_BYTES, "profileId")?;
            }
            let enabled_ids = {
                let idx = self.index.lock().unwrap();
                idx.enabled_ids()
                    .map_err(|_| ApiError::internal("Failed to read enabled profiles."))?
            };
            let state = self.shared.state.lock().unwrap();
            for id in ids {
                if !state.known_profiles.contains_key(id) {
                    return Err(ApiError::profile_not_found());
                }
                if !enabled_ids.contains(id) {
                    return Err(ApiError::profile_disabled());
                }
            }
        }

        let bounded_limit = limit
            .unwrap_or(DEFAULT_SEARCH_LIMIT)
            .clamp(1, MAX_SEARCH_LIMIT);
        let stale = self.shared.is_scan_active_for(profile_ids.as_deref());

        let idx = self.index.lock().unwrap();
        idx.search(
            Search {
                query,
                engine_id,
                profile_ids: profile_ids.as_deref(),
                limit: bounded_limit,
            },
            stale,
        )
        .map_err(|_| ApiError::internal("Search query failed."))
    }

    pub(super) fn actions_available(&self, entry_id: &str) -> Result<Value, ApiError> {
        validate_entity_id(entry_id, MAX_ID_BYTES, "entryId")?;

        let (entry_opt, pinned) = {
            let idx = self.index.lock().unwrap();
            let entry_opt = idx
                .entry(entry_id)
                .map_err(|_| ApiError::internal("Failed to resolve index entry."))?;
            let pinned = if entry_opt.is_some() {
                idx.is_pinned(entry_id)
                    .map_err(|_| ApiError::internal("Failed to read pin state."))?
            } else {
                false
            };
            (entry_opt, pinned)
        };
        let Some((raw_url, _profile_id)) = entry_opt else {
            return Err(ApiError::entry_not_found());
        };
        if index::validate_url(&raw_url).is_none() {
            return Err(ApiError::invalid_url("Indexed URL is invalid."));
        }

        let installed = detect_installed_browsers();
        let default_handler = default_uri_handler("https").or_else(|| default_uri_handler("http"));
        let default_available = default_handler.is_some();
        let default_private = detect_default_browser_private(&installed);

        let mut actions = Vec::new();
        actions.push(if default_available {
            json!({
                "action": "open.default",
                "label": "Open",
                "enabled": true,
            })
        } else {
            json!({
                "action": "open.default",
                "label": "Open",
                "enabled": false,
                "reason": "BROWSER_NOT_INSTALLED",
            })
        });

        actions.push(if default_private {
            json!({
                "action": "open.private",
                "label": "Open in Private Window",
                "enabled": true,
            })
        } else {
            json!({
                "action": "open.private",
                "label": "Open in Private Window",
                "enabled": false,
                "reason": if default_available {
                    "OPERATION_UNSUPPORTED"
                } else {
                    "BROWSER_NOT_INSTALLED"
                },
            })
        });

        for browser in &installed {
            actions.push(json!({
                "action": "open.browser",
                "label": format!("Open in {}", browser.def.label),
                "enabled": true,
                "browserId": browser.def.browser_id,
            }));
        }

        actions.push(json!({
            "action": "copy.url",
            "label": "Copy URL",
            "enabled": true,
        }));
        actions.push(json!({
            "action": "search.site",
            "label": "Search Site",
            "enabled": true,
        }));
        actions.push(json!({
            "action": "bookmark.browser",
            "label": "Bookmark in Browser",
            "enabled": false,
            "reason": "EXTENSION_NOT_CONNECTED",
        }));
        actions.push(json!({
            "action": "pin.local",
            "label": if pinned {
                "Unpin from Launcher"
            } else {
                "Pin to Launcher"
            },
            "enabled": true,
        }));
        actions.push(json!({
            "action": "forget.localPage",
            "label": "Forget This Page",
            "enabled": true,
        }));
        actions.push(json!({
            "action": "forget.localSite",
            "label": "Forget Site",
            "enabled": true,
        }));

        Ok(Value::Array(actions))
    }

    pub(super) fn perform(&self, params: PerformParams) -> Result<Value, ApiError> {
        if params.action.is_empty()
            || params.action.len() > 64
            || params.action.chars().any(char::is_control)
        {
            return Err(ApiError::invalid_request("Invalid action name."));
        }

        if let Some(ref pid) = params.profile_id {
            validate_entity_id(pid, MAX_ID_BYTES, "profileId")?;
        }
        if let Some(ref bid) = params.browser_id {
            validate_slug_id(bid, 64, "browserId")?;
        }
        if let Some(ref eid) = params.event_id {
            validate_entity_id(eid, MAX_REQ_ID_BYTES, "eventId")?;
        }

        match params.action.as_str() {
            "forget.browserPage" | "bookmark.browser" => {
                if let Some(ref pid) = params.profile_id {
                    self.require_enabled_profile(pid)?;
                }
                Err(ApiError::extension_not_connected())
            }
            "copy.url" | "search.site" => Err(ApiError::operation_unsupported(
                "Action is handled by the launcher UI.",
            )),
            "pin.local" => self.perform_pin(params.target),
            "forget.localPage" => self.perform_forget(params.target, false),
            "forget.localSite" => self.perform_forget(params.target, true),
            "open.default" | "open.browser" | "open.private" | "search" => {
                self.perform_open(params)
            }
            _ => Err(ApiError::operation_unsupported("Unsupported action.")),
        }
    }

    fn perform_pin(&self, target: Option<PerformTarget>) -> Result<Value, ApiError> {
        let target =
            target.ok_or_else(|| ApiError::invalid_request("Missing target for pin action."))?;
        let entry_id = target
            .entry_id
            .as_deref()
            .ok_or_else(|| ApiError::invalid_request("entryId is required for pin action."))?;
        validate_entity_id(entry_id, MAX_ID_BYTES, "entryId")?;

        let pinned_opt = {
            let mut idx = self.index.lock().unwrap();
            idx.toggle_pin(entry_id)
                .map_err(|_| ApiError::internal("Failed to update pin state."))?
        };
        let Some(pinned) = pinned_opt else {
            return Err(ApiError::entry_not_found());
        };

        Ok(json!({
            "status": "completed",
            "pinned": pinned,
        }))
    }

    fn perform_forget(&self, target: Option<PerformTarget>, site: bool) -> Result<Value, ApiError> {
        let target =
            target.ok_or_else(|| ApiError::invalid_request("Missing target for forget action."))?;
        let entry_id = target
            .entry_id
            .as_deref()
            .ok_or_else(|| ApiError::invalid_request("entryId is required for forget action."))?;
        validate_entity_id(entry_id, MAX_ID_BYTES, "entryId")?;

        let resolved = {
            let idx = self.index.lock().unwrap();
            idx.entry(entry_id)
                .map_err(|_| ApiError::internal("Failed to resolve index entry."))?
        };
        let Some((raw_url, _profile_id)) = resolved else {
            return Err(ApiError::entry_not_found());
        };

        let affected = {
            let mut idx = self.index.lock().unwrap();
            idx.hide(entry_id, site)
                .map_err(|_| ApiError::internal("Failed to update hidden state."))?
        };

        if let Some(parsed) = index::validate_url(&raw_url) {
            let mut cache = self.favicons.cache.lock().unwrap();
            if site {
                if let Some(host) = parsed.host_str() {
                    cache.invalidate_host(host);
                }
            } else {
                cache.invalidate_url(parsed.as_str());
            }
        }

        Ok(json!({
            "status": "completed",
            "scope": "launcherOnly",
            "affectedProfileCount": affected,
        }))
    }

    fn perform_open(&self, params: PerformParams) -> Result<Value, ApiError> {
        let action = params.action.as_str();
        let target = params
            .target
            .ok_or_else(|| ApiError::invalid_request("Missing target."))?;

        let is_private = match action {
            "open.private" => {
                if params.private == Some(false) {
                    return Err(ApiError::invalid_request(
                        "open.private cannot set private=false.",
                    ));
                }
                true
            }
            _ => params.private.unwrap_or(false),
        };

        if action == "open.default" && params.browser_id.is_some() {
            return Err(ApiError::invalid_request(
                "browserId cannot be used with open.default.",
            ));
        }

        // Validate explicit profileId if provided.
        let explicit_profile = match params.profile_id.as_deref() {
            Some(pid) => Some(self.require_enabled_profile(pid)?),
            None => None,
        };

        if let (Some(prof), Some(bid)) = (&explicit_profile, params.browser_id.as_deref())
            && prof.browser_id != bid
        {
            return Err(ApiError::invalid_request(
                "profileId does not belong to the requested browserId.",
            ));
        }

        // Resolve target URL (entryId is authoritative when provided).
        let (resolved_url, entry_profile_id) = if let Some(ref eid) = target.entry_id {
            validate_entity_id(eid, MAX_ID_BYTES, "entryId")?;
            let entry_opt = {
                let idx = self.index.lock().unwrap();
                idx.entry(eid)
                    .map_err(|_| ApiError::internal("Failed to resolve index entry."))?
            };
            let Some((raw_url, entry_pid)) = entry_opt else {
                return Err(ApiError::entry_not_found());
            };
            let parsed = index::validate_url(&raw_url)
                .ok_or_else(|| ApiError::invalid_url("Indexed URL is invalid."))?;
            (parsed, Some(entry_pid))
        } else if action == "search" {
            if let Some(ref raw_url) = target.url {
                if let Some(ref engine_id) = target.engine_id {
                    validate_slug_id(engine_id, 64, "engineId")?;
                    if index::engine_url(engine_id, "").is_none() {
                        return Err(ApiError::invalid_request("Unknown search engineId."));
                    }
                }
                let parsed = index::validate_url(raw_url)
                    .ok_or_else(|| ApiError::invalid_url("Invalid search target URL."))?;
                (parsed, None)
            } else if let (Some(engine_id), Some(q)) =
                (target.engine_id.as_deref(), target.query.as_deref())
            {
                validate_slug_id(engine_id, 64, "engineId")?;
                if q.len() > MAX_QUERY_BYTES || q.chars().any(char::is_control) {
                    return Err(ApiError::invalid_query("Invalid search query."));
                }
                let built = index::engine_url(engine_id, q)
                    .ok_or_else(|| ApiError::invalid_request("Unknown search engineId."))?;
                let parsed = index::validate_url(built.as_str())
                    .ok_or_else(|| ApiError::invalid_url("Invalid search target URL."))?;
                (parsed, None)
            } else {
                return Err(ApiError::invalid_request(
                    "Search action requires target.url or engineId+query.",
                ));
            }
        } else {
            let raw_url = target
                .url
                .as_deref()
                .ok_or_else(|| ApiError::invalid_request("Target url or entryId is required."))?;
            let parsed = index::validate_url(raw_url)
                .ok_or_else(|| ApiError::invalid_url("Invalid or disallowed target URL."))?;
            (parsed, None)
        };

        // Check in-session idempotent replay for provided eventId.
        let fingerprint = EventFingerprint {
            action: action.to_owned(),
            url: resolved_url.as_str().to_owned(),
            browser_id: params.browser_id.clone(),
            profile_id: params.profile_id.clone(),
            is_private,
        };
        if let Some(ref eid) = params.event_id {
            let cache = self.dispatched_events.lock().unwrap();
            if let Some(prev) = cache.get(eid) {
                if prev.fingerprint != fingerprint {
                    return Err(ApiError::invalid_request(
                        "eventId was already used for a different request.",
                    ));
                }
                return Ok(prev.response.clone());
            }
        }

        // Dispatch according to strict strategy rules (no implicit fallback).
        let installed = detect_installed_browsers();
        match action {
            "open.default" | "search" => {
                if let Some(bid) = params.browser_id.as_deref() {
                    let handler = find_installed_browser(&installed, bid)
                        .ok_or_else(ApiError::browser_not_installed)?;
                    dispatch_allowlisted_browser(
                        handler,
                        &resolved_url,
                        explicit_profile.as_ref(),
                        is_private,
                    )?;
                } else {
                    dispatch_default_browser(
                        &installed,
                        &resolved_url,
                        explicit_profile.as_ref(),
                        is_private,
                    )?;
                }
            }
            "open.browser" => {
                let target_bid = params
                    .browser_id
                    .as_deref()
                    .or_else(|| explicit_profile.as_ref().map(|p| p.browser_id.as_str()))
                    .ok_or_else(|| {
                        ApiError::invalid_request("open.browser requires browserId or profileId.")
                    })?;
                let handler = find_installed_browser(&installed, target_bid)
                    .ok_or_else(ApiError::browser_not_installed)?;
                dispatch_allowlisted_browser(
                    handler,
                    &resolved_url,
                    explicit_profile.as_ref(),
                    is_private,
                )?;
            }
            "open.private" => {
                if let Some(bid) = params.browser_id.as_deref() {
                    let handler = find_installed_browser(&installed, bid)
                        .ok_or_else(ApiError::browser_not_installed)?;
                    dispatch_allowlisted_browser(
                        handler,
                        &resolved_url,
                        explicit_profile.as_ref(),
                        true,
                    )?;
                } else if let Some(ref prof) = explicit_profile {
                    let handler = find_installed_browser(&installed, &prof.browser_id)
                        .ok_or_else(ApiError::browser_not_installed)?;
                    dispatch_allowlisted_browser(handler, &resolved_url, Some(prof), true)?;
                } else {
                    dispatch_default_browser(&installed, &resolved_url, None, true)?;
                }
            }
            _ => return Err(ApiError::operation_unsupported("Unsupported open action.")),
        }

        // Only record successful non-private dispatches; report recording result independently.
        let launcher_open_recorded = if is_private {
            false
        } else {
            let effective_event_id = params.event_id.clone().unwrap_or_else(|| {
                let seq = self.auto_event_seq.fetch_add(1, Ordering::Relaxed);
                format!("auto-{}-{seq}", std::process::id())
            });
            let record_profile_id = params.profile_id.as_deref().or(entry_profile_id.as_deref());
            let mut idx = self.index.lock().unwrap();
            idx.record_open(&resolved_url, record_profile_id, &effective_event_id)
                .is_ok()
        };

        let response = json!({
            "status": "dispatched",
            "nativeHistoryExpected": !is_private,
            "nativeVisitConfirmed": false,
            "launcherOpenRecorded": launcher_open_recorded,
        });

        if let Some(eid) = params.event_id {
            let mut cache = self.dispatched_events.lock().unwrap();
            cache.insert(
                eid,
                CachedDispatch {
                    fingerprint,
                    response: response.clone(),
                },
            );
        }

        Ok(response)
    }
}
