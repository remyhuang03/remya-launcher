use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use flate2::read::GzDecoder;
use quick_xml::Reader;
use quick_xml::events::Event;
use ruzstd::decoding::StreamingDecoder;

use crate::desktop::IndexedApp;
use crate::protocol::{PackageEntry, Preferences};
use crate::store::absolute_env;

const MAX_PACKAGE_RESULTS: usize = 20;

#[derive(Debug, Clone)]
pub struct OfficialPkg {
    pub name: String,
    pub name_lower: String,
    pub version: String,
    pub description: String,
    pub desc_lower: String,
    pub repo: String,
}

#[derive(Debug, Clone)]
pub struct FlatpakPkg {
    pub id: String,
    pub id_lower: String,
    pub name: String,
    pub name_lower: String,
    pub description: String,
    pub desc_lower: String,
    pub version: String,
    pub remote: String,
}

#[derive(Debug, Clone)]
pub struct AurPkg {
    pub name: String,
    pub name_lower: String,
}

#[derive(Default)]
pub struct PackageCache {
    loaded: bool,
    fingerprint: Vec<(PathBuf, Option<SystemTime>)>,
    official: Vec<OfficialPkg>,
    official_repos: HashMap<String, String>,
    flatpak: Vec<FlatpakPkg>,
    aur: Vec<AurPkg>,
    aur_helper: Option<String>,
}

impl PackageCache {
    pub fn ensure_loaded(&mut self) {
        let fp = source_fingerprint();
        if self.loaded && fp == self.fingerprint {
            return;
        }
        self.reload(fp);
    }

    pub fn official_names(&self) -> HashSet<String> {
        self.official_repos.keys().cloned().collect()
    }

    pub fn search(
        &mut self,
        query: &str,
        prefs: &Preferences,
        installed_flatpak_ids: &HashSet<String>,
    ) -> Vec<PackageEntry> {
        self.ensure_loaded();
        let q = query.trim().to_lowercase();
        if q.chars().count() < 3 {
            return Vec::new();
        }

        #[derive(Clone)]
        struct Candidate {
            source_rank: u8,
            match_rank: u8,
            entry: PackageEntry,
        }

        let mut results = Vec::new();

        if prefs.official {
            for pkg in &self.official {
                let rank = if pkg.name_lower == q {
                    Some(0)
                } else if pkg.name_lower.starts_with(&q) {
                    Some(1)
                } else if pkg.name_lower.contains(&q) {
                    Some(2)
                } else if pkg.desc_lower.contains(&q) {
                    Some(3)
                } else {
                    None
                };
                if let Some(match_rank) = rank {
                    results.push(Candidate {
                        source_rank: 0,
                        match_rank,
                        entry: PackageEntry {
                            id: pkg.name.clone(),
                            name: pkg.name.clone(),
                            description: pkg.description.clone(),
                            source: pkg.repo.clone(),
                            command: format!("sudo pacman -S {}", pkg.name),
                        },
                    });
                }
            }
        }

        if prefs.flatpak {
            for pkg in &self.flatpak {
                if installed_flatpak_ids.contains(&pkg.id_lower) {
                    continue;
                }
                let rank = if pkg.name_lower == q || pkg.id_lower == q {
                    Some(0)
                } else if pkg.name_lower.starts_with(&q) || pkg.id_lower.starts_with(&q) {
                    Some(1)
                } else if pkg.name_lower.contains(&q) || pkg.id_lower.contains(&q) {
                    Some(2)
                } else if pkg.desc_lower.contains(&q) {
                    Some(3)
                } else {
                    None
                };
                if let Some(match_rank) = rank {
                    results.push(Candidate {
                        source_rank: 1,
                        match_rank,
                        entry: PackageEntry {
                            id: pkg.id.clone(),
                            name: pkg.name.clone(),
                            description: pkg.description.clone(),
                            source: "Flatpak".into(),
                            command: format!("flatpak install {} {}", pkg.remote, pkg.id),
                        },
                    });
                }
            }
        }

        if prefs.aur
            && let Some(helper) = self.aur_helper.as_deref()
        {
            for pkg in &self.aur {
                if self.official_repos.contains_key(&pkg.name) {
                    continue;
                }
                let rank = if pkg.name_lower == q {
                    Some(0)
                } else if pkg.name_lower.starts_with(&q) {
                    Some(1)
                } else if pkg.name_lower.contains(&q) {
                    Some(2)
                } else {
                    None
                };
                if let Some(match_rank) = rank {
                    results.push(Candidate {
                        source_rank: 2,
                        match_rank,
                        entry: PackageEntry {
                            id: pkg.name.clone(),
                            name: pkg.name.clone(),
                            description: "AUR package".into(),
                            source: "AUR".into(),
                            command: format!("{helper} -S {}", pkg.name),
                        },
                    });
                }
            }
        }

        results.sort_by(|a, b| {
            a.source_rank
                .cmp(&b.source_rank)
                .then_with(|| a.match_rank.cmp(&b.match_rank))
                .then_with(|| {
                    a.entry
                        .name
                        .to_lowercase()
                        .cmp(&b.entry.name.to_lowercase())
                })
                .then_with(|| a.entry.id.cmp(&b.entry.id))
        });

        results
            .into_iter()
            .take(MAX_PACKAGE_RESULTS)
            .map(|c| c.entry)
            .collect()
    }

