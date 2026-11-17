use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::process::{ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::{OFlags, fcntl_setfl};
use rustix::process::{Pid, Signal, kill_process_group};
use serde::Deserialize;

use crate::protocol::AppWindow;

const HYPRCTL_TIMEOUT: Duration = Duration::from_millis(1500);
const MAX_HYPRCTL_REPLY: usize = 8 * 1024 * 1024;
const HYPRCTL_TIMEOUT_MESSAGE: &str = "Hyprland did not respond in time.";

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

pub fn fetch_clients() -> Result<Vec<HyprClient>, String> {
    let output = run_hyprctl(&["-j", "clients"])?;
    let raw_clients = serde_json::from_str::<Vec<RawClient>>(&output)
        .map_err(|_| "Hyprland returned an invalid window list.")?;
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
    Ok(clients)
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
    fetch_clients()?
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
    let clients = fetch_clients()?;
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
    let deadline = Instant::now() + HYPRCTL_TIMEOUT;
    let mut child = Command::new("hyprctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("Hyprland control is unavailable: {e}"))?;

    let result = (|| {
        let stdout = child.stdout.take().expect("piped hyprctl stdout");
        let output = read_reply(stdout, deadline).map_err(|error| match error.kind() {
            io::ErrorKind::TimedOut => HYPRCTL_TIMEOUT_MESSAGE.to_owned(),
            _ => format!("Could not read Hyprland response: {error}"),
        })?;
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok((status, output)),
                Ok(None) if Instant::now() >= deadline => {
                    return Err(HYPRCTL_TIMEOUT_MESSAGE.into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => return Err(format!("Failed waiting for hyprctl: {e}")),
            }
        }
    })();
    if result.is_err() {
        let _ = kill_process_group(Pid::from_child(&child), Signal::KILL);
        let _ = child.wait();
    }
    let (status, output) = result?;
    status
        .success()
        .then_some(output)
        .ok_or_else(|| format!("hyprctl failed with status {status}"))
}

fn read_reply(mut stdout: ChildStdout, deadline: Instant) -> io::Result<String> {
    fcntl_setfl(&stdout, OFlags::NONBLOCK)?;
    let mut reply = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) if reply.len() + n <= MAX_HYPRCTL_REPLY => reply.extend_from_slice(&chunk[..n]),
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "response is too large",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                let mut fds = [PollFd::new(&stdout, PollFlags::IN)];
                let timeout = Timespec::try_from(remaining).expect("bounded duration");
                match poll(&mut fds, Some(&timeout)) {
                    Ok(_) | Err(rustix::io::Errno::INTR) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    String::from_utf8(reply).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
