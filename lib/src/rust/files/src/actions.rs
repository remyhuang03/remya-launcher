use std::ffi::OsStr;
use std::fs::{self, File, Metadata};
use std::io::{Seek, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gio::prelude::*;
use rustix::fs::{
    MemfdFlags, Mode, OFlags, RenameFlags, memfd_create, mkdirat, openat, renameat_with,
};
use serde_json::{Value, json};

mod trash;

use crate::model::path_id;
pub use crate::preview::version;
use crate::preview::{checked_file, fd_path};

type Outcome = Result<Value, String>;

#[derive(Default)]
pub struct Actions {
    sequence: u64,
    undo: Option<Undo>,
}

struct Undo {
    id: String,
    expires: Instant,
    path: PathBuf,
    version: String,
    original: Option<PathBuf>,
    trash_item: Option<trash::Item>,
}

fn verified(path: &Path, expected: &str) -> Result<Metadata, String> {
    fs::symlink_metadata(path)
        .ok()
        .filter(|m| version(m) == expected)
        .ok_or_else(|| "File changed. Refresh and try again.".into())
}

fn name_param(params: &Value) -> Result<&str, String> {
    params["name"]
        .as_str()
        .filter(|s| !s.is_empty() && !s.contains('\0'))
        .ok_or_else(|| "Enter a name.".into())
}

fn confirm(params: &Value) -> Result<(), String> {
    if params["confirmed"] == true {
        Ok(())
    } else {
        Err("Confirmation required.".into())
    }
}

impl Actions {
    pub fn perform(
        &mut self,
        path: &Path,
        params: &Value,
        allowed: impl Fn(&Path) -> bool,
    ) -> Outcome {
        let action = params["action"].as_str().unwrap_or_default();
        if action == "undo" {
            let undo = self
                .undo
                .as_ref()
                .filter(|u| {
                    Some(u.id.as_str()) == params["undoId"].as_str() && u.expires > Instant::now()
                })
                .ok_or("Undo expired.")?;
            if !allowed(&undo.path) {
                return Err("Path excluded by Files settings.".into());
            }
            if let Some(item) = &undo.trash_item {
                item.restore(&undo.path, &allowed)?;
                let result =
                    json!({"path":undo.path.to_string_lossy(),"entryId":path_id(&undo.path)});
                self.undo = None;
                return Ok(result);
            }
            let meta = verified(&undo.path, &undo.version)?;
            if let Some(original) = &undo.original {
                rename(&undo.path, original, &undo.version, &allowed)?;
                let result = json!({"path":original.to_string_lossy(),"entryId":path_id(original),"previousEntryId":path_id(&undo.path)});
                self.undo = None;
                return Ok(result);
            }
            if !(meta.is_file() && meta.len() == 0)
                && !(meta.is_dir()
                    && fs::read_dir(&undo.path).is_ok_and(|mut d| d.next().is_none()))
            {
                return Err("Created item is no longer empty.".into());
            }
            gio::File::for_path(&undo.path)
                .trash(gio::Cancellable::NONE)
                .map_err(|e| e.to_string())?;
            self.undo = None;
            return Ok(json!({}));
        }
        if !path.is_absolute() || !allowed(path) {
            return Err("Path excluded by Files settings.".into());
        }
        let expected = params["version"]
            .as_str()
            .ok_or("Refresh to verify this item.")?;
        verified(path, expected)?;
        let settings = &params["preferences"]["files-actions"];
        match action {
            "rename" => {
                let name = name_param(params)?;
                if !matches!(
                    Path::new(name).components().next(),
                    Some(Component::Normal(_))
                ) || name.contains('/')
                {
                    return Err("Use one file name.".into());
                }
                let dest = path.with_file_name(name);
                let moved = rename(path, &dest, expected, &allowed)?;
                let mut result = self.changed(&dest, Some(path.to_owned()), Some(moved));
                result["previousEntryId"] = path_id(path).into();
                Ok(result)
            }
            "createFile" | "createDirectory" => {
                let (dest, created_version) = create(
                    path,
                    name_param(params)?,
                    action == "createDirectory",
                    expected,
                    &allowed,
                )?;
                Ok(self.changed(&dest, None, created_version))
            }
            "trash" => {
                confirm(params)?;
                let (parent, source) = trash::pin(path, expected, &allowed)?;
                gio::File::for_path(path)
                    .trash(gio::Cancellable::NONE)
                    .map_err(|e| e.to_string())?;
                self.undo = None;
                let mut result = json!({});
                let root = gio::glib::user_data_dir().join("Trash");
                if let Some(item) = trash::Item::locate(&root, path, parent, source, &allowed) {
                    result["undoId"] = self.offer(path, None, String::new(), Some(item)).into();
                }
                Ok(result)
            }
            "copyPath" => Ok(
                json!({"path":path.to_str().ok_or("Path is not valid UTF-8. Use Copy file instead.")?}),
            ),
            "copyFile" => {
                let uri = gio::File::for_path(path).uri();
                let mut data = File::from(
                    memfd_create("clipboard-uri", MemfdFlags::CLOEXEC)
                        .map_err(|e| e.to_string())?,
                );
                write!(data, "{uri}\r\n")
                    .and_then(|()| data.rewind())
                    .map_err(|e| e.to_string())?;
                let mut child = Command::new("wl-copy")
                    .args(["--type", "text/uri-list"])
                    .stdin(data)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                let deadline = Instant::now() + Duration::from_secs(2);
                let status = loop {
                    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                        break status;
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err("Wayland clipboard timed out.".into());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                };
                if !status.success() {
                    return Err("Wayland clipboard rejected the file.".into());
                }
                Ok(json!({}))
            }
            _ => desktop_action(path, action, params, settings),
        }
    }

    fn changed(
        &mut self,
        path: &Path,
        original: Option<PathBuf>,
        created_version: Option<String>,
    ) -> Value {
        let mut result = json!({"entryId":path_id(path),"path":path.to_string_lossy()});
        self.undo = None;
        if let Some(version) = created_version {
            result["undoId"] = self.offer(path, original, version, None).into();
        }
        result
    }

    fn offer(
        &mut self,
        path: &Path,
        original: Option<PathBuf>,
        version: String,
        trash_item: Option<trash::Item>,
    ) -> String {
        self.sequence += 1;
        let id = format!("{}-{}", std::process::id(), self.sequence);
        self.undo = Some(Undo {
            id: id.clone(),
            expires: Instant::now() + Duration::from_secs(5),
            path: path.to_owned(),
            original,
            version,
            trash_item,
        });
        id
    }
}

fn rename(
    path: &Path,
    dest: &Path,
    expected: &str,
    allowed: &impl Fn(&Path) -> bool,
) -> Result<String, String> {
    if !allowed(dest) || path.parent() != dest.parent() {
        return Err("Destination excluded or outside parent directory.".into());
    }
    let (parent, source) = trash::pin(path, expected, allowed)?;
    let old = path.file_name().ok_or("Missing source name.")?;
    let new = dest.file_name().ok_or("Missing destination name.")?;
    let pinned = PathBuf::from(format!("/proc/self/fd/{}", parent.as_raw_fd()));
    let source_meta = source.metadata().map_err(|e| e.to_string())?;
    renameat_with(&parent, old, &parent, new, RenameFlags::NOREPLACE).map_err(|e| e.to_string())?;
    let renamed = fs::symlink_metadata(pinned.join(new)).map_err(|e| e.to_string())?;
    if (source_meta.dev(), source_meta.ino()) != (renamed.dev(), renamed.ino()) {
        return Err("Source changed during rename. Check the directory before continuing.".into());
    }
    Ok(version(&renamed))
}

fn create(
    parent: &Path,
    name: &str,
    directory: bool,
    expected: &str,
    allowed: &impl Fn(&Path) -> bool,
) -> Result<(PathBuf, Option<String>), String> {
    let parts: Vec<_> = name.split('/').collect();
    if parts.len() > 64
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return Err("Enter a relative path with valid names.".into());
    }
    let destination = parent.join(name);
    if !allowed(&destination) {
        return Err("Destination excluded by Files settings.".into());
    }
    let mut current = checked_file(parent, OFlags::PATH | OFlags::DIRECTORY, allowed)?;
    if version(&current.metadata().map_err(|e| e.to_string())?) != expected {
        return Err("Parent changed. Refresh and try again.".into());
    }
    let mut created_parent = None;
    let mut created_version = None;
    let action = (|| -> Result<(), String> {
        for (i, part) in parts.iter().enumerate() {
            let next = fd_path(&current)?.join(part);
            if !allowed(&next) {
                return Err("Destination excluded by Files settings.".into());
            }
            if i == parts.len() - 1 && !directory {
                let file = File::from(
                    openat(
                        &current,
                        *part,
                        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC,
                        Mode::from_raw_mode(0o666),
                    )
                    .map_err(|e| e.to_string())?,
                );
                created_version = file.metadata().ok().map(|m| version(&m));
            } else {
                match mkdirat(&current, *part, Mode::from_raw_mode(0o777)) {
                    Ok(()) => {
                        if i + 1 < parts.len() {
                            created_parent = Some(next.clone());
                        }
                    }
                    Err(rustix::io::Errno::EXIST) if i + 1 < parts.len() => (),
                    Err(e) => return Err(e.to_string()),
                }
                current = File::from(
                    openat(
                        &current,
                        *part,
                        OFlags::PATH | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                        Mode::empty(),
                    )
                    .map_err(|e| e.to_string())?,
                );
                if i + 1 == parts.len() {
                    created_version = current.metadata().ok().map(|m| version(&m));
                }
            }
        }
        Ok(())
    })();
    if let Err(e) = action {
        return Err(match created_parent {
            Some(p) => format!("{e}. Created directories through {}.", p.display()),
            None => e,
        });
    }
    Ok((
        destination,
        created_version.filter(|_| created_parent.is_none()),
    ))
}