    pub fn flatpak_remote_for(&mut self, app_id: &str) -> Option<String> {
        self.ensure_loaded();
        self.flatpak
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(app_id))
            .map(|p| p.remote.clone())
    }

    pub fn aur_helper(&mut self) -> Option<String> {
        self.ensure_loaded();
        self.aur_helper.clone()
    }

    pub fn resolve_install_command(
        &mut self,
        package_id: &str,
        source: Option<&str>,
    ) -> Result<Vec<String>, String> {
        if !is_safe_package_id(package_id) {
            return Err("Invalid package identifier.".into());
        }
        self.ensure_loaded();

        if let Some(src) = source {
            if src.eq_ignore_ascii_case("flatpak") {
                return self.flatpak_install_argv(package_id);
            }
            if src.eq_ignore_ascii_case("aur") {
                return self.aur_install_argv(package_id);
            }
            return self.official_install_argv(package_id, Some(src));
        }

        if let Ok(argv) = self.official_install_argv(package_id, None) {
            return Ok(argv);
        }
        if let Ok(argv) = self.flatpak_install_argv(package_id) {
            return Ok(argv);
        }
        if let Ok(argv) = self.aur_install_argv(package_id) {
            return Ok(argv);
        }
        Err(format!(
            "Package candidate is not in the local offline cache: {package_id}"
        ))
    }

    fn official_install_argv(
        &self,
        package_id: &str,
        repo: Option<&str>,
    ) -> Result<Vec<String>, String> {
        let pkg = self
            .official
            .iter()
            .find(|p| p.name == package_id && repo.is_none_or(|r| p.repo.eq_ignore_ascii_case(r)))
            .ok_or_else(|| {
                format!("Package candidate is not in local repository cache: {package_id}")
            })?;
        if !command_exists_in_path("sudo") || !command_exists_in_path("pacman") {
            return Err("sudo or pacman is not available.".into());
        }
        Ok(vec![
            "sudo".into(),
            "pacman".into(),
            "-S".into(),
            pkg.name.clone(),
        ])
    }

    fn flatpak_install_argv(&self, package_id: &str) -> Result<Vec<String>, String> {
        let pkg = self
            .flatpak
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(package_id))
            .ok_or_else(|| {
                format!("Package candidate is not in local Flatpak cache: {package_id}")
            })?;
        if !is_safe_package_id(&pkg.id) {
            return Err("Invalid Flatpak application identifier.".into());
        }
        if !command_exists_in_path("flatpak") {
            return Err("flatpak is not installed.".into());
        }
        let remote = if is_safe_remote(&pkg.remote) {
            pkg.remote.clone()
        } else {
            "flathub".into()
        };
        Ok(vec![
            "flatpak".into(),
            "install".into(),
            remote,
            pkg.id.clone(),
        ])
    }

    fn aur_install_argv(&self, package_id: &str) -> Result<Vec<String>, String> {
        let pkg = self
            .aur
            .iter()
            .find(|p| p.name == package_id)
            .ok_or_else(|| format!("Package candidate is not in local AUR cache: {package_id}"))?;
        let helper = self
            .aur_helper
            .clone()
            .ok_or_else(|| "No AUR helper (paru or yay) is installed.".to_owned())?;
        Ok(vec![helper, "-S".into(), pkg.name.clone()])
    }

    pub fn info_for_package(
        &mut self,
        package_id: &str,
        source: Option<&str>,
    ) -> Result<String, String> {
        if !is_safe_package_id(package_id) {
            return Err("Invalid package identifier.".into());
        }
        self.ensure_loaded();

        let is_flatpak = source.is_some_and(|s| s.eq_ignore_ascii_case("flatpak"));
        let is_aur = source.is_some_and(|s| s.eq_ignore_ascii_case("aur"));

        if is_flatpak
            && let Some(pkg) = self
                .flatpak
                .iter()
                .find(|p| p.id.eq_ignore_ascii_case(package_id))
        {
            return Ok(format_package_info(
                &pkg.name,
                &pkg.id,
                if pkg.version.is_empty() {
                    "unknown"
                } else {
                    &pkg.version
                },
                &format!("Flatpak ({})", pkg.remote),
                &pkg.description,
            ));
        }

        if !is_flatpak
            && !is_aur
            && let Some(pkg) = self.official.iter().find(|p| {
                p.name == package_id && source.is_none_or(|s| p.repo.eq_ignore_ascii_case(s))
            })
        {
            return Ok(format_package_info(
                &pkg.name,
                &pkg.name,
                &pkg.version,
                &pkg.repo,
                &pkg.description,
            ));
        }

        if is_aur || self.aur.iter().any(|p| p.name == package_id) {
            return Ok(format_package_info(
                package_id,
                package_id,
                "AUR (uninstalled)",
                "AUR",
                "Arch User Repository package",
            ));
        }

        if let Some(pkg) = self
            .flatpak
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(package_id))
        {
            return Ok(format_package_info(
                &pkg.name,
                &pkg.id,
                if pkg.version.is_empty() {
                    "unknown"
                } else {
                    &pkg.version
                },
                &format!("Flatpak ({})", pkg.remote),
                &pkg.description,
            ));
        }

        Err(format!("Package information unavailable for {package_id}."))
    }

    pub fn info_for_app(&mut self, app: &IndexedApp) -> String {
        self.ensure_loaded();

        if app.source.as_deref() == Some("Flatpak") {
            let app_id = app.id.strip_suffix(".desktop").unwrap_or(&app.id);
            if let Some(fp) = self
                .flatpak
                .iter()
                .find(|p| p.id.eq_ignore_ascii_case(app_id))
            {
                return format_package_info(
                    &app.name,
                    &fp.id,
                    if fp.version.is_empty() {
                        "installed"
                    } else {
                        &fp.version
                    },
                    &format!("Flatpak ({})", fp.remote),
                    &app.description,
                );
            }
            return format_package_info(
                &app.name,
                app_id,
                "installed",
                "Flatpak",
                &app.description,
            );
        }

        if app.source.as_deref() == Some("AppImage") {
            return format!(
                "{}\nSource: AppImage\nDesktop file: {}",
                app.name,
                app.desktop_path.display()
            );
        }

        if let Some((pkg_name, version)) = lookup_local_pacman_owner(&app.desktop_path) {
            let source = self
                .official_repos
                .get(&pkg_name)
                .cloned()
                .unwrap_or_else(|| "AUR".into());
            return format_package_info(&app.name, &pkg_name, &version, &source, &app.description);
        }

        format!(
            "{}\nSource: Local desktop entry\nDesktop file: {}",
            app.name,
            app.desktop_path.display()
        )
    }

    fn reload(&mut self, fingerprint: Vec<(PathBuf, Option<SystemTime>)>) {
        let (official, official_repos) = load_official_packages();
        let flatpak = load_flatpak_packages();
        let (aur_helper, aur) = load_aur_packages();

        self.official = official;
        self.official_repos = official_repos;
        self.flatpak = flatpak;
        self.aur_helper = aur_helper;
        self.aur = aur;
        self.fingerprint = fingerprint;
        self.loaded = true;
    }
}

