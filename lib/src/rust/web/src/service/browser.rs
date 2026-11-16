use std::env;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

use gio::DesktopAppInfo;
use gio::prelude::*;

use crate::browsers::{Family, Profile};

use super::protocol::ApiError;

#[derive(Clone, Copy)]
pub(super) struct BrowserDef {
    pub(super) browser_id: &'static str,
    pub(super) label: &'static str,
    pub(super) family: Family,
    pub(super) desktop_ids: &'static [&'static str],
    pub(super) private_flags: Option<&'static [&'static str]>,
}

const BROWSER_ALLOWLIST: &[BrowserDef] = &[
    BrowserDef {
        browser_id: "firefox",
        label: "Firefox",
        family: Family::Gecko,
        desktop_ids: &[
            "firefox.desktop",
            "org.mozilla.firefox.desktop",
            "firefox_firefox.desktop",
            "firefox-esr.desktop",
            "firefox-developer-edition.desktop",
            "firefox-nightly.desktop",
        ],
        private_flags: Some(&["--private-window"]),
    },
    BrowserDef {
        browser_id: "librewolf",
        label: "LibreWolf",
        family: Family::Gecko,
        desktop_ids: &["librewolf.desktop", "io.gitlab.librewolf-community.desktop"],
        private_flags: Some(&["--private-window"]),
    },
    BrowserDef {
        browser_id: "zen",
        label: "Zen Browser",
        family: Family::Gecko,
        desktop_ids: &[
            "zen.desktop",
            "zen-browser.desktop",
            "app.zen_browser.zen.desktop",
            "io.github.zen_browser.zen.desktop",
        ],
        private_flags: Some(&["--private-window"]),
    },
    BrowserDef {
        browser_id: "floorp",
        label: "Floorp",
        family: Family::Gecko,
        desktop_ids: &["floorp.desktop", "one.ablaze.floorp.desktop"],
        private_flags: Some(&["--private-window"]),
    },
    BrowserDef {
        browser_id: "waterfox",
        label: "Waterfox",
        family: Family::Gecko,
        desktop_ids: &["waterfox.desktop", "net.waterfox.waterfox.desktop"],
        private_flags: Some(&["--private-window"]),
    },
    BrowserDef {
        browser_id: "chromium",
        label: "Chromium",
        family: Family::Chromium,
        desktop_ids: &[
            "chromium.desktop",
            "org.chromium.Chromium.desktop",
            "chromium_chromium.desktop",
        ],
        private_flags: Some(&["--incognito"]),
    },
    BrowserDef {
        browser_id: "google-chrome",
        label: "Google Chrome",
        family: Family::Chromium,
        desktop_ids: &["google-chrome.desktop", "com.google.Chrome.desktop"],
        private_flags: Some(&["--incognito"]),
    },
    BrowserDef {
        browser_id: "brave",
        label: "Brave",
        family: Family::Chromium,
        desktop_ids: &[
            "brave-browser.desktop",
            "com.brave.Browser.desktop",
            "brave_brave.desktop",
        ],
        private_flags: Some(&["--incognito"]),
    },
    BrowserDef {
        browser_id: "microsoft-edge",
        label: "Microsoft Edge",
        family: Family::Chromium,
        desktop_ids: &["microsoft-edge.desktop", "com.microsoft.Edge.desktop"],
        private_flags: Some(&["--inprivate"]),
    },
    BrowserDef {
        browser_id: "vivaldi",
        label: "Vivaldi",
        family: Family::Chromium,
        desktop_ids: &[
            "vivaldi-stable.desktop",
            "vivaldi.desktop",
            "com.vivaldi.Vivaldi.desktop",
            "vivaldi_vivaldi-stable.desktop",
            "vivaldi_vivaldi.desktop",
        ],
        private_flags: Some(&["--incognito"]),
    },
    BrowserDef {
        browser_id: "opera",
        label: "Opera",
        family: Family::Chromium,
        desktop_ids: &[
            "opera.desktop",
            "com.opera.Opera.desktop",
            "opera_opera.desktop",
        ],
        private_flags: Some(&["--private"]),
    },
];

pub(super) struct InstalledBrowser {
    pub(super) def: &'static BrowserDef,
    pub(super) desktop_id: &'static str,
    pub(super) desktop_info: DesktopAppInfo,
    pub(super) base_argv: Vec<String>,
    pub(super) supports_profile_routing: bool,
    pub(super) supports_private: bool,
}

pub(super) fn detect_installed_browsers() -> Vec<InstalledBrowser> {
    let mut installed = Vec::new();
    for def in BROWSER_ALLOWLIST {
        for &desktop_id in def.desktop_ids {
            if let Some(entry) = resolve_desktop_entry(def, desktop_id) {
                installed.push(entry);
                break;
            }
        }
    }
    installed
}

