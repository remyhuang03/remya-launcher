use crate::model::{Entry, Hit, Policy, decode_id, expand_path, hidden_name, path_id, places};
use crate::query::Query;
use base64::{Engine, engine::general_purpose::STANDARD};
use ignore::WalkBuilder;
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::fs;
use std::io::{ErrorKind, Read};
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

struct Budget<'a> {
    deadline: Instant,
    cancelled: &'a AtomicBool,
}
impl Budget<'_> {
    fn stopped(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed) || Instant::now() >= self.deadline
    }
}

struct Results {
    entries: Vec<Entry>,
    matched: usize,
    partial: bool,
    diagnostic: Option<String>,
    extra: Value,
}
impl Results {
    fn new() -> Self {
        Self {
            entries: vec![],
            matched: 0,
            partial: false,
            diagnostic: None,
            extra: json!({}),
        }
    }
    fn finish(self, mode: &str, started: Instant) -> Value {
        let mut data = self.extra;
        data["entries"] = json!(self.entries);
        data["done"] = json!(true);
        data["mode"] = json!(mode);
        data["matchedCount"] = json!(self.matched);
        data["partial"] = json!(self.partial);
        data["elapsedMs"] = json!(started.elapsed().as_millis());
        if let Some(d) = self.diagnostic {
            data["diagnostic"] = json!(d);
        }
        data
    }
}

struct Scan<'a> {
    query: &'a Query,
    policy: Arc<Policy>,
    hidden: bool,
    ignore: bool,
    limit: usize,
    changed: Option<HashSet<PathBuf>>,
    budget: Budget<'a>,
}

impl Scan<'_> {
    fn allows(&self, path: &Path) -> bool {
        self.changed
            .as_ref()
            .is_none_or(|paths| paths.contains(path))
    }
    fn walk(
        &self,
        root: &Path,
        recursive: bool,
        eligible: Option<Arc<HashSet<PathBuf>>>,
    ) -> ignore::Walk {
        let policy = Arc::clone(&self.policy);
        let mut walker = WalkBuilder::new(root);
        walker
            .hidden(!self.hidden)
            .follow_links(false)
            .require_git(false)
            .git_ignore(self.ignore)
            .git_exclude(self.ignore)
            .git_global(self.ignore)
            .ignore(self.ignore)
            .parents(self.ignore)
            .same_file_system(true)
            .filter_entry(move |e| {
                eligible.as_ref().is_none_or(|set| set.contains(e.path()))
                    && policy.visible_path(e.path())
            });
        if !recursive {
            walker.max_depth(Some(1));
        }
        walker.build()
    }
}

