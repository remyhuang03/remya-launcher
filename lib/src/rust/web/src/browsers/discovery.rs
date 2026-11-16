use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::roots::{BROWSER_ROOTS, RootBase};
use super::{
    Family, MAX_BOOKMARKS_BYTES, MAX_DIR_SCAN_ENTRIES, MAX_INI_BYTES, MAX_LOCAL_STATE_BYTES,
    MAX_PROFILES, Profile, check_owned_dir_rel, check_owned_dir_with_meta,
    check_owned_file_with_meta, open_owned_readonly_file, read_bounded_file, sanitize_label,
};

pub(super) fn discover_with_env(
    home: Option<&Path>,
    xdg_config: Option<&Path>,
    chrome_config: Option<&Path>,
    chrome_user_data: Option<&Path>,
) -> Vec<Profile> {
    let fallback_xdg = home.map(|h| h.join(".config"));
    let effective_xdg = xdg_config.or(fallback_xdg.as_deref());
    let effective_chrome = chrome_config.or(effective_xdg);

    let mut profiles = Vec::new();
    let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();
    let mut seen_paths: HashSet<PathBuf> = HashSet::new();

    for spec in BROWSER_ROOTS {
        if profiles.len() >= MAX_PROFILES {
            break;
        }
        let Some(base_dir) = (match spec.base {
            RootBase::XdgConfig => effective_xdg,
            RootBase::ChromeConfigHome => effective_chrome,
            RootBase::ChromeUserData => chrome_user_data,
            RootBase::Home => home,
        }) else {
            continue;
        };

        let Ok((canon_root, _)) = check_owned_dir_rel(base_dir, Path::new(spec.rel_path)) else {
            continue;
        };

        let candidates = match spec.family {
            Family::Gecko => discover_gecko_candidates(&canon_root),
            Family::Chromium => discover_chromium_candidates(&canon_root, spec.flat_root),
        };

        for (candidate_dir, raw_label) in candidates {
            if profiles.len() >= MAX_PROFILES {
                break;
            }
            let Ok((canon_profile, meta)) = check_owned_dir_with_meta(&candidate_dir) else {
                continue;
            };
            let dev_ino = (meta.dev(), meta.ino());
            if !seen_inodes.insert(dev_ino) || !seen_paths.insert(canon_profile.clone()) {
                continue;
            }
            let has_data = match spec.family {
                Family::Gecko => is_gecko_profile_dir(&canon_profile),
                Family::Chromium => is_chromium_profile_dir(&canon_profile),
            };
            if !has_data {
                continue;
            }
            let label = sanitize_label(&raw_label, "Default");
            profiles.push(Profile {
                id: profile_id(spec.browser_id, &canon_profile),
                browser_id: spec.browser_id.to_string(),
                browser_label: spec.browser_label.to_string(),
                label,
                path: canon_profile,
                family: spec.family,
            });
        }
    }

    profiles
}

struct GeckoIniProfile {
    name: Option<String>,
    path: String,
    is_relative: bool,
    is_default: bool,
}

fn discover_gecko_candidates(root: &Path) -> Vec<(PathBuf, String)> {
    let mut entries: Vec<GeckoIniProfile> = Vec::new();
    let mut seen_raw_paths: HashSet<String> = HashSet::new();

    if let Ok(bytes) = read_bounded_file(&root.join("profiles.ini"), MAX_INI_BYTES) {
        let text = String::from_utf8_lossy(&bytes);
        parse_gecko_ini(&text, true, &mut entries, &mut seen_raw_paths);
    }

    if let Ok(bytes) = read_bounded_file(&root.join("installs.ini"), MAX_INI_BYTES) {
        let text = String::from_utf8_lossy(&bytes);
        parse_gecko_ini(&text, false, &mut entries, &mut seen_raw_paths);
    }

    // Prefer default-marked profiles first while keeping stable order within ties.
    entries.sort_by_key(|item| !item.is_default);

    let mut out = Vec::new();
    for entry in entries.into_iter().take(MAX_PROFILES) {
        let Some(resolved) = resolve_gecko_profile_path(root, &entry.path, entry.is_relative)
        else {
            continue;
        };
        if entry.is_relative
            && let Ok(rel) = resolved.strip_prefix(root)
            && check_owned_dir_rel(root, rel).is_err()
        {
            continue;
        }
        let fallback_name = resolved
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Default");
        let label = entry
            .name
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(fallback_name)
            .to_string();
        if is_excluded_profile_name(&label) || is_excluded_profile_name(fallback_name) {
            continue;
        }
        out.push((resolved, label));
    }
    out
}

