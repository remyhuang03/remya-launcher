use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rustix::fs::{FlockOperation, Mode, OFlags, flock, open};
use rustix::net::sockopt::socket_peercred;
use rustix::process::geteuid;
use serde_json::{Value, json};

use crate::protocol::serialize_result;
use crate::service::AppsService;
use crate::store::{absolute_env, ensure_private_dir};

const MAX_WORKERS: usize = 16;
const MAX_REQUEST_BYTES: usize = 256 * 1024;
const MAX_REPLY_BYTES: usize = 8 * 1024 * 1024;
const ENV_HELLO_PREFIX: &str = "{\"__remya_hello\":";

pub fn run_daemon() -> Result<(), String> {
    let runtime = runtime_dir()?;
    ensure_private_dir(&runtime).map_err(|e| format!("Failed to prepare runtime dir: {e}"))?;
    let _lock = lock_runtime(&runtime)?;

    let socket_path = runtime.join("apps.sock");
    match fs::symlink_metadata(&socket_path) {
        Ok(meta) if meta.file_type().is_socket() && meta.uid() == geteuid().as_raw() => {
            fs::remove_file(&socket_path)
                .map_err(|e| format!("Failed to remove stale socket: {e}"))?;
        }
        Ok(_) => return Err("Apps socket path is already occupied.".into()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("Failed to inspect apps socket path: {e}")),
    }

    let listener =
        UnixListener::bind(&socket_path).map_err(|e| format!("Failed to bind apps socket: {e}"))?;

    let service = Arc::new(AppsService::new(true)?);
    service.warm_background();

    let expected_env = env_fingerprint();
    let workers = Arc::new(AtomicUsize::new(0));

    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        if !socket_peercred(&stream).is_ok_and(|peer| peer.uid == geteuid()) {
            continue;
        }
        if workers.fetch_add(1, Ordering::Relaxed) >= MAX_WORKERS {
            workers.fetch_sub(1, Ordering::Relaxed);
            continue;
        }
        let svc = Arc::clone(&service);
        let workers_ref = Arc::clone(&workers);
        let env_fp = expected_env.clone();
        std::thread::spawn(move || {
            let _ = handle_client(&svc, stream, &env_fp);
            workers_ref.fetch_sub(1, Ordering::Relaxed);
        });
    }
    Ok(())
}

pub fn try_connect_proxy() -> Option<UnixStream> {
    let socket_path = runtime_dir().ok()?.join("apps.sock");
    let meta = fs::symlink_metadata(&socket_path).ok()?;
    if !meta.file_type().is_socket() || meta.uid() != geteuid().as_raw() {
        return None;
    }

    let mut stream = UnixStream::connect(&socket_path).ok()?;
    if !socket_peercred(&stream).is_ok_and(|peer| peer.uid == geteuid()) {
        return None;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));

    let hello = json!({ "__remya_hello": env_fingerprint() });
    if writeln!(stream, "{hello}").is_err() || stream.flush().is_err() {
        return None;
    }

    let mut sock_reader = BufReader::new(stream.try_clone().ok()?);
    let mut ack = String::new();
    if sock_reader.read_line(&mut ack).ok()? == 0 {
        return None;
    }
    let ack_val: Value = serde_json::from_str(ack.trim()).ok()?;
    if ack_val.get("__remya_ok").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let _ = stream.set_read_timeout(None);
    Some(stream)
}

pub fn run_proxy(
    mut stream: UnixStream,
    input: impl Read,
    mut output: impl Write,
) -> io::Result<()> {
    let mut sock_reader = BufReader::new(stream.try_clone()?);
    let mut stdin_reader = BufReader::new(input);
    let mut line = String::new();
    loop {
        line.clear();
        if read_bounded_line(&mut stdin_reader, &mut line, MAX_REQUEST_BYTES)? == 0 {
            return Ok(());
        }
        let req_id = serde_json::from_str::<Value>(line.trim())
            .ok()
            .and_then(|v| v.get("id").cloned())
            .unwrap_or(Value::Null);

        if writeln!(stream, "{}", line.trim_end_matches(['\r', '\n'])).is_err()
            || stream.flush().is_err()
        {
            // Do not replay mutating requests after transport failure.
            let err_reply = serialize_result::<Value>(
                req_id,
                Err("Apps daemon transport disconnected.".into()),
            );
            let _ = writeln!(output, "{err_reply}");
            let _ = output.flush();
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "apps daemon transport write failed",
            ));
        }

        let mut reply = String::new();
        match read_bounded_line(&mut sock_reader, &mut reply, MAX_REPLY_BYTES) {
            Ok(0) | Err(_) => {
                // Do not replay mutating requests after transport failure.
                let err_reply = serialize_result::<Value>(
                    req_id,
                    Err("Apps daemon disconnected before replying.".into()),
                );
                let _ = writeln!(output, "{err_reply}");
                let _ = output.flush();
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "apps daemon disconnected",
                ));
            }
            Ok(_) => {
                writeln!(output, "{}", reply.trim_end_matches(['\r', '\n']))?;
                output.flush()?;
            }
        }
    }
}

