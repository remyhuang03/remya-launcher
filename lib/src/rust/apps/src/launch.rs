use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use gio::DesktopAppInfo;
use gio::prelude::*;

use crate::desktop::{IndexedApp, tokenize_exec_raw, user_applications_dir};
use crate::hyprland;
use crate::packages::{PackageCache, command_exists_in_path, is_executable_file};
use crate::protocol::Preferences;

static EDIT_STAGING_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn format_copy_command(app: &IndexedApp) -> Result<String, String> {
    let argv = expand_exec_argv(
        &app.exec_raw,
        app.icon.as_deref(),
        &app.name,
        &app.desktop_path,
    );
    if argv.is_empty() {
        return Err("Application has no Exec command.".into());
    }
    Ok(argv
        .into_iter()
        .map(|arg| shell_quote_if_needed(&arg))
        .collect::<Vec<_>>()
        .join(" "))
}

pub fn launch_app(app: &IndexedApp, prefs: &Preferences) -> Result<(), String> {
    validate_launchable(app, None)?;
    if app.terminal {
        let argv = expand_exec_argv(
            &app.exec_raw,
            app.icon.as_deref(),
            &app.name,
            &app.desktop_path,
        );
        return launch_in_terminal(&argv, prefs);
    }
    let info = DesktopAppInfo::from_filename(&app.desktop_path)
        .ok_or_else(|| format!("Desktop entry could not be loaded: {}", app.id))?;
    let ctx = gio::AppLaunchContext::new();
    info.launch(&[], Some(&ctx))
        .map_err(|e| format!("Failed to launch {}: {e}", app.name))
}

pub fn launch_desktop_action(
    app: &IndexedApp,
    action_id: &str,
    prefs: &Preferences,
) -> Result<(), String> {
    let action = app
        .actions
        .iter()
        .find(|a| a.id == action_id)
        .ok_or_else(|| format!("Unknown desktop action: {action_id}"))?;
    let override_exec = (!action.exec_raw.is_empty()).then_some(action.exec_raw.as_str());
    validate_launchable(app, override_exec)?;

    if app.terminal {
        let raw = override_exec.unwrap_or(&app.exec_raw);
        let argv = expand_exec_argv(raw, app.icon.as_deref(), &app.name, &app.desktop_path);
        return launch_in_terminal(&argv, prefs);
    }

    let info = DesktopAppInfo::from_filename(&app.desktop_path)
        .ok_or_else(|| format!("Desktop entry could not be loaded: {}", app.id))?;
    let ctx = gio::AppLaunchContext::new();
    info.launch_action(action_id, Some(&ctx));
    Ok(())
}

pub fn launch_new_instance(app: &IndexedApp, prefs: &Preferences) -> Result<(), String> {
    if let Some(action) = select_new_window_action(app) {
        return launch_desktop_action(app, action, prefs);
    }
    launch_app(app, prefs)
}

fn select_new_window_action(app: &IndexedApp) -> Option<&str> {
    let is_excluded = |id_lower: &str, name_lower: &str| {
        id_lower.contains("private")
            || id_lower.contains("incognito")
            || name_lower.contains("private")
            || name_lower.contains("incognito")
    };

    for canonical in ["new-window", "newwindow", "new_window"] {
        if let Some(action) = app
            .actions
            .iter()
            .find(|a| a.id.eq_ignore_ascii_case(canonical))
        {
            return Some(action.id.as_str());
        }
    }

    for action in &app.actions {
        let id_lower = action.id.to_ascii_lowercase();
        if is_excluded(&id_lower, &action.name_lower) {
            continue;
        }
        let alnum: String = id_lower
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect();
        if alnum == "newwindow"
            || action.name_lower.trim() == "new window"
            || alnum.starts_with("newwindow")
            || alnum.ends_with("newwindow")
        {
            return Some(action.id.as_str());
        }
    }
    None
}

pub fn launch_on_workspace(
    app: &IndexedApp,
    target: &str,
    prefs: &Preferences,
) -> Result<(), String> {
    let workspace = hyprland::parse_positive_workspace(target)?;
    validate_launchable(app, None)?;
    hyprland::switch_workspace(workspace)?;
    launch_app(app, prefs)
}