pub fn is_safe_package_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@._+-".contains(&b))
}

pub fn is_safe_remote(remote: &str) -> bool {
    !remote.is_empty()
        && remote.len() <= 64
        && !remote.starts_with('-')
        && remote
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn format_package_info(
    title: &str,
    pkg_id: &str,
    version: &str,
    source: &str,
    description: &str,
) -> String {
    let mut lines = vec![
        format!("{title} ({pkg_id})"),
        format!("Version: {version}"),
        format!("Source: {source}"),
    ];
    if !description.is_empty() {
        lines.push(description.to_owned());
    }
    lines.join("\n")
}

fn uses_system_data_dirs() -> bool {
    match env::var_os("XDG_DATA_DIRS").filter(|v| !v.is_empty()) {
        None => true,
        Some(val) => env::split_paths(&val).any(|p| p == Path::new("/usr/share")),
    }
}

fn pacman_sync_dir() -> Option<PathBuf> {
    if let Some(override_dir) = env::var_os("REMYA_APPS_PACMAN_SYNC_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Some(override_dir);
    }
    uses_system_data_dirs().then(|| PathBuf::from("/var/lib/pacman/sync"))
}

fn pacman_local_dir() -> Option<PathBuf> {
    if let Some(override_dir) = env::var_os("REMYA_APPS_PACMAN_LOCAL_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Some(override_dir);
    }
    uses_system_data_dirs().then(|| PathBuf::from("/var/lib/pacman/local"))
}

fn flatpak_appstream_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(user_data) = absolute_env("XDG_DATA_HOME")
        .or_else(|| absolute_env("HOME").map(|h| h.join(".local/share")))
    {
        roots.push(user_data.join("flatpak/appstream"));
    }
    if uses_system_data_dirs() {
        roots.push(PathBuf::from("/var/lib/flatpak/appstream"));
    }
    roots
}

