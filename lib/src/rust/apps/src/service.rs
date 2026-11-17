use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use crate::desktop::{DesktopCatalog, resolve_installed_aur_desktops};
use crate::hyprland;
use crate::launch;
use crate::packages::PackageCache;
use crate::protocol::{Operation, RequestEnvelope, serialize_result};
use crate::search::{list_hidden_apps, run_search};
use crate::store::{Store, now_unix_secs};
use crate::windows::list_windows;

pub struct AppsService {
    catalog: Mutex<DesktopCatalog>,
    store: Mutex<Store>,
    packages: Arc<Mutex<PackageCache>>,
}

impl AppsService {
    pub fn new(start_watcher: bool) -> Result<Self, String> {
        let catalog = DesktopCatalog::load(start_watcher);
        let store = Store::open().map_err(|e| format!("Failed to open apps storage: {e}"))?;
        let packages = Arc::new(Mutex::new(PackageCache::default()));

        Ok(Self {
            catalog: Mutex::new(catalog),
            store: Mutex::new(store),
            packages,
        })
    }

    pub fn warm_background(self: &Arc<Self>) {
        let this = Arc::clone(self);
        std::thread::Builder::new()
            .name("apps-pkg-warm".into())
            .spawn(move || {
                let official = {
                    let mut pkgs = this.packages.lock().unwrap();
                    pkgs.ensure_loaded();
                    pkgs.official_names()
                };
                let aur_paths = resolve_installed_aur_desktops(&official);
                if let Ok(mut catalog) = this.catalog.lock() {
                    catalog.set_aur_desktop_paths(aur_paths);
                }
            })
            .ok();
    }

    pub fn handle_line(&self, line: &str) -> String {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return serialize_result::<Value>(Value::Null, Err("Empty request.".into()));
        }