pub fn search(
    request: &Value,
    cancelled: &AtomicBool,
    on_batch: impl Fn(Value),
) -> Result<Value, String> {
    let started = Instant::now();
    let preferences = &request["preferences"];
    let policy = Arc::new(Policy::new(preferences));
    let settings = &preferences["files-search"];
    let raw = request["query"].as_str().unwrap_or("");
    if raw.len() > 8192 {
        return Err("Search text is too long.".into());
    }
    let hint = request["mode"].as_str().unwrap_or("");
    let dir = request["directoryId"].as_str().map(decode_id).transpose()?;
    let anchor = dir.as_deref().unwrap_or(&policy.home);
    let q = Query::parse(
        raw,
        hint,
        anchor,
        &policy.home,
        request["filters"].as_array().map(Vec::as_slice),
    )?;
    let path_mode = hint == "path"
        || q.text.starts_with(['~', '/'])
        || (q.text.starts_with('$') && q.text.contains('/'))
        || q.text.starts_with("./")
        || q.text.starts_with("../")
        || q.text == "."
        || q.text == "..";
    let mode = if q.contents {
        "contents"
    } else if path_mode {
        "path"
    } else if q.text.trim().is_empty() && !q.filtered && dir.is_none() {
        "empty"
    } else {
        "names"
    };
    let mut result = Results::new();
    result.diagnostic = q.diagnostic.clone();
    let explicit_hidden = q.hidden.or(request["showHidden"].as_bool());
    let hidden =
        explicit_hidden.unwrap_or_else(|| settings["show_hidden"].as_bool().unwrap_or(false));
    let limit = settings["result_limit"]
        .as_u64()
        .unwrap_or(200)
        .clamp(20, 500) as usize;
    let limit = request["limit"]
        .as_u64()
        .map(|n| n.clamp(1, limit as u64) as usize)
        .unwrap_or(limit);
    let mut scan = Scan {
        query: &q,
        policy: Arc::clone(&policy),
        hidden,
        ignore: settings["respect_ignore"].as_bool().unwrap_or(true)
            && !request["includeIgnored"].as_bool().unwrap_or(false),
        limit,
        changed: None,
        budget: Budget {
            deadline: started
                + if request["compact"] == true {
                    Duration::from_millis(180)
                } else {
                    Duration::from_secs(3)
                },
            cancelled,
        },
    };
    if mode == "empty" {
        let mut seen = HashSet::new();
        match crate::history::snapshot(preferences, &policy) {
            Ok(history) => {
                for path in history.pinned.iter() {
                    if let Some(e) = Entry::new(path, &policy, "pinned")
                        && seen.insert(e.id.clone())
                    {
                        result.entries.push(e);
                    }
                }
                result.entries.extend(
                    places(&policy)
                        .into_iter()
                        .filter(|e| seen.insert(e.id.clone())),
                );
                for path in history.recent.iter() {
                    if result.entries.len() >= limit {
                        result.partial = true;
                        break;
                    }
                    if !hidden
                        && path
                            .strip_prefix(&policy.home)
                            .unwrap_or(path)
                            .components()
                            .any(|p| hidden_name(p.as_os_str()))
                    {
                        continue;
                    }
                    if let Some(e) = Entry::new(path, &policy, "recent")
                        && seen.insert(e.id.clone())
                    {
                        result.entries.push(e);
                    }
                }
            }
            Err(e) => {
                result.entries = places(&policy);
                result.diagnostic = Some(e);
            }
        }
        result.matched = result.entries.len();
    } else {
        let requested: Vec<_> = q
            .scope
            .as_ref()
            .or(dir.as_ref())
            .map(|p| vec![p.clone()])
            .unwrap_or_else(|| policy.roots.clone());
        let mut roots: Vec<_> = requested
            .into_iter()
            .filter(|p| policy.allowed(p) && p.is_dir())
            .filter_map(|p| p.canonicalize().ok())
            .collect();
        roots.sort();
        roots.dedup();
        if q.changed {
            if roots.len() != 1 {
                return Err("Choose one repository with in: to search changed files.".into());
            }
            scan.changed = Some(changed_paths(&roots[0], &scan.budget)?);
        }
        if mode == "path" {
            path_search(&mut scan, anchor, explicit_hidden.is_some(), &mut result)?;
        } else if roots.is_empty() {
            result.diagnostic = Some("No accessible search folder.".into());
        } else if mode == "contents" {
            if !settings["contents_enabled"].as_bool().unwrap_or(true) {
                result.diagnostic = Some("Content search is disabled in Files settings.".into());
            } else {
                content_search(&scan, &roots, settings, &mut result)?;
            }
        } else {
            let history = crate::history::snapshot(preferences, &policy);
            if let Err(e) = &history {
                result.diagnostic = Some(e.clone());
            }
            let history = history.unwrap_or_default();
            early_names(&scan, &roots, &history, &mut result);
            if !result.entries.is_empty() && !cancelled.load(Ordering::Relaxed) {
                on_batch(
                    json!({"done":false,"mode":"names","entries":result.entries,"matchedCount":result.entries.len(),"elapsedMs":started.elapsed().as_millis(),"partial":false}),
                );
            }
            let scores = history.scores;
            names_search(&scan, &roots, &scores, &mut result, started, &on_batch);
        }
    }
    if matches!(mode, "path" | "contents")
        && !result.entries.is_empty()
        && !cancelled.load(Ordering::Relaxed)
        && let Err(error) = crate::history::mark_pins(&mut result.entries)
    {
        result.diagnostic.get_or_insert(error);
    }
    result.partial |= cancelled.load(Ordering::Relaxed);
    Ok(result.finish(mode, started))
}

