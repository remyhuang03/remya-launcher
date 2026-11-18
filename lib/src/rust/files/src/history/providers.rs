//! Both CLIs may maintain their databases even during list/query.
use super::Kind;
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, LazyLock, Mutex},
    thread,
    time::{Duration, Instant},
};

const RECORD_LIMIT: usize = 20_000;
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
type Rows = Vec<(PathBuf, f64, Option<i64>)>;
type ReadResult = Result<Arc<Rows>, &'static str>;
type Cache = HashMap<(Kind, String), (Instant, ReadResult)>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);

pub(super) fn invalidate() {
    CACHE.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

pub(super) fn selected(cfg: &Value, kind: Kind) -> &str {
    match kind {
        Kind::File => match cfg["file_provider"].as_str() {
            Some("xbel") => "xbel",
            Some("memy") => "memy",
            _ => "invalid",
        },
        Kind::Directory => match cfg["directory_provider"].as_str() {
            Some("memy") => "memy",
            Some("xbel") => "xbel",
            Some("zoxide") => "zoxide",
            _ => "invalid",
        },
    }
}

fn run(command: &mut Command, timeout: Duration, limit: usize) -> Result<Vec<u8>, &'static str> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                "not_installed"
            } else {
                "spawn_failed"
            }
        })?;
    let started = Instant::now();
    let result = (|| {
        let mut pipe = child.stdout.take().ok_or("provider_error")?;
        let flags = fcntl_getfl(&pipe).map_err(|_| "provider_error")?;
        fcntl_setfl(&pipe, flags | OFlags::NONBLOCK).map_err(|_| "provider_error")?;
        let mut bytes = Vec::new();
        let mut buf = [0u8; 16384];
        loop {
            if started.elapsed() >= timeout {
                return Err("timeout");
            }
            match pipe.read(&mut buf) {
                Ok(0) => {
                    if let Some(exit) = child.try_wait().map_err(|_| "provider_error")? {
                        return exit.success().then_some(bytes).ok_or("provider_error");
                    }
                }
                Ok(n) => {
                    if bytes.len() + n > limit {
                        return Err("output_too_large");
                    }
                    bytes.extend_from_slice(&buf[..n]);
                    continue;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => return Err("provider_error"),
            }
            thread::sleep(Duration::from_millis(2));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result
}

pub fn probe(provider: &str) -> Result<(), &'static str> {
    if !matches!(provider, "memy" | "zoxide") {
        return Err("configuration_error");
    }
    let out = run(
        Command::new(provider).arg("--version"),
        Duration::from_millis(500),
        1024,
    )?;
    let line = std::str::from_utf8(&out)
        .map_err(|_| "unsupported_version")?
        .trim();
    let supported = match provider {
        "memy" => matches!(line, "memy v1.0.0" | "memy 1.0.0"),
        "zoxide" => line == "zoxide 0.10.0",
        _ => false,
    };
    if supported {
        Ok(())
    } else {
        Err("unsupported_version")
    }
}

fn command(provider: &str) -> Command {
    let mut cmd = Command::new(provider);
    if provider == "memy" {
        cmd.args(["--color", "never", "--config", "import_on_first_use=false"]);
    }
    cmd
}

pub(super) fn cached_health(kind: Kind, cfg: &Value) -> Option<Result<(), &'static str>> {
    let cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let (seen, result) = cache.get(&(kind, cfg.to_string()))?;
    (seen.elapsed() < Duration::from_secs(5)).then(|| result.as_ref().map(|_| ()).map_err(|&e| e))
}

pub(super) fn rows(kind: Kind, cfg: &Value, refresh: bool) -> ReadResult {
    let key = (kind, cfg.to_string());
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    cache.retain(|_, (seen, _)| seen.elapsed() < Duration::from_secs(5));
    if !refresh && let Some((_, result)) = cache.get(&key) {
        return result.clone();
    }
    let result = load(kind, cfg).map(Arc::new);
    cache.insert(key, (Instant::now(), result.clone()));
    result
}

fn load(kind: Kind, cfg: &Value) -> Result<Rows, &'static str> {
    let provider = selected(cfg, kind);
    probe(provider)?;
    let mut cmd = command(provider);
    if provider == "memy" {
        cmd.args([
            "--config",
            "denied_files_on_list=skip-silently",
            "--config",
            "missing_files_delete_from_db_after=-1",
            "list",
            if kind == Kind::File {
                "--files-only"
            } else {
                "--directories-only"
            },
            "--format",
            "json",
            "--sort",
            "descending",
            "--limit-results",
            "20000",
        ]);
    } else {
        cmd.args(["query", "--all", "--list", "--score"]);
    }
    let bytes = run(&mut cmd, Duration::from_secs(2), OUTPUT_LIMIT)?;
    let mut rows = if provider == "memy" {
        parse_memy(&bytes)?
    } else {
        parse_zoxide(&bytes)?
    };
    rows.sort_by(|a, b| a.1.total_cmp(&b.1));
    let n = rows.len();
    let mut start = 0;
    while start < n {
        let mut end = start + 1;
        while end < n && rows[end].1 == rows[start].1 {
            end += 1;
        }
        let rank = if n == 1 {
            0.5
        } else {
            (start + end - 1) as f64 / (2.0 * (n - 1) as f64)
        };
        for row in &mut rows[start..end] {
            row.1 = rank;
        }
        start = end;
    }
    Ok(rows)
}