fn aur_cache_candidates() -> Vec<(&'static str, PathBuf)> {
    let mut out = Vec::new();
    let cache_base =
        absolute_env("XDG_CACHE_HOME").or_else(|| absolute_env("HOME").map(|h| h.join(".cache")));
    if let Some(base) = cache_base {
        out.push(("paru", base.join("paru/completion.cache")));
        out.push(("yay", base.join("yay/completion.cache")));
    }
    out
}

fn source_fingerprint() -> Vec<(PathBuf, Option<SystemTime>)> {
    let mut fp = Vec::new();
    if let Some(sync) = pacman_sync_dir()
        && let Ok(entries) = fs::read_dir(&sync)
    {
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("db"))
            .collect();
        paths.sort();
        for p in paths {
            fp.push((p.clone(), fs::metadata(&p).and_then(|m| m.modified()).ok()));
        }
    }
    for root in flatpak_appstream_roots() {
        for (path, _) in collect_appstream_files(&root) {
            fp.push((
                path.clone(),
                fs::metadata(&path).and_then(|m| m.modified()).ok(),
            ));
        }
    }
    for (_, path) in aur_cache_candidates() {
        if path.is_file() {
            fp.push((
                path.clone(),
                fs::metadata(&path).and_then(|m| m.modified()).ok(),
            ));
        }
    }
    fp
}

fn load_official_packages() -> (Vec<OfficialPkg>, HashMap<String, String>) {
    let Some(sync) = pacman_sync_dir() else {
        return (Vec::new(), HashMap::new());
    };
    let Ok(entries) = fs::read_dir(&sync) else {
        return (Vec::new(), HashMap::new());
    };
    let mut db_files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("db"))
        .collect();

    db_files.sort_by(|a, b| {
        let rank = |p: &Path| match p.file_stem().and_then(|s| s.to_str()) {
            Some("core") => 0,
            Some("extra") => 1,
            Some("multilib") => 2,
            _ => 3,
        };
        rank(a).cmp(&rank(b)).then_with(|| a.cmp(b))
    });

    let mut packages = Vec::new();
    let mut repos = HashMap::new();

    for db_path in db_files {
        let Some(repo) = db_path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(file) = File::open(&db_path) else {
            continue;
        };
        let _ = parse_pacman_sync_db(file, repo, &mut packages, &mut repos);
    }

    (packages, repos)
}