// Walk only the ancestors of the early candidates so ignore files apply exactly as in the full scan.
fn early_names(
    scan: &Scan<'_>,
    roots: &[PathBuf],
    history: &crate::history::Snapshot,
    result: &mut Results,
) {
    let mut candidates = Vec::new();
    candidates.extend(
        history
            .pinned
            .iter()
            .take(32)
            .map(|p| (p.clone(), "pinned")),
    );
    candidates.extend(
        history
            .recent
            .iter()
            .take(32)
            .map(|p| (p.clone(), "recent")),
    );
    candidates.extend(
        places(&scan.policy)
            .into_iter()
            .filter_map(|e| decode_id(&e.id).ok().map(|p| (p, "places"))),
    );
    candidates.retain(|(p, _)| {
        scan.allows(p)
            && roots.iter().any(|r| {
                p.strip_prefix(r).ok().is_some_and(|rel| {
                    scan.query
                        .rank(
                            &p.file_name().unwrap_or_default().to_string_lossy(),
                            &rel.parent().unwrap_or(Path::new("")).to_string_lossy(),
                        )
                        .is_some()
                })
            })
    });
    if candidates.is_empty() {
        return;
    }
    let ancestors = Arc::new(
        candidates
            .iter()
            .flat_map(|(p, _)| p.ancestors().map(Path::to_path_buf))
            .collect(),
    );
    let mut visible = HashSet::new();
    let until = Instant::now() + Duration::from_millis(120);
    for root in roots {
        for item in scan
            .walk(root, true, Some(Arc::clone(&ancestors)))
            .flatten()
        {
            if scan.budget.stopped() || Instant::now() > until {
                break;
            }
            visible.insert(item.into_path());
        }
    }
    let mut recent = 0;
    for (p, group) in candidates {
        if (group == "recent" && recent >= 3) || !visible.remove(&p) {
            continue;
        }
        if let Some(entry) = Entry::new(&p, &scan.policy, group)
            && scan.query.accepts(&entry)
        {
            result.entries.push(entry);
            recent += usize::from(group == "recent");
            if result.entries.len() >= scan.limit {
                break;
            }
        }
    }
}

fn names_search(
    scan: &Scan<'_>,
    roots: &[PathBuf],
    scores: &HashMap<PathBuf, f64>,
    result: &mut Results,
    started: Instant,
    on_batch: &impl Fn(Value),
) {
    let early: HashSet<_> = result.entries.iter().map(|e| e.id.clone()).collect();
    let limit = scan.limit.saturating_sub(early.len());
    result.matched = early.len();
    let mut ranked = Vec::new();
    let mut next_batch = started + Duration::from_millis(75);
    let mut dirty = false;
    let mut seen = HashSet::new();
    let mut visited = 0;
    for root in roots {
        for item in scan.walk(root, true, None) {
            if scan.budget.stopped() || visited >= 200_000 {
                result.partial = true;
                break;
            }
            let Ok(item) = item else {
                continue;
            };
            if item.depth() == 0 {
                continue;
            }
            visited += 1;
            if dirty && (visited as usize).is_multiple_of(64) && Instant::now() >= next_batch {
                let entries: Vec<&Entry> = result
                    .entries
                    .iter()
                    .chain(ranked.iter().map(|(_, entry)| entry))
                    .collect();
                on_batch(
                    json!({"done":false,"mode":"names","entries":entries,"matchedCount":result.matched,"elapsedMs":started.elapsed().as_millis(),"partial":false}),
                );
                dirty = false;
                next_batch = Instant::now() + Duration::from_millis(240);
            }
            let path = item.path();
            if !scan.allows(path) {
                continue;
            }
            let name = item.file_name().to_string_lossy();
            let parent = path
                .parent()
                .and_then(|p| p.strip_prefix(root).ok())
                .unwrap_or(Path::new(""))
                .to_string_lossy();
            let Some(rank) = scan.query.rank(&name, &parent) else {
                continue;
            };
            if seen.contains(path) {
                continue;
            }
            let Some(entry) = Entry::new(path, &scan.policy, "results") else {
                continue;
            };
            if early.contains(&entry.id) || !scan.query.accepts(&entry) {
                continue;
            }
            result.matched += 1;
            let key = (
                std::cmp::Reverse(rank),
                std::cmp::Reverse((scores.get(path).copied().unwrap_or_default() * 1000.0) as i64),
                item.depth(),
                name.len(),
                path.to_owned(),
            );
            let at = ranked.partition_point(|(k, _)| k < &key);
            if at < limit {
                seen.insert(path.to_owned());
                ranked.insert(at, (key, entry));
                dirty = true;
                if ranked.len() > limit
                    && let Some(((.., removed), _)) = ranked.pop()
                {
                    seen.remove(&removed);
                }
            }
        }
    }
    result.partial |= result.matched > scan.limit;
    result.entries.extend(ranked.into_iter().map(|(_, e)| e));
}

