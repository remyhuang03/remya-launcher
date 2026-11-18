use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

pub fn path_id(path: &Path) -> String {
    URL_SAFE_NO_PAD.encode(
        path.components()
            .collect::<PathBuf>()
            .as_os_str()
            .as_bytes(),
    )
}

pub fn decode_id(id: &str) -> Result<PathBuf, String> {
    let bytes = URL_SAFE_NO_PAD.decode(id).map_err(|_| "Invalid file ID")?;
    if bytes.contains(&0) || bytes.len() > 4096 {
        return Err("Invalid file ID".into());
    }
    let path = PathBuf::from(OsString::from_vec(bytes));
    path.is_absolute()
        .then_some(path)
        .ok_or("Invalid file ID".into())
}

const PRIVATE_NAMES: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".password-store",
    ".aws",
    ".kube",
    ".git",
    ".pki",
];

pub struct Policy {
    pub home: PathBuf,
    pub roots: Vec<PathBuf>,
    pub exclusions: Vec<PathBuf>,
    mounts: Vec<PathBuf>,
}

fn xdg(variable: &str, fallback: &Path) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| fallback.to_owned())
}

pub fn expand_path(input: &str, anchor: &Path, home: &Path) -> Result<PathBuf, String> {
    let (head, tail) = input.split_once('/').unwrap_or((input, ""));
    let base = if head == "~" || head == "$HOME" {
        home.to_owned()
    } else if let Some(user) = head.strip_prefix('~') {
        let passwd = fs::read("/etc/passwd").map_err(|_| "Local user directory unavailable.")?;
        passwd
            .split(|&b| b == b'\n')
            .find_map(|line| {
                let mut fields = line.split(|&b| b == b':');
                if fields.next()? != user.as_bytes() {
                    return None;
                }
                let path = PathBuf::from(OsString::from_vec(fields.nth(4)?.into()));
                path.is_absolute().then_some(path)
            })
            .ok_or("Unknown local user.")?
    } else if let Some(variable) = head.strip_prefix('$') {
        if !matches!(
            variable,
            "XDG_CONFIG_HOME"
                | "XDG_DATA_HOME"
                | "XDG_STATE_HOME"
                | "XDG_CACHE_HOME"
                | "XDG_RUNTIME_DIR"
        ) {
            return Err("Unsupported path variable.".into());
        }
        std::env::var_os(variable)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or("Path variable is unset.")?
    } else {
        return Ok(anchor.join(input));
    };
    Ok(base.join(tail.trim_start_matches('/')))
}

// mountinfo represents arbitrary path bytes with octal escapes.
fn unescape_mount(raw: &[u8]) -> OsString {
    let mut result = Vec::new();
    let mut bytes = raw;
    while !bytes.is_empty() {
        if bytes[0] == b'\\'
            && bytes.len() >= 4
            && bytes[1..4].iter().all(|b| (b'0'..=b'7').contains(b))
        {
            result.push((bytes[1] - b'0') * 64 + (bytes[2] - b'0') * 8 + bytes[3] - b'0');
            bytes = &bytes[4..];
        } else {
            result.push(bytes[0]);
            bytes = &bytes[1..];
        }
    }
    OsString::from_vec(result)
}