pub(super) fn find_installed_browser<'a>(
    installed: &'a [InstalledBrowser],
    browser_id: &str,
) -> Option<&'a InstalledBrowser> {
    installed.iter().find(|b| b.def.browser_id == browser_id)
}

pub(super) fn default_uri_handler(scheme: &str) -> Option<gio::AppInfo> {
    gio::AppInfo::default_for_uri_scheme(scheme)
}

pub(super) fn detect_default_browser_private(installed: &[InstalledBrowser]) -> bool {
    let Some(default_info) = default_uri_handler("https").or_else(|| default_uri_handler("http"))
    else {
        return false;
    };
    let Some(default_id) = default_info.id() else {
        return false;
    };
    installed
        .iter()
        .any(|b| b.desktop_id == default_id.as_str() && b.supports_private)
}

fn resolve_desktop_entry(
    def: &'static BrowserDef,
    desktop_id: &'static str,
) -> Option<InstalledBrowser> {
    let info = DesktopAppInfo::new(desktop_id)?;
    if info.is_hidden() || info.is_nodisplay() {
        return None;
    }
    if !validate_desktop_try_exec(&info) {
        return None;
    }
    let exec_raw = info.string("Exec")?;
    let raw_tokens = tokenize_exec_raw(&exec_raw);
    let resolved_prog = validate_non_shell_exec(&raw_tokens, Some(&info))?;

    let dbus_activatable = info.boolean("DBusActivatable");
    let has_env_prefix = raw_tokens.first().is_some_and(|p| is_env_program(p));
    let has_wrapper = has_env_prefix
        || dbus_activatable
        || raw_tokens
            .first()
            .is_some_and(|p| is_sandbox_wrapper_program(p))
        || is_sandbox_wrapper_path(&resolved_prog)
        || has_wrapper_or_profile_conflict_args(&raw_tokens);

    let base_argv = expand_desktop_exec(&exec_raw);
    if base_argv.is_empty() {
        return None;
    }

    let supports_profile_routing = !has_wrapper;
    let supports_private = !has_wrapper && def.private_flags.is_some();

    Some(InstalledBrowser {
        def,
        desktop_id,
        desktop_info: info,
        base_argv,
        supports_profile_routing,
        supports_private,
    })
}

pub(super) fn dispatch_default_browser(
    installed: &[InstalledBrowser],
    url: &url::Url,
    explicit_profile: Option<&Profile>,
    is_private: bool,
) -> Result<(), ApiError> {
    let default_info = default_uri_handler(url.scheme())
        .or_else(|| default_uri_handler("https"))
        .or_else(|| default_uri_handler("http"))
        .ok_or_else(ApiError::browser_not_installed)?;

    let default_id = default_info.id().map(|g| g.to_string());
    let matched_allowlisted = default_id
        .as_deref()
        .and_then(|id| installed.iter().find(|b| b.desktop_id == id));

    if is_private || explicit_profile.is_some() {
        let Some(handler) = matched_allowlisted else {
            return Err(ApiError::operation_unsupported(
                "Default browser does not support requested private or profile dispatch.",
            ));
        };
        if let Some(prof) = explicit_profile
            && prof.browser_id != handler.def.browser_id
        {
            return Err(ApiError::operation_unsupported(
                "Requested profile does not belong to the default browser.",
            ));
        }
        return dispatch_allowlisted_browser(handler, url, explicit_profile, is_private);
    }

    // Standard non-private, non-profile default open via trusted GIO AppInfo.
    // Always verify non-shell executable before invoking GIO.
    validate_gio_app_non_shell(&default_info, default_id.as_deref())?;

    let ctx = gio::AppLaunchContext::new();
    default_info
        .launch_uris(&[url.as_str()], Some(&ctx))
        .map_err(|_| ApiError::internal("Failed to launch default browser."))
}

pub(super) fn dispatch_allowlisted_browser(
    handler: &InstalledBrowser,
    url: &url::Url,
    explicit_profile: Option<&Profile>,
    is_private: bool,
) -> Result<(), ApiError> {
    if let Some(profile) = explicit_profile {
        if profile.browser_id != handler.def.browser_id {
            return Err(ApiError::invalid_request(
                "Profile does not belong to the selected browser.",
            ));
        }
        if !handler.supports_profile_routing {
            return Err(ApiError::operation_unsupported(
                "Profile routing is not reliably supported for this packaged browser.",
            ));
        }
    }

    if is_private && !handler.supports_private {
        return Err(ApiError::operation_unsupported(
            "Private window dispatch is not supported for this browser.",
        ));
    }

    // Normal browser open without explicit profile or private mode uses the trusted GIO handler,
    // which safely supports native, Flatpak, Snap, and DBusActivatable desktop entries.
    if explicit_profile.is_none() && !is_private {
        let ctx = gio::AppLaunchContext::new();
        return handler
            .desktop_info
            .launch_uris(&[url.as_str()], Some(&ctx))
            .map_err(|_| ApiError::internal("Failed to launch browser."));
    }

    let mut argv = handler.base_argv.clone();
    if let Some(profile) = explicit_profile {
        let profile_args = build_profile_routing_args(handler.def.family, profile)?;
        argv.extend(profile_args);
    }

    if is_private {
        let Some(flags) = handler.def.private_flags else {
            return Err(ApiError::operation_unsupported(
                "Private window dispatch is not supported for this browser.",
            ));
        };
        argv.extend(flags.iter().map(|s| (*s).to_owned()));
    }

    argv.push(url.as_str().to_owned());
    spawn_detached(&argv)
}