fn path_search(
    scan: &mut Scan<'_>,
    anchor: &Path,
    explicit_hidden: bool,
    result: &mut Results,
) -> Result<(), String> {
    let raw = &scan.query.text;
    let path = expand_path(
        raw,
        scan.query.scope.as_deref().unwrap_or(anchor),
        &scan.policy.home,
    )?;
    if let Some(scope) = &scan.query.scope
        && (!scope.is_dir()
            || !path.starts_with(scope)
            || !scope.canonicalize().is_ok_and(|base| {
                path.ancestors()
                    .find_map(|p| p.canonicalize().ok())
                    .is_some_and(|dir| dir.starts_with(base))
            }))
    {
        return Err("This path is outside the selected folder.".into());
    }
    if !scan.policy.allowed(&path) {
        return Err("This location is unavailable.".into());
    }
    let browse = raw.is_empty()
        || raw.ends_with('/')
        || raw.ends_with("/..")
        || matches!(raw.as_str(), "~" | "." | "..");
    let (directory, prefix) = if raw.ends_with("/.") {
        (path.as_path(), String::from("."))
    } else if browse && path.is_dir() {
        (path.as_path(), String::new())
    } else {
        (
            path.parent().unwrap_or(&path),
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase(),
        )
    };
    if !explicit_hidden && prefix.starts_with('.') {
        scan.hidden = true;
    }
    if directory.is_dir() {
        result.extra["directoryId"] = json!(path_id(directory));
        result.extra["directory"] = json!(scan.policy.display(directory));
        if !fs::metadata(directory).is_ok_and(|m| m.is_dir()) || fs::read_dir(directory).is_err() {
            return Err("Cannot read this folder.".into());
        }
        let mut sorted = Vec::new();
        for (i, item) in scan.walk(directory, false, None).enumerate() {
            if i >= 5000 || scan.budget.stopped() {
                result.partial = true;
                break;
            }
            let Ok(item) = item else {
                continue;
            };
            if item.depth() == 0
                || !scan.allows(item.path())
                || !item
                    .file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .starts_with(&prefix)
            {
                continue;
            }
            if let Some(entry) = Entry::new(item.path(), &scan.policy, "results")
                && scan.query.accepts(&entry)
            {
                sorted.push((
                    entry.kind != "directory",
                    gio::glib::FilenameCollationKey::from(&entry.name),
                    entry,
                ));
            }
        }
        result.matched = sorted.len();
        if !result.partial {
            result.extra["totalCount"] = json!(result.matched);
        }
        sorted.sort_by(|a, b| (&a.0, &a.1, &a.2.id).cmp(&(&b.0, &b.1, &b.2.id)));
        result.partial |= sorted.len() > scan.limit;
        result.entries = sorted
            .into_iter()
            .take(scan.limit)
            .map(|(_, _, e)| e)
            .collect();
    }
    if fs::symlink_metadata(&path).is_err_and(|e| e.kind() == ErrorKind::NotFound)
        && !path.components().any(|p| p == Component::ParentDir)
        && let Some(existing) = path.ancestors().find(|p| fs::symlink_metadata(p).is_ok())
        && existing.is_dir()
        && scan.policy.allowed(existing)
        && let Ok(relative) = path.strip_prefix(existing)
        && let Some(name) = path.file_name().and_then(|n| n.to_str())
        && let Some(relative) = relative.to_str()
    {
        result.extra["directoryId"] = json!(path_id(existing));
        result.extra["directoryVersion"] = json!(crate::actions::version(
            &fs::symlink_metadata(existing).map_err(|_| "Folder changed.")?
        ));
        use rustix::fs::{Access, AtFlags, CWD, accessat};
        result.extra["directory"] = json!(scan.policy.display(existing));
        result.extra["createWritable"] = json!(
            accessat(
                CWD,
                existing,
                Access::WRITE_OK | Access::EXEC_OK,
                AtFlags::EACCESS
            )
            .is_ok()
        );
        result.extra["createName"] = json!(name);
        result.extra["createRelative"] = json!(relative);
        result.extra["createPath"] = json!(path.to_string_lossy());
        result.extra["createAncestors"] = json!(
            Path::new(relative)
                .parent()
                .into_iter()
                .flat_map(Path::components)
                .map(|p| p.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
        );
        result.extra["createDirectoryOnly"] = json!(raw.ends_with('/'));
    }
    Ok(())
}

// Read a child without blocking cancellation on a quiet pipe or an unterminated record.
fn command_records(
    command: &mut Command,
    delimiter: u8,
    budget: &Budget<'_>,
    cap: usize,
    mut record: impl FnMut(&[u8]) -> bool,
) -> Result<(Option<i32>, bool), String> {
    use rustix::process::{Signal, getpid, getppid, set_parent_process_death_signal};
    use std::os::unix::process::CommandExt;
    let parent = getpid();
    // Only async-signal-safe syscalls run here, between fork and exec.
    unsafe {
        command.pre_exec(move || {
            set_parent_process_death_signal(Some(Signal::KILL))?;
            if getppid() != Some(parent) {
                return Err(std::io::Error::from_raw_os_error(4));
            }
            Ok(())
        });
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            format!(
                "Could not start {}: {e}",
                command.get_program().to_string_lossy()
            )
        })?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let nonblocking =
        fcntl_getfl(&stdout).and_then(|flags| fcntl_setfl(&stdout, flags | OFlags::NONBLOCK));
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 32768];
    let mut limited = nonblocking.is_err();
    while !limited {
        if budget.stopped() {
            limited = true;
            break;
        }
        match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buffer.extend_from_slice(&chunk[..n]);
                let mut offset = 0;
                while let Some(end) = buffer[offset..].iter().position(|&b| b == delimiter) {
                    if end > cap || !record(&buffer[offset..offset + end]) {
                        limited = true;
                        break;
                    }
                    offset += end + 1;
                }
                buffer.drain(..offset);
                limited |= buffer.len() > cap;
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(3))
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(_) => {
                limited = true;
            }
        }
    }
    drop(stdout);
    if limited {
        let _ = child.kill();
    }
    let status = child
        .wait()
        .map_err(|_| "Could not finish search process.")?;
    Ok((status.code(), limited))
}

