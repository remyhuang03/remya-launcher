use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::protocol::AppWindow;

const HYPRCTL_TIMEOUT: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone)]
pub struct HyprClient {
    pub address: String,
    pub class: String,
    pub initial_class: String,
    pub title: String,
    pub workspace: String,
    pub focus_history_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawClient {
    #[serde(default)]
    address: String,
    #[serde(default)]
    mapped: bool,
    #[serde(default)]
    class: String,
    #[serde(default)]
    initial_class: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    workspace: RawWorkspace,
    #[serde(default, rename = "focusHistoryID")]
    focus_history_id: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct RawWorkspace {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
}

pub fn is_valid_address(addr: &str) -> bool {
    let Some(hex) = addr.strip_prefix("0x") else {
        return false;
    };
    !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) && addr != "0x0"
}

pub fn parse_positive_workspace(target: &str) -> Result<u32, String> {
    let trimmed = target.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Workspace must be a positive integer.".into());
    }
    let num: u32 = trimmed
        .parse()
        .map_err(|_| "Workspace number is out of range.".to_owned())?;
    if num == 0 {
        return Err("Workspace must be a positive integer.".into());
    }
    Ok(num)
}

pub fn fetch_clients() -> Vec<HyprClient> {
    let Ok(output) = run_hyprctl(&["-j", "clients"]) else {
        return Vec::new();
    };
    let Ok(raw_clients) = serde_json::from_str::<Vec<RawClient>>(&output) else {
        return Vec::new();
    };
    let mut clients: Vec<HyprClient> = raw_clients
        .into_iter()
        .filter(|c| c.mapped && is_valid_address(&c.address))
        .map(|c| {
            let workspace = if !c.workspace.name.is_empty() {
                c.workspace.name
            } else {
                c.workspace.id.to_string()
            };
            HyprClient {
                address: c.address,
                class: c.class,
                initial_class: c.initial_class,
                title: c.title,
                workspace,
                focus_history_id: c.focus_history_id,
            }
        })
        .collect();
    clients.sort_by(|a, b| {
        mru_sort_key(a.focus_history_id)
            .cmp(&mru_sort_key(b.focus_history_id))
            .then_with(|| a.address.cmp(&b.address))
    });
    clients
}

fn mru_sort_key(focus_history_id: Option<i64>) -> (u8, i64) {
    match focus_history_id {
        Some(id) if id >= 0 => (0, id),
        _ => (1, i64::MAX),
    }
}

pub fn match_windows(
    startup_wm_class: Option<&str>,
    desktop_id: &str,
    clients: &[HyprClient],
) -> Vec<AppWindow> {
    let wm_class = startup_wm_class.filter(|s| !s.is_empty());
    let desktop_stem = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id);
    if wm_class.is_none() && desktop_stem.is_empty() {
        return Vec::new();
    }
    clients
        .iter()
        .filter(|c| {
            let matches_wm = wm_class.is_some_and(|wm| {
                c.class.eq_ignore_ascii_case(wm) || c.initial_class.eq_ignore_ascii_case(wm)
            });
            let matches_desktop = !desktop_stem.is_empty()
                && (c.class.eq_ignore_ascii_case(desktop_stem)
                    || c.initial_class.eq_ignore_ascii_case(desktop_stem)
                    || c.class.eq_ignore_ascii_case(desktop_id)
                    || c.initial_class.eq_ignore_ascii_case(desktop_id));
            matches_wm || matches_desktop
        })
        .map(HyprClient::to_app_window)
        .collect()
}

impl HyprClient {
    fn to_app_window(&self) -> AppWindow {
        AppWindow {
            id: self.address.clone(),
            title: self.title.clone(),
            workspace: self.workspace.clone(),
        }
    }
}

pub fn focus_known_window(address: &str) -> Result<(), String> {
    let address = known_address(address)?;
    dispatch_lua(
        &format!("hl.dsp.focus({{window=\"address:{address}\"}})"),
        "Could not focus window.",
    )
}

/// Returns the validated address of a currently open window.
fn known_address(address: &str) -> Result<String, String> {
    if !is_valid_address(address) {
        return Err("Invalid window address.".into());
    }
    fetch_clients()
        .into_iter()
        .find(|c| c.address.eq_ignore_ascii_case(address))
        .map(|c| c.address)
        .ok_or_else(|| "Window is no longer open.".to_owned())
}

pub fn move_window_here(address: &str) -> Result<(), String> {
    let address = known_address(address)?;
    // The launcher is a layer surface, so the active monitor's workspace is where the user is.
    dispatch_lua(
        &format!(
            "hl.dsp.window.move({{workspace=tostring(hl.get_active_monitor().active_workspace.id), window=\"address:{address}\"}})"
        ),
        "Could not move window.",
    )?;
    dispatch_lua(
        &format!("hl.dsp.focus({{window=\"address:{address}\"}})"),
        "Could not focus window.",
    )
}

pub fn quit_app_windows(startup_wm_class: Option<&str>, desktop_id: &str) -> Result<(), String> {
    let clients = fetch_clients();
    let windows = match_windows(startup_wm_class, desktop_id, &clients);
    if windows.is_empty() {
        return Err("Application has no open windows.".into());
    }
    for window in windows {
        if !is_valid_address(&window.id) {
            continue;
        }
        dispatch_lua(
            &format!("hl.dsp.window.close({{window=\"address:{}\"}})", window.id),
            "Could not close application window.",
        )?;
    }
    Ok(())
}

pub fn switch_workspace(workspace: u32) -> Result<(), String> {
    dispatch_lua(
        &format!("hl.dsp.focus({{workspace=\"{workspace}\"}})"),
        "Could not switch workspace.",
    )
}

fn dispatch_lua(action: &str, fallback: &str) -> Result<(), String> {
    let code = format!(
        "local result = hl.dispatch({action}); return result.ok and \"ok\" or (result.error or \"unknown Hyprland error\")"
    );
    let reply = run_hyprctl(&["repl", &code])?;
    check_dispatch_ok(&reply, fallback)
}

fn check_dispatch_ok(reply: &str, fallback: &str) -> Result<(), String> {
    let trimmed = reply.trim();
    if trimmed == "ok" {
        Ok(())
    } else if trimmed.is_empty() {
        Err(fallback.to_owned())
    } else {
        Err(format!("{fallback} ({trimmed})"))
    }
}

fn run_hyprctl(args: &[&str]) -> Result<String, String> {
    let mut child = Command::new("hyprctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Hyprland control is unavailable: {e}"))?;

    let mut stdout_pipe = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(ref mut out) = stdout_pipe {
            let _ = out.read_to_string(&mut buf);
        }
        buf
    });

    let deadline = Instant::now() + HYPRCTL_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = reader.join().unwrap_or_default();
                if status.success() {
                    return Ok(output);
                }
                return Err(format!("hyprctl failed with status {status}"));
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err("Hyprland did not respond in time.".into());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = reader.join();
                return Err(format!("Failed waiting for hyprctl: {e}"));
            }
        }
    }
}