pub fn edit_desktop_entry(app: &IndexedApp) -> Result<PathBuf, String> {
    if app.id.is_empty() || app.id.contains('/') || app.id.contains("..") {
        return Err("Invalid desktop entry identifier.".into());
    }
    let user_dir = user_applications_dir()
        .ok_or_else(|| "User applications directory is unavailable.".to_owned())?;
    fs::create_dir_all(&user_dir)
        .map_err(|e| format!("Failed to prepare user applications directory: {e}"))?;
    let dest = user_dir.join(&app.id);
    if app.desktop_path != dest && fs::symlink_metadata(&dest).is_err() {
        copy_no_clobber(&app.desktop_path, &user_dir, &dest)?;
    }

    let ctx = gio::AppLaunchContext::new();
    let file = gio::File::for_path(&dest);
    if let Some(editor) = gio::AppInfo::default_for_type("text/plain", false)
        && editor.launch(&[file], Some(&ctx)).is_ok()
    {
        return Ok(dest);
    }

    // Fallback to $VISUAL if no default text/plain desktop handler is registered.
    if let Some(visual) = env::var_os("VISUAL").filter(|v| !v.is_empty()) {
        let bin = visual.to_string_lossy().into_owned();
        let tokens = tokenize_exec_raw(&bin);
        if !tokens.is_empty() && resolve_executable(&tokens[0]).is_some() {
            let mut cmd_argv = tokens;
            cmd_argv.push(dest.to_string_lossy().into_owned());
            spawn_detached(&cmd_argv)?;
            return Ok(dest);
        }
    }

    Err(format!(
        "No text/plain desktop editor is configured to open {}.",
        dest.display()
    ))
}

fn copy_no_clobber(src: &Path, user_dir: &Path, dest: &Path) -> Result<(), String> {
    let bytes =
        fs::read(src).map_err(|e| format!("Failed to read desktop entry for editing: {e}"))?;
    let seq = EDIT_STAGING_SEQ.fetch_add(1, Ordering::Relaxed);
    let staging = user_dir.join(format!(".remya-edit-{}-{seq}.tmp", std::process::id()));
    let mut staged = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|e| format!("Failed to stage desktop entry for editing: {e}"))?;
    let write_res = staged.write_all(&bytes).and_then(|()| staged.sync_all());
    drop(staged);
    if let Err(e) = write_res {
        let _ = fs::remove_file(&staging);
        return Err(format!("Failed to write desktop entry for editing: {e}"));
    }
    let link_res = fs::hard_link(&staging, dest);
    let _ = fs::remove_file(&staging);
    match link_res {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(format!("Failed to copy desktop entry for editing: {e}")),
    }
}

pub fn open_web_search(query: &str) -> Result<(), String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("Search query is empty.".into());
    }
    let encoded = percent_encode(trimmed);
    let url = format!("https://duckduckgo.com/?q={encoded}");
    let ctx = gio::AppLaunchContext::new();
    gio::AppInfo::launch_default_for_uri(&url, Some(&ctx))
        .map_err(|e| format!("Failed to open browser: {e}"))
}

pub fn install_package(
    package_id: &str,
    source: Option<&str>,
    prefs: &Preferences,
    packages: &mut PackageCache,
) -> Result<(), String> {
    let install_argv = packages.resolve_install_command(package_id, source)?;
    launch_in_terminal(&install_argv, prefs)
}

pub fn resolve_terminal_argv(configured: &str) -> Result<Vec<String>, String> {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        let mut tokens = tokenize_exec_raw(trimmed);
        if tokens.is_empty() {
            return Err("Configured terminal command is empty.".into());
        }
        if resolve_executable(&tokens[0]).is_none() {
            return Err(format!("Configured terminal not found: {}", tokens[0]));
        }
        if tokens.len() == 1 {
            append_terminal_exec_flag(&mut tokens);
        }
        return Ok(tokens);
    }

    if let Ok(env_term) = env::var("TERMINAL") {
        let env_trimmed = env_term.trim();
        if !env_trimmed.is_empty() {
            let mut tokens = tokenize_exec_raw(env_trimmed);
            if !tokens.is_empty() && resolve_executable(&tokens[0]).is_some() {
                if tokens.len() == 1 {
                    append_terminal_exec_flag(&mut tokens);
                }
                return Ok(tokens);
            }
        }
    }

    const CANDIDATES: &[&str] = &[
        "xdg-terminal-exec",
        "kitty",
        "foot",
        "alacritty",
        "ghostty",
        "wezterm",
        "konsole",
        "gnome-terminal",
        "ptyxis",
        "xfce4-terminal",
        "xterm",
    ];
    for candidate in CANDIDATES {
        if command_exists_in_path(candidate) {
            let mut tokens = vec![(*candidate).to_owned()];
            append_terminal_exec_flag(&mut tokens);
            return Ok(tokens);
        }
    }

    Err("No terminal emulator configured or found.".into())
}