fn changed_paths(root: &Path, budget: &Budget<'_>) -> Result<HashSet<PathBuf>, String> {
    let git = || {
        let mut cmd = Command::new("git");
        cmd.current_dir(root)
            .args(["--no-optional-locks", "-c", "core.fsmonitor=false"]);
        cmd
    };
    let mut repo = None;
    let (code, partial) = command_records(
        git().args(["rev-parse", "--show-toplevel"]),
        b'\n',
        budget,
        4096,
        |b| {
            repo = Some(PathBuf::from(OsString::from_vec(b.to_vec())));
            true
        },
    )?;
    let repo = repo
        .filter(|_| code == Some(0) && !partial)
        .ok_or("Choose a Git repository with in: to search changed files.")?;
    let mut paths = HashSet::new();
    let (code, partial) = command_records(
        git().args([
            "status",
            "--porcelain=v1",
            "--no-renames",
            "-z",
            "--untracked-files=all",
        ]),
        0,
        budget,
        65536,
        |b| {
            if let Some(path) = b.get(3..) {
                paths.insert(repo.join(OsString::from_vec(path.into())));
            }
            paths.len() < 50000
        },
    )?;
    if partial || code != Some(0) {
        return Err("Could not read the repository status within the search limit.".into());
    }
    Ok(paths)
}

fn rg_bytes(value: &Value) -> Option<Vec<u8>> {
    value["text"]
        .as_str()
        .map(|s| s.as_bytes().to_vec())
        .or_else(|| STANDARD.decode(value["bytes"].as_str()?).ok())
}