fn desktop_action(path: &Path, action: &str, params: &Value, settings: &Value) -> Outcome {
    let target = fs::metadata(path).map_err(|e| e.to_string())?;
    if !(target.is_dir() || target.is_file()) {
        return Err("Unsupported file type.".into());
    }
    let executable = target.is_file() && target.mode() & 0o111 != 0;
    let parent = if target.is_dir() {
        path
    } else {
        path.parent().ok_or("Missing parent directory.")?
    };
    let terminal = program(
        settings,
        "terminal",
        &[
            "xdg-terminal-exec",
            "foot",
            "kitty",
            "alacritty",
            "ghostty",
            "gnome-terminal",
            "konsole",
            "xterm",
        ],
    );
    let mime = if target.is_dir() {
        "inode/directory".to_owned()
    } else {
        let info = gio::File::for_path(path)
            .query_info(
                "standard::content-type",
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            )
            .map_err(|e| e.to_string())?;
        info.content_type()
            .map(|t| t.to_string())
            .unwrap_or_else(|| "application/octet-stream".into())
    };
    // Select a viewing application explicitly for runnable files and desktop entries.
    let view_type = if executable
        || [
            "application/x-desktop",
            "application/x-executable",
            "application/java-archive",
            "application/x-sharedlib",
        ]
        .iter()
        .any(|t| gio::content_type_is_a(&mime, t))
    {
        "text/plain"
    } else {
        &mime
    };
    match action {
        "applications" => {
            let apps: Vec<_> = gio::AppInfo::all_for_type(view_type)
                .into_iter()
                .filter_map(|app| {
                    let id = app.id()?;
                    Some(json!({"id":id.as_str(),"name":app.display_name().as_str()}))
                })
                .collect();
            let editable = target.is_file()
                && (gio::content_type_is_a(&mime, "text/plain")
                    || matches!(
                        mime.as_str(),
                        "application/json"
                            | "application/xml"
                            | "application/javascript"
                            | "application/x-desktop"
                    ));
            let clipboard = gio::glib::find_program_in_path("wl-copy").is_some()
                && std::env::var_os("WAYLAND_DISPLAY").is_some();
            Ok(json!({
                "applications":apps, "editable":editable,
                "terminalAvailable":terminal.is_some(),
                "runAvailable":executable && terminal.as_deref().is_some_and(can_run_in),
                "copyFileAvailable":clipboard,
            }))
        }
        "open" => {
            if target.is_dir()
                && !settings["file_manager"]
                    .as_str()
                    .unwrap_or_default()
                    .is_empty()
            {
                let manager = program(settings, "file_manager", &[])
                    .ok_or("Configured file manager unavailable.")?;
                spawn(Command::new(manager).arg(path))?;
            } else {
                launch_default(view_type, path)?;
            }
            Ok(json!({"dispatched":true}))
        }
        "openWith" => {
            let id = params["appId"].as_str().ok_or("Select an application.")?;
            let app = gio::AppInfo::all_for_type(view_type)
                .into_iter()
                .find(|a| a.id().as_deref() == Some(id))
                .ok_or("Application unavailable for this file type.")?;
            app.launch(&[gio::File::for_path(path)], gio::AppLaunchContext::NONE)
                .map_err(|e| e.to_string())?;
            let mut result = json!({"dispatched":true});
            if params["setDefault"] == true && view_type != mime {
                result["warning"] =
                    "Viewing application opened. Default unchanged for executable content.".into();
            } else if params["setDefault"] == true
                && let Err(error) = app.set_as_default_for_type(&mime)
            {
                result["warning"] = error.to_string().into();
            }
            Ok(result)
        }
        "editor" if target.is_file() => {
            let mut result = json!({"dispatched":true});
            let line = params["line"].as_u64().filter(|n| *n > 0);
            let argv =
                if let Some(configured) = settings["editor"].as_str().filter(|s| !s.is_empty()) {
                    Some(vec![std::ffi::OsString::from(configured)])
                } else if let Some(value) = ["VISUAL", "EDITOR"]
                    .iter()
                    .find_map(|v| std::env::var_os(v).filter(|s| !s.is_empty()))
                {
                    Some(gio::glib::shell_parse_argv(value).map_err(|e| e.to_string())?)
                } else {
                    None
                };
            if let Some(argv) = argv {
                let (program, args) = argv.split_first().ok_or("Editor command is empty.")?;
                let editor = gio::glib::find_program_in_path(program)
                    .ok_or("Configured editor unavailable.")?;
                let name = editor.file_name().and_then(OsStr::to_str).unwrap_or("");
                if matches!(name, "nvim" | "vim" | "vi" | "nano") {
                    let term = terminal
                        .as_deref()
                        .ok_or("No terminal available for this editor.")?;
                    let mut cmd = terminal_command(term, parent, true)?;
                    cmd.arg(editor).args(args);
                    if let Some(line) = line {
                        cmd.arg(format!("+{line}"));
                    }
                    spawn(cmd.arg("--").arg(path))?;
                } else {
                    let mut cmd = Command::new(&editor);
                    cmd.args(args);
                    if matches!(name, "code" | "codium" | "code-oss")
                        && let Some(line) = line
                    {
                        let mut location = path.as_os_str().to_owned();
                        location.push(format!(":{line}"));
                        cmd.arg("--goto").arg(location);
                    } else {
                        cmd.arg(path);
                        if line.is_some() {
                            result["warning"] =
                                "Opened in your editor without line positioning.".into();
                        }
                    }
                    spawn(&mut cmd)?;
                }
            } else {
                launch_default("text/plain", path)?;
                if line.is_some() {
                    result["warning"] = "Opened in your editor without line positioning.".into();
                }
            }
            Ok(result)
        }
        "terminal" | "run" => {
            let term = terminal.ok_or("Terminal unavailable.")?;
            let run = action == "run";
            if run {
                confirm(params)?;
                if !executable {
                    return Err("This file is not executable.".into());
                }
            }
            let mut cmd = terminal_command(&term, parent, run)?;
            if run {
                cmd.arg(path);
            }
            spawn(&mut cmd)?;
            Ok(json!({}))
        }
        "reveal" => {
            if settings["file_manager"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
            {
                let manager = program(settings, "file_manager", &[])
                    .ok_or("Configured file manager unavailable.")?;
                spawn(Command::new(manager).arg(path.parent().unwrap_or(path)))?;
            } else if !show_item(path) {
                launch_default("inode/directory", path.parent().unwrap_or(path))?;
            }
            Ok(json!({}))
        }
        _ => Err("Unsupported file action.".into()),
    }
}

fn launch_default(mime: &str, path: &Path) -> Result<(), String> {
    gio::AppInfo::default_for_type(mime, false)
        .ok_or("No associated application. Choose Open with.")?
        .launch(&[gio::File::for_path(path)], gio::AppLaunchContext::NONE)
        .map_err(|e| e.to_string())
}

fn program(settings: &Value, key: &str, candidates: &[&str]) -> Option<PathBuf> {
    if let Some(name) = settings[key].as_str().filter(|s| !s.is_empty()) {
        if name.contains('/') && !Path::new(name).is_absolute() {
            return None;
        }
        return gio::glib::find_program_in_path(name);
    }
    candidates.iter().find_map(gio::glib::find_program_in_path)
}

fn can_run_in(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some(
            "xdg-terminal-exec"
                | "foot"
                | "kitty"
                | "alacritty"
                | "ghostty"
                | "gnome-terminal"
                | "konsole"
                | "xterm"
                | "wezterm"
        )
    )
}