fn build_profile_routing_args(family: Family, profile: &Profile) -> Result<Vec<String>, ApiError> {
    let meta =
        fs::symlink_metadata(&profile.path).map_err(|_| ApiError::profile_permission_denied())?;
    if !meta.is_dir() {
        return Err(ApiError::profile_permission_denied());
    }
    let path_str = profile.path.to_str().ok_or_else(|| {
        ApiError::operation_unsupported("Profile path is not valid UTF-8 for CLI routing.")
    })?;
    if path_str.is_empty() || path_str.contains('\0') {
        return Err(ApiError::operation_unsupported(
            "Profile path cannot be routed safely.",
        ));
    }

    match family {
        Family::Gecko => Ok(vec!["--profile".to_owned(), path_str.to_owned()]),
        Family::Chromium => {
            let parent = profile
                .path
                .parent()
                .and_then(|p| p.to_str())
                .filter(|s| !s.is_empty() && !s.contains('\0'))
                .ok_or_else(|| {
                    ApiError::operation_unsupported(
                        "Chromium user data directory could not be resolved.",
                    )
                })?;
            let dir_name = profile
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .filter(|s| !s.is_empty() && !s.contains('\0'))
                .ok_or_else(|| {
                    ApiError::operation_unsupported(
                        "Chromium profile directory name could not be resolved.",
                    )
                })?;
            Ok(vec![
                format!("--user-data-dir={parent}"),
                format!("--profile-directory={dir_name}"),
            ])
        }
    }
}

fn spawn_detached(argv: &[String]) -> Result<(), ApiError> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(ApiError::browser_not_installed)?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = cmd
        .spawn()
        .map_err(|_| ApiError::internal("Failed to start browser process."))?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn expand_desktop_exec(exec_raw: &str) -> Vec<String> {
    let tokens = tokenize_exec_raw(exec_raw);
    let mut out = Vec::with_capacity(tokens.len());
    for tok in tokens {
        if matches!(
            tok.as_str(),
            "%f" | "%F"
                | "%u"
                | "%U"
                | "%d"
                | "%D"
                | "%n"
                | "%N"
                | "%v"
                | "%m"
                | "%i"
                | "%c"
                | "%k"
                | "@@u"
                | "@@"
        ) {
            continue;
        }
        let cleaned = tok
            .replace("%u", "")
            .replace("%U", "")
            .replace("%f", "")
            .replace("%F", "")
            .replace("%%", "%");
        if !cleaned.is_empty() {
            out.push(cleaned);
        }
    }
    out
}