fn parse_pacman_sync_db(
    file: File,
    repo: &str,
    out: &mut Vec<OfficialPkg>,
    repos: &mut HashMap<String, String>,
) -> io::Result<()> {
    let mut reader = BufReader::new(file);
    let mut magic = [0u8; 4];
    let n = reader.read(&mut magic)?;
    let mut prefix = Vec::with_capacity(n);
    prefix.extend_from_slice(&magic[..n]);
    let combined = prefix.chain(reader);

    if n >= 2 && magic[0] == 0x1f && magic[1] == 0x8b {
        let mut dec = GzDecoder::new(combined);
        parse_tar_descs(&mut dec, repo, out, repos)
    } else if n >= 4 && magic == [0x28, 0xb5, 0x2f, 0xfd] {
        let mut dec = StreamingDecoder::new(combined).map_err(io::Error::other)?;
        parse_tar_descs(&mut dec, repo, out, repos)
    } else {
        let mut plain = combined;
        parse_tar_descs(&mut plain, repo, out, repos)
    }
}

fn parse_tar_descs<R: Read>(
    reader: &mut R,
    repo: &str,
    out: &mut Vec<OfficialPkg>,
    repos: &mut HashMap<String, String>,
) -> io::Result<()> {
    let mut header = [0u8; 512];
    let mut desc_buf = Vec::new();

    loop {
        if !read_exact_or_eof(reader, &mut header)? {
            break;
        }
        if header.iter().all(|&b| b == 0) {
            break;
        }
        let name = cstr_slice(&header[..100]);
        let size = parse_octal(&header[124..136]).unwrap_or(0);
        let typeflag = header[156];
        let is_desc = (typeflag == b'0' || typeflag == 0) && name.ends_with("/desc");

        if is_desc && size <= 1024 * 1024 {
            desc_buf.resize(size, 0);
            reader.read_exact(&mut desc_buf)?;
            if let Ok(text) = std::str::from_utf8(&desc_buf)
                && let Some((pkg_name, version, description)) = parse_pacman_desc(text)
            {
                repos
                    .entry(pkg_name.clone())
                    .or_insert_with(|| repo.to_owned());
                out.push(OfficialPkg {
                    name_lower: pkg_name.to_lowercase(),
                    desc_lower: description.to_lowercase(),
                    name: pkg_name,
                    version,
                    description,
                    repo: repo.to_owned(),
                });
            }
        } else {
            skip_bytes(reader, size)?;
        }

        let rem = size % 512;
        if rem != 0 {
            skip_bytes(reader, 512 - rem)?;
        }
    }
    Ok(())
}

fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut offset = 0;
    while offset < buf.len() {
        let n = reader.read(&mut buf[offset..])?;
        if n == 0 {
            return if offset == 0 {
                Ok(false)
            } else {
                Err(io::Error::from(io::ErrorKind::UnexpectedEof))
            };
        }
        offset += n;
    }
    Ok(true)
}

fn skip_bytes<R: Read>(reader: &mut R, mut remaining: usize) -> io::Result<()> {
    let mut sink = [0u8; 4096];
    while remaining > 0 {
        let to_read = remaining.min(sink.len());
        let n = reader.read(&mut sink[..to_read])?;
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        remaining -= n;
    }
    Ok(())
}