fn terminal_command(terminal: &Path, cwd: &Path, run: bool) -> Result<Command, String> {
    let mut cmd = Command::new(terminal);
    cmd.current_dir(cwd);
    if run {
        match terminal.file_name().and_then(OsStr::to_str) {
            Some("alacritty" | "ghostty" | "konsole" | "xterm") => {
                cmd.arg("-e");
            }
            Some("xdg-terminal-exec" | "gnome-terminal") => {
                cmd.arg("--");
            }
            Some("wezterm") => {
                cmd.args(["start", "--"]);
            }
            Some("foot" | "kitty") => (),
            _ => return Err("Unknown terminal command adapter.".into()),
        }
    }
    Ok(cmd)
}

fn spawn(cmd: &mut Command) -> Result<(), String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| e.to_string())?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn show_item(path: &Path) -> bool {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return false;
    };
    let parameters = (
        vec![gio::File::for_path(path).uri().to_string()],
        String::new(),
    )
        .to_variant();
    bus.call_sync(
        Some("org.freedesktop.FileManager1"),
        "/org/freedesktop/FileManager1",
        "org.freedesktop.FileManager1",
        "ShowItems",
        Some(&parameters),
        None,
        gio::DBusCallFlags::NONE,
        1200,
        gio::Cancellable::NONE,
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(action: &str, path: &Path, name: &str) -> Value {
        json!({"action":action,"version":version(&fs::symlink_metadata(path).unwrap()),"name":name})
    }
    #[test]
    fn rename_and_undo_refuse_overwrite_and_changed_items() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        fs::write(&a, "one").unwrap();
        fs::write(&b, "two").unwrap();
        let mut actions = Actions::default();
        let rename = request("rename", &a, "b");
        assert!(actions.perform(&a, &rename, |_| true).is_err());
        fs::remove_file(&b).unwrap();
        let offer = actions.perform(&a, &rename, |_| true).unwrap();
        let undo = json!({"action":"undo","undoId":offer["undoId"]});
        fs::write(&a, "other").unwrap();
        assert!(actions.perform(&a, &undo, |_| true).is_err());
        fs::remove_file(&a).unwrap();
        actions.perform(&a, &undo, |_| true).unwrap();
        assert_eq!(fs::read(&a).unwrap(), b"one");
        let stale = request("rename", &a, "new");
        fs::write(&a, "changed").unwrap();
        assert!(actions.perform(&a, &stale, |_| true).is_err());
    }
    #[test]
    fn exclusive_nested_create_and_policy() {
        let dir = tempfile::tempdir().unwrap();
        let parent = dir.path();
        let mut actions = Actions::default();
        for name in ["../escape", "/abs", "a//b", "./file"] {
            assert!(
                actions
                    .perform(parent, &request("createFile", parent, name), |_| true)
                    .is_err()
            );
        }
        let create = request("createFile", parent, "new/sub/file");
        assert!(
            actions
                .perform(parent, &create, |p| !p.ends_with("file"))
                .is_err()
        );
        let result = actions.perform(parent, &create, |_| true).unwrap();
        assert!(result["undoId"].is_null());
        assert!(parent.join("new/sub/file").is_file());
        assert!(
            actions
                .perform(
                    parent,
                    &request("createFile", parent, "new/sub/file"),
                    |_| true
                )
                .is_err()
        );
        let new_file = parent.join("new-file");
        let offer = actions
            .perform(parent, &request("createFile", parent, "new-file"), |_| true)
            .unwrap();
        fs::write(&new_file, "mine").unwrap();
        assert!(
            actions
                .perform(
                    parent,
                    &json!({"action":"undo","undoId":offer["undoId"]}),
                    |_| true
                )
                .is_err()
        );
        assert_eq!(fs::read(new_file).unwrap(), b"mine");
    }
}