fn parse_gecko_ini(
    content: &str,
    is_profiles_ini: bool,
    out: &mut Vec<GeckoIniProfile>,
    seen_paths: &mut HashSet<String>,
) {
    #[derive(Default)]
    struct SectionState {
        is_profile: bool,
        is_install: bool,
        name: Option<String>,
        path: Option<String>,
        is_relative: Option<bool>,
        is_default: bool,
        install_default: Option<String>,
    }

    let flush =
        |sec: SectionState, out: &mut Vec<GeckoIniProfile>, seen_paths: &mut HashSet<String>| {
            if out.len() >= MAX_PROFILES {
                return;
            }
            if sec.is_profile {
                if let Some(path) = sec.path.filter(|p| !p.is_empty())
                    && seen_paths.insert(path.clone())
                {
                    out.push(GeckoIniProfile {
                        name: sec.name,
                        path,
                        is_relative: sec.is_relative.unwrap_or(true),
                        is_default: sec.is_default,
                    });
                }
            } else if sec.is_install
                && let Some(path) = sec.install_default.filter(|p| !p.is_empty())
                && seen_paths.insert(path.clone())
            {
                out.push(GeckoIniProfile {
                    name: None,
                    path,
                    is_relative: true,
                    is_default: true,
                });
            }
        };

    let mut current = SectionState::default();
    let mut section_count = 0usize;

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            flush(current, out, seen_paths);
            section_count += 1;
            if section_count > MAX_PROFILES * 2 {
                return;
            }
            let header = &line[1..line.len() - 1];
            let is_profile = is_profiles_ini && is_gecko_profile_section(header);
            let is_install = if is_profiles_ini {
                header.starts_with("Install") && header.len() > "Install".len()
            } else {
                !header.eq_ignore_ascii_case("General") && !header.is_empty()
            };
            current = SectionState {
                is_profile,
                is_install,
                ..SectionState::default()
            };
            continue;
        }
        let Some((key, val)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let val = val.trim();
        if current.is_profile {
            match key {
                "Name" => current.name = Some(val.to_string()),
                "Path" => current.path = Some(val.to_string()),
                "IsRelative" => {
                    current.is_relative = Some(val == "1" || val.eq_ignore_ascii_case("true"))
                }
                "Default" => current.is_default = val == "1" || val.eq_ignore_ascii_case("true"),
                _ => {}
            }
        } else if current.is_install && key == "Default" {
            current.install_default = Some(val.to_string());
        }
    }
    flush(current, out, seen_paths);
}

fn is_gecko_profile_section(header: &str) -> bool {
    let Some(suffix) = header.strip_prefix("Profile") else {
        return false;
    };
    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
}

fn resolve_gecko_profile_path(root: &Path, raw: &str, is_relative: bool) -> Option<PathBuf> {
    if raw.is_empty() || raw.contains('\0') {
        return None;
    }
    let rel_or_abs = Path::new(raw);
    if is_relative {
        if rel_or_abs.is_absolute() {
            return None;
        }
        let mut clean = PathBuf::new();
        for comp in rel_or_abs.components() {
            match comp {
                Component::Normal(seg) => clean.push(seg),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
            }
        }
        if clean.as_os_str().is_empty() {
            return None;
        }
        Some(root.join(clean))
    } else if rel_or_abs.is_absolute() {
        Some(rel_or_abs.to_path_buf())
    } else {
        None
    }
}

