use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustix::fs::OFlags;
use rustix::process::geteuid;

use chromium::read_chromium_profile;
use discovery::discover_with_env;
use favicon::{build_favicon_url_candidates, read_chromium_favicon, read_gecko_favicon};
use gecko::read_gecko_profile;

mod chromium;
mod discovery;
mod favicon;
mod gecko;
mod roots;
mod sqlite;

#[cfg(test)]
mod tests_discovery;
#[cfg(test)]
mod tests_favicon;
#[cfg(test)]
mod tests_source;

const NINETY_DAYS_MS: i64 = 90 * 24 * 60 * 60 * 1000;
const WEBKIT_EPOCH_OFFSET_US: i128 = 11_644_473_600_000_000;

const SQLITE_BUSY_TIMEOUT_MS: u64 = 250;
const HISTORY_BATCH_SIZE: i64 = 500;
const MAX_HISTORY_ENTRIES: usize = 10_000;
const MAX_HISTORY_SCAN_ROWS: usize = 100_000;
const MAX_BOOKMARKS: usize = 10_000;
const MAX_BOOKMARK_SCAN_ROWS: usize = 100_000;
const MAX_BOOKMARK_SCAN_NODES: usize = 100_000;
const MAX_BOOKMARK_FOLDERS: usize = 10_000;
const MAX_FOLDER_DEPTH: usize = 64;

const MAX_PROFILES: usize = 64;
const MAX_DIR_SCAN_ENTRIES: usize = 256;
const MAX_INI_BYTES: usize = 256 * 1024;
const MAX_LOCAL_STATE_BYTES: usize = 4 * 1024 * 1024;
const MAX_BOOKMARKS_BYTES: usize = 16 * 1024 * 1024;

const MAX_TITLE_CHARS: usize = 512;
const MAX_LABEL_CHARS: usize = 128;
const MAX_FOLDER_CHARS: usize = 128;
const MAX_VISIT_COUNT: i64 = 1_000_000;

const MAX_FAVICON_BYTES: usize = 512 * 1024;
const MAX_FAVICON_SOURCE_DIM: u32 = 2048;
const MAX_FAVICON_PIXELS: u64 = 2048 * 2048;
const MAX_FAVICON_DECODE_ALLOC: usize = 32 * 1024 * 1024;
const TARGET_FAVICON_DIM: u32 = 64;

/// Browser engine family determining on-disk storage layout and SQLite schemas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Gecko,
    Chromium,
}

/// Discovered local browser profile metadata.
///
/// The filesystem `path` is internal to the backend and must never be exposed
/// over UI-facing IPC responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub id: String,
    pub browser_id: String,
    pub browser_label: String,
    pub label: String,
    pub path: PathBuf,
    pub family: Family,
}

/// Normalized history or bookmark entry extracted from a browser profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub source_id: String,
    pub bookmark: bool,
    pub url: String,
    pub title: String,
    pub visited_count: i64,
    pub last_visited_ms: Option<i64>,
    pub folder_path: Vec<String>,
}

/// Discover local Gecko and Chromium browser profiles from bounded, curated root paths.
pub fn discover() -> Vec<Profile> {
    let home = absolute_env_path("HOME");
    let xdg_config = absolute_env_path("XDG_CONFIG_HOME");
    let chrome_config = absolute_env_path("CHROME_CONFIG_HOME");
    let chrome_user_data = absolute_env_path("CHROME_USER_DATA_DIR");
    discover_with_env(
        home.as_deref(),
        xdg_config.as_deref(),
        chrome_config.as_deref(),
        chrome_user_data.as_deref(),
    )
}

/// Read bounded 90-day recent history and bookmarks from a discovered browser profile.
///
/// Returns stable uppercase error codes (`SOURCE_LOCKED`, `UNSUPPORTED_SCHEMA`,
/// `PERMISSION_DENIED`, `SOURCE_UNAVAILABLE`) without mutating source files.
pub fn read(profile: &Profile) -> Result<Vec<Entry>, &'static str> {
    let canon_dir = check_owned_dir(&profile.path)?;
    let now_ms = current_unix_ms();
    match profile.family {
        Family::Gecko => read_gecko_profile(&canon_dir, now_ms),
        Family::Chromium => read_chromium_profile(&canon_dir, now_ms),
    }
}

/// Read and normalize a local browser-cached favicon into a bounded <=64px PNG.
///
/// Rejects invalid URLs, SVG payloads, oversized source BLOBs, and oversized images.
/// Never performs network requests.
pub fn favicon(profile: &Profile, url: &str) -> Option<Vec<u8>> {
    let validated = crate::index::validate_url(url)?;
    let canon_dir = check_owned_dir(&profile.path).ok()?;
    let candidate_urls = build_favicon_url_candidates(url, validated.as_str());

    match profile.family {
        Family::Gecko => read_gecko_favicon(&canon_dir, &candidate_urls),
        Family::Chromium => read_chromium_favicon(&canon_dir, &candidate_urls),
    }
}

fn absolute_env_path(key: &str) -> Option<PathBuf> {
    std::env::var_os(key)
        .filter(|val| !val.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn current_euid() -> u32 {
    geteuid().as_raw()
}

fn check_owned_dir(path: &Path) -> Result<PathBuf, &'static str> {
    check_owned_dir_with_meta(path).map(|(canon, _)| canon)
}

fn check_owned_dir_rel(base: &Path, rel: &Path) -> Result<(PathBuf, fs::Metadata), &'static str> {
    let euid = current_euid();
    let _ = check_owned_dir_with_meta(base)?;
    let mut cur = base.to_path_buf();
    for comp in rel.components() {
        match comp {
            Component::Normal(seg) => {
                cur.push(seg);
                let meta = fs::symlink_metadata(&cur).map_err(map_io_err)?;
                if meta.file_type().is_symlink() {
                    if meta.uid() != euid {
                        return Err("PERMISSION_DENIED");
                    }
                } else if !meta.is_dir() {
                    return Err("SOURCE_UNAVAILABLE");
                } else if meta.uid() != euid || (meta.mode() & 0o002) != 0 {
                    return Err("PERMISSION_DENIED");
                }
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("PERMISSION_DENIED");
            }
        }
    }
    check_owned_dir_with_meta(&cur)
}