impl Policy {
    pub fn new(preferences: &Value) -> Self {
        let home = xdg("HOME", Path::new("/nonexistent"));
        let search = &preferences["files-search"];
        let paths = |key: &str| -> Vec<PathBuf> {
            search[key]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|s| !s.is_empty())
                .filter_map(|s| expand_path(s, &home, &home).ok())
                .collect()
        };
        let mut roots = paths("roots");
        if search["roots"].is_null() {
            roots.push(home.clone());
        }
        let mut exclusions: Vec<PathBuf> = ["/proc", "/dev", "/sys"]
            .into_iter()
            .map(PathBuf::from)
            .collect();
        exclusions.extend(paths("exclusions"));
        exclusions.extend([
            xdg("XDG_STATE_HOME", &home.join(".local/state")).join("remya"),
            xdg("XDG_CACHE_HOME", &home.join(".cache")).join("remya"),
            xdg("XDG_DATA_HOME", &home.join(".local/share")).join("remya/launcher"),
            xdg("XDG_CONFIG_HOME", &home.join(".config")).join("remya/launcher"),
        ]);
        let resolved: Vec<_> = exclusions
            .iter()
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        exclusions.extend(resolved);
        let mut local_mounts = Vec::new();
        if let Ok(mounts) = fs::read("/proc/self/mountinfo") {
            for line in mounts.split(|&b| b == b'\n') {
                let fields: Vec<_> = line.split(|&b| b == b' ').collect();
                let Some(i) = fields.iter().position(|&f| f == b"-") else {
                    continue;
                };
                let (Some(&kind), Some(&raw)) = (fields.get(i + 1), fields.get(4)) else {
                    continue;
                };
                let mount = PathBuf::from(unescape_mount(raw));
                if matches!(
                    kind,
                    b"nfs"
                        | b"nfs4"
                        | b"cifs"
                        | b"smb3"
                        | b"9p"
                        | b"ceph"
                        | b"afs"
                        | b"autofs"
                        | b"davfs"
                ) || kind.starts_with(b"fuse.")
                {
                    exclusions.push(mount);
                } else if mount.starts_with("/media") || mount.starts_with("/run/media") {
                    local_mounts.push(mount);
                }
            }
        }
        exclusions.sort();
        exclusions.dedup();
        Self {
            home,
            roots,
            exclusions,
            mounts: local_mounts,
        }
    }

    pub fn visible_path(&self, path: &Path) -> bool {
        path.is_absolute()
            && !path.components().any(|c| matches!(c, Component::Normal(n) if PRIVATE_NAMES.contains(&n.to_str().unwrap_or(""))))
            && !self.exclusions.iter().any(|p| path.starts_with(p))
    }

    pub fn allowed(&self, path: &Path) -> bool {
        if !self.visible_path(path) {
            return false;
        }
        let mut target = path;
        loop {
            match target.canonicalize() {
                Ok(resolved) => return self.visible_path(&resolved),
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound
                        && fs::symlink_metadata(target).is_err()
                        && !path.components().any(|c| c == Component::ParentDir) =>
                {
                    if let Some(parent) = target.parent() {
                        target = parent;
                    } else {
                        return false;
                    }
                }
                _ => return false,
            }
        }
    }

    pub fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.home)
            .map(|p| Path::new("~").join(p))
            .unwrap_or_else(|_| path.to_owned())
            .to_string_lossy()
            .into_owned()
    }

    pub fn private_names(&self) -> &'static [&'static str] {
        PRIVATE_NAMES
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub display_path: String,
    pub parent: String,
    pub kind: &'static str,
    pub group: &'static str,
    pub version: String,
    pub executable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    pub modified: i64,
    pub size: Option<u64>,
    pub completion: Option<String>,
    pub line: Option<u64>,
    pub snippet: Option<String>,
    pub hit_count: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hits: Vec<Hit>,
}

#[derive(Clone, Serialize)]
pub struct Hit {
    pub line: u64,
    pub snippet: String,
}

impl Entry {
    pub fn new(path: &Path, policy: &Policy, group: &'static str) -> Option<Self> {
        if !policy.allowed(path) {
            return None;
        }
        let metadata = fs::symlink_metadata(path).ok()?;
        let kind = if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else if metadata.is_symlink() {
            "symlink"
        } else {
            return None;
        };
        let completion = path.to_str().map(|p| {
            if metadata.is_dir() && !p.ends_with('/') {
                format!("{p}/")
            } else {
                p.into()
            }
        });
        Some(Self {
            id: path_id(path),
            name: path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned(),
            path: path.to_string_lossy().into_owned(),
            display_path: policy.display(path),
            parent: path.parent().map(|p| policy.display(p)).unwrap_or_default(),
            kind,
            group,
            version: crate::actions::version(&metadata),
            executable: metadata.is_file() && metadata.mode() & 0o111 != 0,
            pinned: None,
            size: metadata.is_file().then_some(metadata.len()),
            modified: metadata.mtime(),
            completion,
            line: None,
            snippet: None,
            hit_count: None,
            hits: vec![],
        })
    }
}

pub fn places(policy: &Policy) -> Vec<Entry> {
    use gio::glib::{UserDirectory, user_special_dir};
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut add = |path: PathBuf, label: Option<&str>| {
        if !seen.insert(path.clone()) || !policy.allowed(&path) || !path.is_dir() {
            return;
        }
        if let Some(mut entry) = Entry::new(&path, policy, "places") {
            if let Some(label) = label {
                entry.name = label.into();
            }
            entries.push(entry);
        }
    };
    add(policy.home.clone(), Some("Home"));
    for directory in [
        UserDirectory::Desktop,
        UserDirectory::Documents,
        UserDirectory::Downloads,
        UserDirectory::Pictures,
        UserDirectory::Music,
        UserDirectory::Videos,
        UserDirectory::PublicShare,
        UserDirectory::Templates,
    ] {
        if let Some(path) = user_special_dir(directory) {
            add(path, None);
        }
    }
    let bookmarks = xdg("XDG_CONFIG_HOME", &policy.home.join(".config")).join("gtk-3.0/bookmarks");
    if let Ok(bytes) = fs::File::open(bookmarks) {
        use std::io::Read;
        let mut s = String::new();
        if bytes.take(65536).read_to_string(&mut s).is_ok() {
            for line in s.lines().take(128) {
                let (uri, label) = line
                    .split_once(' ')
                    .map_or((line, None), |(u, l)| (u, Some(l)));
                if let Ok(url) = url::Url::parse(uri)
                    && let Ok(path) = url.to_file_path()
                {
                    add(path, label);
                }
            }
        }
    }
    for mount in &policy.mounts {
        add(mount.clone(), None);
    }
    entries
}

pub fn hidden_name(name: &OsStr) -> bool {
    name.as_bytes().starts_with(b".")
}