fn parse_zoxide(bytes: &[u8]) -> Result<Rows, &'static str> {
    std::str::from_utf8(bytes)
        .map_err(|_| "invalid_output")?
        .lines()
        .take(RECORD_LIMIT + 1)
        .map(|line| {
            let (score, path) = line
                .trim_start_matches(' ')
                .split_once(' ')
                .ok_or("invalid_output")?;
            let score: f64 = score.parse().map_err(|_| "invalid_output")?;
            if !score.is_finite() || !Path::new(path).is_absolute() || path.contains(['\r', '\0']) {
                return Err("invalid_output");
            }
            Ok((PathBuf::from(path), score, None))
        })
        .collect::<Result<Rows, _>>()
        .and_then(bounded)
}

fn parse_memy(bytes: &[u8]) -> Result<Rows, &'static str> {
    let values: Vec<Value> = serde_json::from_slice(bytes).map_err(|_| "invalid_output")?;
    if values.len() > RECORD_LIMIT {
        return Err("output_too_large");
    }
    values
        .iter()
        .map(|v| {
            let path = v["path"]
                .as_str()
                .filter(|s| Path::new(s).is_absolute() && !s.contains('\0'))
                .ok_or("invalid_output")?;
            let score = v["frecency"]
                .as_f64()
                .filter(|f| f.is_finite())
                .ok_or("invalid_output")?;
            let date = v["last_noted"]
                .as_str()
                .and_then(|s| gio::glib::DateTime::from_iso8601(s, None).ok())
                .map(|d| d.to_unix());
            Ok((PathBuf::from(path), score, date))
        })
        .collect()
}

fn bounded(rows: Rows) -> Result<Rows, &'static str> {
    if rows.len() > RECORD_LIMIT {
        Err("output_too_large")
    } else {
        Ok(rows)
    }
}

pub(super) fn note(kind: Kind, cfg: &Value, path: &Path, editor: bool) -> &'static str {
    let provider = selected(cfg, kind);
    let mode = cfg["external_write_mode"].as_str();
    if cfg["external_enabled"] != true
        || provider == "xbel"
        || !(mode == Some("launcher") || (mode == Some("delegate_editor") && !editor))
    {
        return "skipped";
    }
    if path
        .to_str()
        .is_none_or(|s| provider == "zoxide" && s.contains(['\r', '\n']))
    {
        return "unsupported_path";
    }
    if let Err(reason) = probe(provider) {
        return reason;
    }
    let mut cmd = command(provider);
    cmd.args([if provider == "memy" { "note" } else { "add" }, "--"])
        .arg(path);
    match run(&mut cmd, Duration::from_secs(1), 4096) {
        Ok(_) => "accepted",
        Err("not_installed" | "spawn_failed") => "unavailable",
        Err(_) => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_cli_formats_preserve_paths_and_provider_time() {
        let z = parse_zoxide(b"   4.0 /tmp/a folder \n   2.0 /tmp/a\tb\n").unwrap();
        assert_eq!(z[0], (PathBuf::from("/tmp/a folder "), 4., None));
        assert_eq!(z[1].0, Path::new("/tmp/a\tb"));
        let m=parse_memy(br#"[{"path":"/tmp/a file","frecency":0.5,"count":1,"last_noted":"2026-11-18T14:53:09+08:00","file_type":"file"}]"#).unwrap();
        assert!(m[0].2.is_some());
        assert!(parse_zoxide(b"   NaN /tmp/foo\n").is_err());
        let invalid = serde_json::json!({"file_provider": "zoxide"});
        assert_eq!(
            probe(selected(&invalid, Kind::File)),
            Err("configuration_error")
        );
    }
}