fn excluded_glob(path: &str) -> String {
    let mut escaped = String::from("!/");
    for c in path.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '{' | '}' | '\\' | '!') {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

fn content_search(
    scan: &Scan<'_>,
    roots: &[PathBuf],
    settings: &Value,
    result: &mut Results,
) -> Result<(), String> {
    let text = scan.query.text.trim();
    let regex = text.strip_prefix('/').and_then(|s| s.strip_suffix('/'));
    let pattern = regex.unwrap_or(text);
    if pattern.is_empty() || text.chars().count() < 3 {
        result.diagnostic = Some("Enter at least three characters to search file contents.".into());
        return Ok(());
    }
    let max = settings["contents_max_bytes"]
        .as_u64()
        .unwrap_or(2 * 1024 * 1024)
        .clamp(1024, 10 * 1024 * 1024);
    let mut positions: HashMap<PathBuf, usize> = HashMap::new();
    let mut hits = 0;
    let mut retained = 0;
    for root in roots {
        let mut cmd = Command::new("rg");
        cmd.current_dir(root)
            .args([
                "--no-config",
                "--json",
                "--smart-case",
                "--threads",
                "2",
                "--one-file-system",
                "--no-require-git",
                "--max-filesize",
            ])
            .arg(max.to_string());
        if regex.is_none() {
            cmd.arg("--fixed-strings");
        }
        if scan.hidden {
            cmd.arg("--hidden");
        }
        if !scan.ignore {
            cmd.arg("--no-ignore");
        }
        for private in scan.policy.private_names() {
            cmd.arg("--glob").arg(format!("!{private}"));
        }
        for excluded in &scan.policy.exclusions {
            if let Ok(path) = excluded.strip_prefix(root) {
                let path = path
                    .to_str()
                    .ok_or("Cannot safely exclude a folder with this search scope.")?;
                cmd.arg("--glob").arg(excluded_glob(path));
            }
        }
        cmd.args(["--", pattern, "."]);
        let mut invalid = false;
        let (code, limited) =
            command_records(&mut cmd, b'\n', &scan.budget, 8 * 1024 * 1024, |record| {
                // Only match events carry rows. Parsing summary or begin/end is unnecessary.
                let Ok(event) = serde_json::from_slice::<Value>(record) else {
                    invalid = true;
                    return false;
                };
                if event["type"] != "match" {
                    return true;
                }
                let data = &event["data"];
                let Some(raw) = rg_bytes(&data["path"]) else {
                    invalid = true;
                    return false;
                };
                let rel = PathBuf::from(OsString::from_vec(raw));
                let path = root.join(rel.strip_prefix(".").unwrap_or(&rel));
                if !scan.allows(&path) {
                    return true;
                }
                let index = if let Some(&index) = positions.get(&path) {
                    index
                } else if let Some(entry) = Entry::new(&path, &scan.policy, "results")
                    && scan.query.accepts(&entry)
                {
                    result.matched += 1;
                    if positions.len() >= scan.limit {
                        return false;
                    }
                    let index = result.entries.len();
                    positions.insert(path, index);
                    result.entries.push(entry);
                    index
                } else {
                    return true;
                };
                let entry = &mut result.entries[index];
                *entry.hit_count.get_or_insert(0) += 1;
                if entry.hits.len() < 32
                    && (entry.hits.is_empty() || retained < 512)
                    && let (Some(line), Some(bytes)) =
                        (data["line_number"].as_u64(), rg_bytes(&data["lines"]))
                {
                    let snippet: String = String::from_utf8_lossy(&bytes)
                        .trim()
                        .chars()
                        .take(240)
                        .collect();
                    if entry.hits.is_empty() {
                        entry.line = Some(line);
                        entry.snippet = Some(snippet.clone());
                    }
                    entry.hits.push(Hit { line, snippet });
                    retained += 1;
                }
                hits += 1;
                hits < 10_000
            })?;
        if limited || !matches!(code, Some(0 | 1)) {
            result.partial = true;
            if invalid || (!limited && code == Some(2)) {
                result.diagnostic=Some("Ripgrep could not complete the search. Check the expression and folder permissions.".into());
            }
            break;
        }
    }
    result.extra["hitCount"] = json!(
        result
            .entries
            .iter()
            .filter_map(|e| e.hit_count)
            .sum::<usize>()
    );
    result.extra["matchQuery"] = json!(pattern);
    Ok(())
}