        let raw_value: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                return serialize_result::<Value>(Value::Null, Err(format!("Invalid JSON: {e}")));
            }
        };

        let id = raw_value.get("id").cloned().unwrap_or(Value::Null);
        if id.is_null() {
            return serialize_result::<Value>(Value::Null, Err("Missing request id.".into()));
        }

        let request: RequestEnvelope = match serde_json::from_value(raw_value) {
            Ok(req) => req,
            Err(e) => {
                return serialize_result::<Value>(id, Err(format!("Invalid request: {e}")));
            }
        };

        let result = self.dispatch(request.op);
        serialize_result(id, result)
    }

    fn dispatch(&self, op: Operation) -> Result<Value, String> {
        match op {
            Operation::Search { query, packages } => self.search(&query, packages),
            Operation::Perform {
                action,
                app_id,
                query,
                target,
                package_id,
                source,
            } => self.perform(
                &action,
                app_id.as_deref(),
                &query,
                target.as_deref(),
                package_id.as_deref(),
                source.as_deref(),
            ),
            Operation::Preferences { values } => {
                let mut store = self.store.lock().unwrap();
                let prefs = match values {
                    Some(patch) => store.update_preferences(patch)?,
                    None => {
                        store.refresh_if_changed();
                        store.preferences().clone()
                    }
                };
                serde_json::to_value(prefs).map_err(|e| format!("Serialization error: {e}"))
            }
            Operation::Hidden {} => self.hidden(),
            Operation::Windows { query } => {
                let clients = hyprland::fetch_clients()?;
                let mut catalog = self.catalog.lock().unwrap();
                catalog.refresh_if_needed();
                let windows = list_windows(catalog.apps(), &clients, &query);
                Ok(json!({ "windows": windows }))
            }
        }
    }

    fn search(&self, query: &str, allow_packages: bool) -> Result<Value, String> {
        let clients = hyprland::fetch_clients().unwrap_or_default();
        let now_secs = now_unix_secs();
        let query = query.trim();
        let (mut data, package_query) = {
            let mut catalog = self.catalog.lock().unwrap();
            catalog.refresh_if_needed();
            let mut store = self.store.lock().unwrap();
            store.refresh_if_changed();
            let data = run_search(&catalog, &store, &clients, query, now_secs);
            let prefs = store.preferences();
            let package_query = (data.matched == 0
                && allow_packages
                && prefs.show_packages
                && query.chars().count() >= 3)
                .then(|| {
                    let installed_flatpaks = catalog
                        .apps()
                        .iter()
                        .filter(|app| app.source.as_deref() == Some("Flatpak"))
                        .map(|app| app.desktop_id_stem_lower.clone())
                        .collect();
                    (prefs.clone(), installed_flatpaks)
                });
            (data, package_query)
        };
        if let Some((prefs, installed_flatpaks)) = package_query {
            let mut cache = self.packages.lock().unwrap();
            data.packages = cache.search(query, &prefs, &installed_flatpaks);
        }
        serde_json::to_value(data).map_err(|e| format!("Serialization error: {e}"))
    }

    fn hidden(&self) -> Result<Value, String> {
        let clients = hyprland::fetch_clients().unwrap_or_default();
        let mut catalog = self.catalog.lock().unwrap();
        catalog.refresh_if_needed();
        let mut store = self.store.lock().unwrap();
        store.refresh_if_changed();
        let apps = list_hidden_apps(&catalog, &store, &clients);
        Ok(json!({ "apps": apps }))
    }

    fn perform(
        &self,
        action: &str,
        app_id: Option<&str>,
        query: &str,
        target: Option<&str>,
        package_id: Option<&str>,
        source: Option<&str>,
    ) -> Result<Value, String> {
        match action {
            "open" | "new" => {
                let id = require_non_empty(app_id, "appId")?;
                let app = self.lookup_app(id)?;
                let prefs = self.current_prefs();
                launch::launch_new_instance(&app, &prefs)?;
                Ok(self.finish_launched_action(&app.id, query))
            }
            "window" => {
                let addr = require_non_empty(target, "target")?;
                hyprland::focus_known_window(addr)?;
                if let Some(id) = app_id.filter(|s| !s.is_empty())
                    && self.lookup_app(id).is_ok()
                {
                    return Ok(self.finish_launched_action(id, query));
                }
                Ok(json!({}))
            }
            "windowHere" => {
                let addr = require_non_empty(target, "target")?;
                hyprland::move_window_here(addr)?;
                Ok(json!({}))
            }
            "desktopAction" => {
                let id = require_non_empty(app_id, "appId")?;
                let action_id = require_non_empty(target, "target")?;
                let app = self.lookup_app(id)?;
                let prefs = self.current_prefs();
                launch::launch_desktop_action(&app, action_id, &prefs)?;
                Ok(self.finish_launched_action(&app.id, query))
            }
            "workspace" => {
                let id = require_non_empty(app_id, "appId")?;
                let ws = require_non_empty(target, "target")?;
                let app = self.lookup_app(id)?;
                let prefs = self.current_prefs();
                launch::launch_on_workspace(&app, ws, &prefs)?;
                Ok(self.finish_launched_action(&app.id, query))
            }
            "pin" => {
                let id = require_non_empty(app_id, "appId")?;
                let _ = self.lookup_app(id)?;
                let mut store = self.store.lock().unwrap();
                store.toggle_pin(id)?;
                Ok(json!({}))
            }
            "copyCommand" => {
                let id = require_non_empty(app_id, "appId")?;
                let app = self.lookup_app(id)?;
                let text = launch::format_copy_command(&app)?;
                Ok(json!({ "text": text }))
            }
            "edit" => {
                let id = require_non_empty(app_id, "appId")?;
                let app = self.lookup_app(id)?;
                launch::edit_desktop_entry(&app)?;
                self.catalog.lock().unwrap().force_reload();
                Ok(json!({}))
            }
            "info" => {
                let text = if let Some(pkg_id) = package_id.filter(|s| !s.is_empty()) {
                    let mut pkgs = self.packages.lock().unwrap();
                    pkgs.info_for_package(pkg_id, source)?
                } else if let Some(id) = app_id.filter(|s| !s.is_empty()) {
                    let app = self.lookup_app(id)?;
                    let mut pkgs = self.packages.lock().unwrap();
                    pkgs.info_for_app(&app)
                } else {
                    return Err("appId or packageId is required for info.".into());
                };
                Ok(json!({ "text": text }))
            }
            "quit" => {
                let id = require_non_empty(app_id, "appId")?;
                let app = self.lookup_app(id)?;
                hyprland::quit_app_windows(app.startup_wm_class.as_deref(), &app.id)?;
                Ok(json!({}))
            }
            "hide" => {
                let id = require_non_empty(app_id, "appId")?;
                let _ = self.lookup_app(id)?;
                let mut store = self.store.lock().unwrap();
                store.set_hidden(id, true)?;
                Ok(json!({}))
            }
            "unhide" => {
                let id = require_non_empty(app_id, "appId")?;
                let mut store = self.store.lock().unwrap();
                store.set_hidden(id, false)?;
                Ok(json!({}))
            }
            "install" => {
                let pkg_id = require_non_empty(package_id, "packageId")?;
                let prefs = self.current_prefs();
                let mut pkgs = self.packages.lock().unwrap();
                launch::install_package(pkg_id, source, &prefs, &mut pkgs)?;
                Ok(json!({}))
            }
            "web" => {
                launch::open_web_search(query)?;
                Ok(json!({}))
            }
            other => Err(format!("Unsupported action: {other}")),
        }
    }

    fn lookup_app(&self, id: &str) -> Result<crate::desktop::IndexedApp, String> {
        let mut catalog = self.catalog.lock().unwrap();
        catalog.refresh_if_needed();
        catalog
            .get(id)
            .cloned()
            .ok_or_else(|| format!("Application not found: {id}"))
    }

    fn current_prefs(&self) -> crate::protocol::Preferences {
        let mut store = self.store.lock().unwrap();
        store.refresh_if_changed();
        store.preferences().clone()
    }

    fn record_usage(&self, app_id: &str, query: &str) -> Result<(), String> {
        let mut store = self.store.lock().unwrap();
        store.record_usage(app_id, query, now_unix_secs())
    }

    fn finish_launched_action(&self, app_id: &str, query: &str) -> Value {
        match self.record_usage(app_id, query) {
            Ok(()) => json!({}),
            Err(err) => {
                // Report stats persistence failure separately after a successful
                // launch/focus so the caller does not falsely retry launching.
                eprintln!("Failed to persist usage statistics for {app_id}: {err}");
                json!({ "warning": err })
            }
        }
    }
}

fn require_non_empty<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, String> {
    value
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("Missing required field: {field}"))
}