fn discover_chromium_candidates(root: &Path, flat_root: bool) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    if let Ok(bytes) = read_bounded_file(&root.join("Local State"), MAX_LOCAL_STATE_BYTES)
        && let Ok(Value::Object(doc)) = serde_json::from_slice::<Value>(&bytes)
        && let Some(info_cache) = doc
            .get("profile")
            .and_then(|p| p.get("info_cache"))
            .and_then(Value::as_object)
    {
        let mut keys: Vec<&String> = info_cache.keys().collect();
        keys.sort();
        for dir_name in keys.into_iter().take(MAX_PROFILES) {
            if !is_safe_single_component(dir_name) {
                continue;
            }
            // Mark seen before checking exclusions so fallback scanning never resurrects excluded entries.
            if !seen_names.insert(dir_name.clone()) || is_excluded_profile_name(dir_name) {
                continue;
            }
            let Some(info) = info_cache.get(dir_name).and_then(Value::as_object) else {
                continue;
            };
            if info.get("is_ephemeral").and_then(Value::as_bool) == Some(true)
                || info.get("is_guest").and_then(Value::as_bool) == Some(true)
            {
                continue;
            }
            let label = info
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(dir_name);
            if is_excluded_profile_name(label) {
                continue;
            }
            out.push((root.join(dir_name), label.to_string()));
        }
    }

    // Direct non-recursive scan for Default and numbered "Profile <N>" fallback directories.
    if let Ok(read_dir) = fs::read_dir(root) {
        let mut fallback_dirs = Vec::new();
        for entry in read_dir.take(MAX_DIR_SCAN_ENTRIES).flatten() {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if !is_safe_single_component(&name) || seen_names.contains(&name) {
                continue;
            }
            if is_chromium_fallback_profile_dir(&name) && !is_excluded_profile_name(&name) {
                fallback_dirs.push(name);
            }
        }
        fallback_dirs.sort();
        for name in fallback_dirs {
            if out.len() >= MAX_PROFILES {
                break;
            }
            if seen_names.insert(name.clone()) {
                out.push((root.join(&name), name));
            }
        }
    }

    if flat_root && out.len() < MAX_PROFILES {
        out.push((root.to_path_buf(), "Default".to_string()));
    }

    out
}

fn is_chromium_fallback_profile_dir(name: &str) -> bool {
    if name == "Default" {
        return true;
    }
    let Some(suffix) = name.strip_prefix("Profile ") else {
        return false;
    };
    !suffix.is_empty() && suffix.len() <= 6 && suffix.bytes().all(|b| b.is_ascii_digit())
}

fn is_safe_single_component(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && name != "."
        && name != ".."
}

fn is_excluded_profile_name(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "guest"
            | "guest profile"
            | "system"
            | "system profile"
            | "off-the-record"
            | "off_the_record"
            | "incognito"
            | "otr"
            | "private"
    )
}

pub(super) fn is_gecko_profile_dir(dir: &Path) -> bool {
    has_sqlite_or_wal(&dir.join("places.sqlite"), &dir.join("places.sqlite-wal"))
}

fn is_chromium_profile_dir(dir: &Path) -> bool {
    has_sqlite_or_wal(&dir.join("History"), &dir.join("History-wal"))
        || check_owned_file_with_meta(&dir.join("Bookmarks"))
            .is_ok_and(|(_, m)| m.len() > 0 && m.len() <= MAX_BOOKMARKS_BYTES as u64)
}

fn has_sqlite_or_wal(db_path: &Path, wal_path: &Path) -> bool {
    let Ok((mut file, meta)) = open_owned_readonly_file(db_path) else {
        return false;
    };
    let mut buf = [0u8; 16];
    let mut read_len = 0usize;
    while read_len < buf.len() {
        match file.read(&mut buf[read_len..]) {
            Ok(0) => break,
            Ok(n) => read_len += n,
            Err(_) => return false,
        }
    }
    if read_len == 16 && buf == *b"SQLite format 3\0" {
        return true;
    }
    read_len == 0
        && meta.len() == 0
        && check_owned_file_with_meta(wal_path).is_ok_and(|(_, wal_meta)| wal_meta.len() > 0)
}

pub(super) fn profile_id(browser_id: &str, canonical_path: &Path) -> String {
    let mut digest = Sha256::new();
    digest.update(b"remya-web-profile-v1\0");
    digest.update(browser_id.as_bytes());
    digest.update(b"\0");
    digest.update(canonical_path.as_os_str().as_encoded_bytes());
    let hash = digest.finalize();
    let mut hex = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}