fn check_owned_dir_with_meta(path: &Path) -> Result<(PathBuf, fs::Metadata), &'static str> {
    let euid = current_euid();
    let link_meta = fs::symlink_metadata(path).map_err(map_io_err)?;
    if link_meta.file_type().is_symlink() {
        if link_meta.uid() != euid {
            return Err("PERMISSION_DENIED");
        }
    } else if !link_meta.is_dir() {
        return Err("SOURCE_UNAVAILABLE");
    }
    let canon = fs::canonicalize(path).map_err(map_io_err)?;
    let meta = fs::symlink_metadata(&canon).map_err(map_io_err)?;
    if !meta.is_dir() {
        return Err("SOURCE_UNAVAILABLE");
    }
    if meta.uid() != euid || (meta.mode() & 0o002) != 0 {
        return Err("PERMISSION_DENIED");
    }
    Ok((canon, meta))
}

fn check_owned_file_with_meta(path: &Path) -> Result<(PathBuf, fs::Metadata), &'static str> {
    let euid = current_euid();
    let link_meta = fs::symlink_metadata(path).map_err(map_io_err)?;
    if link_meta.file_type().is_symlink() {
        return Err("PERMISSION_DENIED");
    }
    if !link_meta.is_file() {
        return Err("SOURCE_UNAVAILABLE");
    }
    if link_meta.uid() != euid || link_meta.nlink() != 1 || (link_meta.mode() & 0o002) != 0 {
        return Err("PERMISSION_DENIED");
    }
    let canon = fs::canonicalize(path).map_err(map_io_err)?;
    let meta = fs::symlink_metadata(&canon).map_err(map_io_err)?;
    if !meta.is_file() {
        return Err("SOURCE_UNAVAILABLE");
    }
    if (meta.dev(), meta.ino()) != (link_meta.dev(), link_meta.ino())
        || meta.uid() != euid
        || meta.nlink() != 1
        || (meta.mode() & 0o002) != 0
    {
        return Err("PERMISSION_DENIED");
    }
    Ok((canon, meta))
}

fn open_owned_readonly_file(path: &Path) -> Result<(File, fs::Metadata), &'static str> {
    let (canon, lmeta) = check_owned_file_with_meta(path)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32 | OFlags::NONBLOCK.bits() as i32)
        .open(&canon)
        .map_err(map_io_err)?;
    let fmeta = file.metadata().map_err(map_io_err)?;
    if !fmeta.is_file() {
        return Err("SOURCE_UNAVAILABLE");
    }
    if fmeta.uid() != current_euid()
        || fmeta.nlink() != 1
        || (fmeta.mode() & 0o002) != 0
        || (fmeta.dev(), fmeta.ino()) != (lmeta.dev(), lmeta.ino())
    {
        return Err("PERMISSION_DENIED");
    }
    Ok((file, fmeta))
}

fn read_bounded_file(path: &Path, max_bytes: usize) -> Result<Vec<u8>, &'static str> {
    let (file, meta) = open_owned_readonly_file(path)?;
    if meta.len() > max_bytes as u64 {
        return Err("SOURCE_TOO_LARGE");
    }
    let mut buf = Vec::with_capacity((meta.len() as usize).min(max_bytes));
    let mut reader = file.take((max_bytes as u64).saturating_add(1));
    reader.read_to_end(&mut buf).map_err(map_io_err)?;
    if buf.len() > max_bytes {
        return Err("SOURCE_TOO_LARGE");
    }
    Ok(buf)
}

fn read_bounded_prefix(path: &Path, max_bytes: usize) -> Result<Vec<u8>, &'static str> {
    let (file, _) = open_owned_readonly_file(path)?;
    let mut buf = Vec::with_capacity(max_bytes);
    file.take(max_bytes as u64)
        .read_to_end(&mut buf)
        .map_err(map_io_err)?;
    Ok(buf)
}

fn map_io_err(err: std::io::Error) -> &'static str {
    match err.kind() {
        std::io::ErrorKind::NotFound => "SOURCE_UNAVAILABLE",
        std::io::ErrorKind::PermissionDenied => "PERMISSION_DENIED",
        _ => "SOURCE_UNAVAILABLE",
    }
}

fn path_present(path: &Path) -> Result<bool, &'static str> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(map_io_err(err)),
    }
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(0)
}

fn sanitize_text(raw: &str, max_chars: usize) -> String {
    let mut cleaned = String::with_capacity(raw.len().min(max_chars));
    let mut char_count = 0usize;
    let mut prev_space = true;
    for ch in raw.chars() {
        if ch == '\0' {
            continue;
        }
        if ch.is_control() || ch.is_whitespace() {
            if !prev_space && char_count < max_chars {
                cleaned.push(' ');
                char_count += 1;
                prev_space = true;
            }
            continue;
        }
        if char_count >= max_chars {
            break;
        }
        cleaned.push(ch);
        char_count += 1;
        prev_space = false;
    }
    while cleaned.ends_with(' ') {
        cleaned.pop();
    }
    cleaned
}

fn sanitize_label(raw: &str, fallback: &str) -> String {
    let cleaned = sanitize_text(raw, MAX_LABEL_CHARS);
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned
    }
}
