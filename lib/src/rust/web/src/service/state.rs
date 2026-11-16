use std::collections::HashMap;
use std::env;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustix::fs::OFlags;
use rustix::process::geteuid;
use serde::{Deserialize, Serialize};

use super::profiles::ProfileRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PersistedProfileMeta {
    pub(super) profile_id: String,
    pub(super) browser_id: String,
    pub(super) browser_label: String,
    pub(super) label: String,
}

pub(super) fn ensure_private_web_state_dir() -> io::Result<PathBuf> {
    let state_home = match env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        Some(raw) => {
            let p = PathBuf::from(raw);
            if !p.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "XDG_STATE_HOME must be absolute",
                ));
            }
            p
        }
        None => {
            let home = env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, "Absolute HOME is required")
                })?;
            home.join(".local/state")
        }
    };

    if fs::symlink_metadata(&state_home).is_err() {
        fs::create_dir_all(&state_home)?;
    }
    let state_meta = fs::metadata(&state_home)?;
    if !state_meta.is_dir() || state_meta.uid() != geteuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "state home is not owned by current user",
        ));
    }

    let app_dir = state_home.join("remya-launcher");
    ensure_owned_no_symlink_dir(&app_dir)?;
    let web_dir = app_dir.join("web");
    ensure_owned_no_symlink_dir(&web_dir)?;
    Ok(web_dir)
}

fn ensure_owned_no_symlink_dir(path: &Path) -> io::Result<()> {
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err),
    }
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || !meta.is_dir() || meta.uid() != geteuid().as_raw() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "directory is symlink or not privately owned",
        ));
    }
    if meta.mode() & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(super) fn validate_existing_private_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() || !meta.is_file() || meta.uid() != geteuid().as_raw()
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "file is symlink or not privately owned",
                ));
            }
            if meta.mode() & 0o077 != 0 {
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            }
            Ok(())
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub(super) fn tighten_db_permissions(state_dir: &Path) {
    for name in ["index.sqlite", "index.sqlite-wal", "index.sqlite-shm"] {
        let p = state_dir.join(name);
        if let Ok(meta) = fs::symlink_metadata(&p)
            && !meta.file_type().is_symlink()
            && meta.is_file()
            && meta.uid() == geteuid().as_raw()
            && (meta.mode() & 0o077 != 0)
        {
            let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o600));
        }
    }
}

fn profiles_meta_path(state_dir: &Path) -> PathBuf {
    state_dir.join("profiles_meta.json")
}

pub(super) fn load_profiles_meta(state_dir: &Path) -> HashMap<String, PersistedProfileMeta> {
    let path = profiles_meta_path(state_dir);
    let Ok(meta) = fs::symlink_metadata(&path) else {
        return HashMap::new();
    };
    if meta.file_type().is_symlink() || !meta.is_file() || meta.uid() != geteuid().as_raw() {
        return HashMap::new();
    }
    let Ok(bytes) = fs::read(&path) else {
        return HashMap::new();
    };
    let Ok(items) = serde_json::from_slice::<Vec<PersistedProfileMeta>>(&bytes) else {
        return HashMap::new();
    };
    items
        .into_iter()
        .map(|item| (item.profile_id.clone(), item))
        .collect()
}

pub(super) fn save_profiles_meta(
    state_dir: &Path,
    known: &HashMap<String, ProfileRecord>,
) -> io::Result<()> {
    let items: Vec<PersistedProfileMeta> = known
        .values()
        .filter(|r| r.enabled)
        .map(|r| PersistedProfileMeta {
            profile_id: r.profile.id.clone(),
            browser_id: r.profile.browser_id.clone(),
            browser_label: r.profile.browser_label.clone(),
            label: r.profile.label.clone(),
        })
        .collect();
    if items.is_empty() {
        remove_profiles_meta(state_dir);
        return Ok(());
    }
    let path = profiles_meta_path(state_dir);
    let tmp = state_dir.join(format!(".profiles_meta.tmp.{}", std::process::id()));
    let bytes = serde_json::to_vec(&items).map_err(io::Error::other)?;
    let _ = fs::remove_file(&tmp);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32)
        .open(&tmp)?;
    if let Err(err) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    drop(file);
    if let Err(err) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

pub(super) fn remove_profiles_meta(state_dir: &Path) {
    let _ = fs::remove_file(profiles_meta_path(state_dir));
}

pub(super) fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