pub fn serve_standalone(input: impl Read, mut output: impl Write) -> io::Result<()> {
    let service = Arc::new(AppsService::new(true).map_err(io::Error::other)?);
    service.warm_background();
    let mut reader = BufReader::new(input);
    let mut line = String::new();

    loop {
        line.clear();
        let n = read_bounded_line(&mut reader, &mut line, MAX_REQUEST_BYTES)?;
        if n == 0 {
            return Ok(());
        }
        let reply = service.handle_line(&line);
        writeln!(output, "{reply}")?;
        output.flush()?;
    }
}

fn handle_client(
    service: &AppsService,
    mut stream: UnixStream,
    expected_env: &str,
) -> io::Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    let mut first = true;

    loop {
        line.clear();
        let n = read_bounded_line(&mut reader, &mut line, MAX_REQUEST_BYTES)?;
        if n == 0 {
            return Ok(());
        }
        let trimmed = line.trim();
        if first {
            first = false;
            if trimmed.starts_with(ENV_HELLO_PREFIX) {
                let matched = serde_json::from_str::<Value>(trimmed)
                    .ok()
                    .and_then(|v| {
                        v.get("__remya_hello")
                            .and_then(Value::as_str)
                            .map(|fp| fp == expected_env)
                    })
                    .unwrap_or(false);
                writeln!(stream, "{}", json!({ "__remya_ok": matched }))?;
                stream.flush()?;
                if !matched {
                    return Ok(());
                }
                continue;
            }
        }
        let reply = service.handle_line(trimmed);
        writeln!(stream, "{reply}")?;
        stream.flush()?;
    }
}

fn read_bounded_line(
    reader: &mut impl BufRead,
    out: &mut String,
    max_bytes: usize,
) -> io::Result<usize> {
    out.clear();
    let mut buf = Vec::new();
    let mut total = 0usize;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            if buf.is_empty() {
                return Ok(0);
            }
            *out = String::from_utf8_lossy(&buf).into_owned();
            return Ok(total);
        }
        let newline = chunk.iter().position(|&b| b == b'\n');
        let take = newline.map_or(chunk.len(), |idx| idx + 1);
        total += take;
        if buf.len() + take > max_bytes {
            reader.consume(take);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "NDJSON line exceeds frame limit",
            ));
        }
        buf.extend_from_slice(&chunk[..take]);
        reader.consume(take);
        if newline.is_some() {
            *out = String::from_utf8_lossy(&buf).into_owned();
            return Ok(total);
        }
    }
}

fn runtime_dir() -> Result<PathBuf, String> {
    absolute_env("XDG_RUNTIME_DIR")
        .ok_or_else(|| "An absolute XDG_RUNTIME_DIR is required.".to_owned())
        .map(|p| p.join("remya-launcher"))
}

fn lock_runtime(runtime: &std::path::Path) -> Result<rustix::fd::OwnedFd, String> {
    let lock = open(
        runtime.join("apps.lock"),
        OFlags::CREATE | OFlags::WRONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::from_bits_retain(0o600),
    )
    .map_err(|e| format!("Failed to open apps lockfile: {e}"))?;
    flock(&lock, FlockOperation::NonBlockingLockExclusive)
        .map_err(|_| "Another apps daemon instance is already running.".to_owned())?;
    Ok(lock)
}

fn env_fingerprint() -> String {
    const KEYS: &[&str] = &[
        "XDG_DATA_HOME",
        "XDG_DATA_DIRS",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_CACHE_HOME",
        "XDG_CURRENT_DESKTOP",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "WAYLAND_DISPLAY",
        "DISPLAY",
        "LANG",
        "LC_ALL",
        "LC_MESSAGES",
        "REMYA_APPS_PACMAN_SYNC_DIR",
        "REMYA_APPS_PACMAN_LOCAL_DIR",
    ];
    let mut parts = Vec::with_capacity(KEYS.len());
    for key in KEYS {
        let val = env::var(key).unwrap_or_default();
        parts.push(format!("{key}={val}"));
    }
    parts.join("\0")
}