fn tokenize_exec_raw(raw: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = raw.chars();

    while let Some(ch) = chars.next() {
        match quote {
            Some('"') => {
                if ch == '"' {
                    quote = None;
                } else if ch == '\\' {
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    current.push(ch);
                }
            }
            Some('\'') => {
                if ch == '\'' {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            Some(_) => unreachable!(),
            None => {
                if ch.is_ascii_whitespace() {
                    if in_token {
                        tokens.push(std::mem::take(&mut current));
                        in_token = false;
                    }
                } else if ch == '"' || ch == '\'' {
                    quote = Some(ch);
                    in_token = true;
                } else if ch == '\\' {
                    in_token = true;
                    if let Some(next) = chars.next() {
                        current.push(next);
                    }
                } else {
                    in_token = true;
                    current.push(ch);
                }
            }
        }
    }
    if in_token {
        tokens.push(current);
    }
    tokens
}

fn validate_desktop_try_exec(info: &DesktopAppInfo) -> bool {
    if let Some(try_exec) = info.string("TryExec") {
        let trimmed = try_exec.trim();
        if !trimmed.is_empty() {
            return !is_shell_program(trimmed) && resolve_executable(trimmed).is_some();
        }
    }
    true
}

fn validate_non_shell_exec(
    raw_tokens: &[String],
    desktop_info: Option<&DesktopAppInfo>,
) -> Option<PathBuf> {
    if let Some(info) = desktop_info {
        let exe = info.executable();
        let exe_str = exe.to_string_lossy();
        if !exe_str.is_empty() && is_shell_program(&exe_str) {
            return None;
        }
    }
    let first = raw_tokens.first()?;
    if is_shell_program(first) {
        return None;
    }
    let unwrapped = unwrap_env_prefix(raw_tokens)?;
    let program = unwrapped.first()?;
    if is_shell_program(program) {
        return None;
    }
    resolve_executable(program)
}

fn validate_gio_app_non_shell(
    app_info: &gio::AppInfo,
    desktop_id: Option<&str>,
) -> Result<(), ApiError> {
    let exe = app_info.executable();
    let exe_str = exe.to_string_lossy();
    if !exe_str.is_empty() && is_shell_program(&exe_str) {
        return Err(ApiError::browser_not_installed());
    }

    let desktop_info = app_info
        .downcast_ref::<DesktopAppInfo>()
        .cloned()
        .or_else(|| desktop_id.and_then(DesktopAppInfo::new));

    if let Some(ref info) = desktop_info {
        if !validate_desktop_try_exec(info) {
            return Err(ApiError::browser_not_installed());
        }
        if let Some(exec_raw) = info.string("Exec") {
            let tokens = tokenize_exec_raw(&exec_raw);
            if validate_non_shell_exec(&tokens, Some(info)).is_none() {
                return Err(ApiError::browser_not_installed());
            }
            return Ok(());
        }
    }

    if let Some(cmdline) = app_info.commandline() {
        let tokens = tokenize_exec_raw(&cmdline.to_string_lossy());
        if validate_non_shell_exec(&tokens, desktop_info.as_ref()).is_none() {
            return Err(ApiError::browser_not_installed());
        }
        return Ok(());
    }

    if exe_str.is_empty() || resolve_executable(&exe_str).is_none() {
        return Err(ApiError::browser_not_installed());
    }
    Ok(())
}

fn unwrap_env_prefix(tokens: &[String]) -> Option<&[String]> {
    let mut i = 0;
    while i < tokens.len() && is_env_program(&tokens[i]) {
        i += 1;
        while i < tokens.len() {
            let tok = tokens[i].as_str();
            if tok == "--" {
                i += 1;
                break;
            }
            if matches!(tok, "-S" | "--split-string") || tok.starts_with("--split-string=") {
                return None;
            }
            if matches!(tok, "-u" | "--unset" | "-C" | "--chdir") {
                i += 2;
                continue;
            }
            if tok.starts_with('-') || is_env_assignment(tok) {
                i += 1;
                continue;
            }
            break;
        }
    }
    (i < tokens.len()).then(|| &tokens[i..])
}

fn is_env_assignment(tok: &str) -> bool {
    let Some((name, _)) = tok.split_once('=') else {
        return false;
    };
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn is_env_program(program: &str) -> bool {
    let base = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(program);
    base == "env"
}

fn is_shell_program(program: &str) -> bool {
    let base = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(program);
    matches!(
        base,
        "sh" | "bash"
            | "dash"
            | "zsh"
            | "fish"
            | "ksh"
            | "ash"
            | "csh"
            | "tcsh"
            | "mksh"
            | "pdksh"
            | "oksh"
            | "yash"
    )
}

fn is_sandbox_wrapper_program(program: &str) -> bool {
    let base = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(program);
    matches!(
        base,
        "flatpak"
            | "flatpak-spawn"
            | "snap"
            | "bwrap"
            | "firejail"
            | "xdg-open"
            | "gio"
            | "gtk-launch"
            | "distrobox"
            | "distrobox-enter"
            | "toolbox"
    )
}

fn is_sandbox_wrapper_path(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.starts_with("/snap/")
        || s.contains("/snap/bin/")
        || s.contains("/var/lib/snapd/")
        || s.contains("/flatpak/exports/bin/")
        || s.contains("/.local/share/flatpak/")
}

fn has_wrapper_or_profile_conflict_args(tokens: &[String]) -> bool {
    tokens.iter().any(|tok| {
        matches!(
            tok.as_str(),
            "@@" | "@@u"
                | "--profile"
                | "-P"
                | "--profile-directory"
                | "--user-data-dir"
                | "--new-instance"
                | "-no-remote"
                | "--no-remote"
        ) || tok.starts_with("--profile=")
            || tok.starts_with("--profile-directory=")
            || tok.starts_with("--user-data-dir=")
    })
}

fn resolve_executable(program: &str) -> Option<PathBuf> {
    if program.is_empty() || program.contains('\0') {
        return None;
    }
    let path = Path::new(program);
    if program.contains('/') {
        return is_executable_file(path).then(|| path.to_path_buf());
    }
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(program);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    meta.is_file() && (meta.permissions().mode() & 0o111 != 0)
}