fn cstr_slice(bytes: &[u8]) -> &str {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

fn parse_octal(bytes: &[u8]) -> Option<usize> {
    let s = cstr_slice(bytes).trim();
    if s.is_empty() {
        return Some(0);
    }
    usize::from_str_radix(s, 8).ok()
}

fn parse_pacman_desc(text: &str) -> Option<(String, String, String)> {
    let mut section = "";
    let mut name = None;
    let mut version = String::new();
    let mut desc = String::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('%') && trimmed.ends_with('%') {
            section = trimmed;
            continue;
        }
        if trimmed.is_empty() {
            continue;
        }
        match section {
            "%NAME%" if name.is_none() => name = Some(trimmed.to_owned()),
            "%VERSION%" if version.is_empty() => version = trimmed.to_owned(),
            "%DESC%" => {
                if !desc.is_empty() {
                    desc.push(' ');
                }
                desc.push_str(trimmed);
            }
            _ => {}
        }
    }
    name.map(|n| (n, version, desc))
}

fn collect_appstream_files(root: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let Ok(remotes) = fs::read_dir(root) else {
        return out;
    };
    for remote_entry in remotes.flatten() {
        let remote_path = remote_entry.path();
        if !remote_path.is_dir() {
            continue;
        }
        let Some(remote_name) = remote_entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if !is_safe_remote(&remote_name) {
            continue;
        }
        let Ok(arches) = fs::read_dir(&remote_path) else {
            continue;
        };
        for arch_entry in arches.flatten() {
            let active = arch_entry.path().join("active");
            let xml = active.join("appstream.xml");
            let gz = active.join("appstream.xml.gz");
            if xml.is_file() {
                out.push((xml, remote_name.clone()));
            } else if gz.is_file() {
                out.push((gz, remote_name.clone()));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn load_flatpak_packages() -> Vec<FlatpakPkg> {
    let mut out = Vec::new();
    let mut seen_ids = HashSet::new();

    for root in flatpak_appstream_roots() {
        for (path, remote) in collect_appstream_files(&root) {
            let Ok(file) = File::open(&path) else {
                continue;
            };
            let is_gz = path.extension().and_then(|e| e.to_str()) == Some("gz");
            let parsed = if is_gz {
                parse_appstream_xml(BufReader::new(GzDecoder::new(file)), &remote)
            } else {
                parse_appstream_xml(BufReader::new(file), &remote)
            };
            for pkg in parsed {
                if is_safe_package_id(&pkg.id) && seen_ids.insert(pkg.id_lower.clone()) {
                    out.push(pkg);
                }
            }
        }
    }
    out
}

fn parse_appstream_xml<R: io::BufRead>(reader: R, remote: &str) -> Vec<FlatpakPkg> {
    let mut xml = Reader::from_reader(reader);
    xml.config_mut().trim_text(true);

    let mut out = Vec::new();
    let mut buf = Vec::new();

    let mut in_desktop_component = false;
    let mut depth = 0usize;
    let mut current_tag = String::new();
    let mut current_has_lang = false;

    let mut id = String::new();
    let mut name = String::new();
    let mut name_fallback = String::new();
    let mut summary = String::new();
    let mut summary_fallback = String::new();
    let mut version = String::new();

    loop {
        match xml.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                if !in_desktop_component && tag == "component" {
                    let is_desktop = e.attributes().flatten().any(|a| {
                        a.key.as_ref() == b"type"
                            && matches!(a.value.as_ref(), b"desktop" | b"desktop-application")
                    });
                    if is_desktop {
                        in_desktop_component = true;
                        depth = 1;
                        id.clear();
                        name.clear();
                        name_fallback.clear();
                        summary.clear();
                        summary_fallback.clear();
                        version.clear();
                    }
                } else if in_desktop_component {
                    depth += 1;
                    if depth == 2 && (tag == "id" || tag == "name" || tag == "summary") {
                        current_tag = tag;
                        current_has_lang = e
                            .attributes()
                            .flatten()
                            .any(|a| a.key.as_ref() == b"xml:lang");
                    } else if tag == "release" && version.is_empty() {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"version" {
                                version = String::from_utf8_lossy(&attr.value).into_owned();
                            }
                        }
                    }
                }
            }
            Ok(Event::Empty(e)) if in_desktop_component => {
                if e.name().as_ref() == b"release" && version.is_empty() {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"version" {
                            version = String::from_utf8_lossy(&attr.value).into_owned();
                        }
                    }
                }
            }
            Ok(Event::Text(e)) if in_desktop_component && depth == 2 && !current_tag.is_empty() => {
                if let Ok(text) = e.xml_content(quick_xml::XmlVersion::Implicit1_0) {
                    let val = text.trim();
                    if !val.is_empty() {
                        match current_tag.as_str() {
                            "id" if id.is_empty() => {
                                id = val.strip_suffix(".desktop").unwrap_or(val).to_owned();
                            }
                            "name" => {
                                if !current_has_lang && name.is_empty() {
                                    name = val.to_owned();
                                } else if name_fallback.is_empty() {
                                    name_fallback = val.to_owned();
                                }
                            }
                            "summary" => {
                                if !current_has_lang && summary.is_empty() {
                                    summary = val.to_owned();
                                } else if summary_fallback.is_empty() {
                                    summary_fallback = val.to_owned();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(e)) if in_desktop_component => {
                if depth == 1 && e.name().as_ref() == b"component" {
                    let final_name = if !name.is_empty() {
                        std::mem::take(&mut name)
                    } else {
                        std::mem::take(&mut name_fallback)
                    };
                    let final_summary = if !summary.is_empty() {
                        std::mem::take(&mut summary)
                    } else {
                        std::mem::take(&mut summary_fallback)
                    };
                    if !id.is_empty() && !final_name.is_empty() {
                        out.push(FlatpakPkg {
                            id_lower: id.to_lowercase(),
                            name_lower: final_name.to_lowercase(),
                            desc_lower: final_summary.to_lowercase(),
                            id: std::mem::take(&mut id),
                            name: final_name,
                            description: final_summary,
                            version: std::mem::take(&mut version),
                            remote: remote.to_owned(),
                        });
                    }
                    in_desktop_component = false;
                    depth = 0;
                    current_tag.clear();
                } else {
                    if depth == 2 {
                        current_tag.clear();
                    }
                    depth = depth.saturating_sub(1);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

fn load_aur_packages() -> (Option<String>, Vec<AurPkg>) {
    let installed_helper = ["paru", "yay"]
        .into_iter()
        .find(|bin| command_exists_in_path(bin));

    for (helper_name, cache_path) in aur_cache_candidates() {
        if !cache_path.is_file() {
            continue;
        }
        let chosen_helper = if command_exists_in_path(helper_name) {
            Some(helper_name.to_owned())
        } else {
            installed_helper.map(str::to_owned)
        };
        let Some(helper) = chosen_helper else {
            continue;
        };
        let Ok(content) = fs::read_to_string(&cache_path) else {
            continue;
        };
        let mut pkgs = Vec::new();
        for line in content.lines() {
            let Some((name, repo)) = line.split_once('\t') else {
                continue;
            };
            if repo.trim() == "AUR" && is_safe_package_id(name) {
                pkgs.push(AurPkg {
                    name_lower: name.to_lowercase(),
                    name: name.to_owned(),
                });
            }
        }
        return (Some(helper), pkgs);
    }

    (installed_helper.map(str::to_owned), Vec::new())
}

pub fn command_exists_in_path(name: &str) -> bool {
    let Some(path_var) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path_var).any(|dir| {
        let candidate = dir.join(name);
        is_executable_file(&candidate)
    })
}

pub fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && (meta.permissions().mode() & 0o111 != 0))
}

fn lookup_local_pacman_owner(desktop_path: &Path) -> Option<(String, String)> {
    let rel = desktop_path
        .strip_prefix("/")
        .ok()
        .and_then(|p| p.to_str())?;
    let local = pacman_local_dir()?;
    let entries = fs::read_dir(local).ok()?;
    for entry in entries.flatten() {
        let dir = entry.path();
        let files_path = dir.join("files");
        let Ok(files_text) = fs::read_to_string(&files_path) else {
            continue;
        };
        if files_text.lines().any(|line| line == rel) {
            let desc_text = fs::read_to_string(dir.join("desc")).ok()?;
            let (name, version, _) = parse_pacman_desc(&desc_text)?;
            return Some((name, version));
        }
    }
    None
}