fn append_terminal_exec_flag(tokens: &mut Vec<String>) {
    let basename = Path::new(&tokens[0])
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&tokens[0])
        .to_ascii_lowercase();
    match basename.as_str() {
        "wezterm" => {
            tokens.push("start".into());
            tokens.push("--".into());
        }
        "gnome-terminal" | "ptyxis" | "kgx" => {
            tokens.push("--".into());
        }
        "alacritty" | "ghostty" | "konsole" | "xfce4-terminal" | "terminator" | "tilix"
        | "xterm" | "urxvt" => {
            tokens.push("-e".into());
        }
        _ => {}
    }
}

fn launch_in_terminal(command_argv: &[String], prefs: &Preferences) -> Result<(), String> {
    if command_argv.is_empty() {
        return Err("Launch command is empty.".into());
    }
    let mut full_argv = resolve_terminal_argv(&prefs.terminal)?;
    full_argv.extend(command_argv.iter().cloned());
    spawn_detached(&full_argv)
}

fn spawn_detached(argv: &[String]) -> Result<(), String> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| "Command is empty.".to_owned())?;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start {program}: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn validate_launchable(app: &IndexedApp, override_exec: Option<&str>) -> Result<(), String> {
    if let Some(try_exec) = app.try_exec.as_deref()
        && resolve_executable(try_exec).is_none()
    {
        return Err(format!("TryExec failed: {try_exec} is not executable."));
    }
    let exec_raw = override_exec.unwrap_or(&app.exec_raw);
    let tokens = tokenize_exec_raw(exec_raw);
    let bin = find_exec_program(&tokens)
        .ok_or_else(|| format!("Application {} has no valid Exec command.", app.name))?;
    if resolve_executable(bin).is_none() {
        return Err(format!("Executable not found: {bin}"));
    }
    Ok(())
}

fn find_exec_program(tokens: &[String]) -> Option<&str> {
    let mut iter = tokens.iter().map(String::as_str);
    while let Some(tok) = iter.next() {
        if tok.is_empty() {
            continue;
        }
        if tok == "env" {
            for next in iter.by_ref() {
                if !next.contains('=') && !next.starts_with('-') {
                    return Some(next);
                }
            }
            return None;
        }
        if tok.contains('=') && !tok.starts_with('/') && !tok.starts_with('.') {
            continue;
        }
        return Some(tok);
    }
    None
}

pub fn resolve_executable(program: &str) -> Option<PathBuf> {
    if program.is_empty() {
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

pub fn expand_exec_argv(
    exec_raw: &str,
    icon: Option<&str>,
    name: &str,
    desktop_path: &Path,
) -> Vec<String> {
    let raw_tokens = tokenize_exec_raw(exec_raw);
    let mut out = Vec::with_capacity(raw_tokens.len());
    let desktop_str = desktop_path.to_string_lossy();

    for token in raw_tokens {
        if token == "%i" {
            if let Some(icon_val) = icon.filter(|s| !s.is_empty()) {
                out.push("--icon".into());
                out.push(icon_val.to_owned());
            }
            continue;
        }
        if matches!(
            token.as_str(),
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m"
        ) {
            continue;
        }

        let mut expanded = String::with_capacity(token.len());
        let mut chars = token.chars();
        while let Some(ch) = chars.next() {
            if ch != '%' {
                expanded.push(ch);
                continue;
            }
            match chars.next() {
                Some('%') => expanded.push('%'),
                Some('c') => expanded.push_str(name),
                Some('k') => expanded.push_str(&desktop_str),
                Some('f' | 'F' | 'u' | 'U' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm' | 'i') => {}
                Some(other) => {
                    expanded.push('%');
                    expanded.push(other);
                }
                None => expanded.push('%'),
            }
        }
        if !expanded.is_empty() {
            out.push(expanded);
        }
    }
    out
}

fn shell_quote_if_needed(token: &str) -> String {
    if token.is_empty() {
        return "''".into();
    }
    let safe = token.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_' | b':' | b'+' | b'@')
    });
    if safe {
        return token.to_owned();
    }
    let mut quoted = String::with_capacity(token.len() + 2);
    quoted.push('\'');
    for ch in token.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for &byte in input.as_bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else if byte == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
