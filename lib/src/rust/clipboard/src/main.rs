mod content;
mod history;
mod policy;
mod service;
mod wayland;

use crate::content::TEXT_LIMIT;
use crate::history::History;
use crate::service::{Error, Request, Service, reply};
use rustix::{
    fs::{FlockOperation, Mode, OFlags, flock, open},
    net::sockopt::socket_peercred,
    process::{DumpableBehavior, geteuid, set_dumpable_behavior, umask},
};
use std::{
    env, fs,
    io::{self, BufWriter, Read},
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_WORKERS: usize = 16;
const MAX_REQUEST: usize = TEXT_LIMIT * 6 + 4096;

fn main() -> Result<(), Error> {
    umask(Mode::from_bits_retain(0o077));
    set_dumpable_behavior(DumpableBehavior::NotDumpable)?;
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--serve-selection") => {
            let linked = match args.next().as_deref() {
                Some("--linked") => true,
                None => false,
                _ => return Err("Unexpected selection helper argument".into()),
            };
            wayland::serve_selection(linked)
                .map_err(|_| "Could not serve clipboard selection".into())
        }
        None => run(),
        Some(_) => Err("Unexpected clipboard service argument".into()),
    }
}

fn run() -> Result<(), Error> {
    let runtime = absolute_env("XDG_RUNTIME_DIR")
        .ok_or("An absolute XDG_RUNTIME_DIR is required")?
        .join("remya-launcher");
    let state = absolute_env("XDG_STATE_HOME")
        .or_else(|| absolute_env("HOME").map(|home| home.join(".local/state")))
        .ok_or("An absolute XDG_STATE_HOME or HOME is required")?
        .join("remya/launcher");
    prepare_directory(&runtime)?;
    prepare_directory(&state)?;
    let _runtime_lock = lock_directory(&runtime)?;
    let _state_lock = lock_directory(&state)?;

    let socket = runtime.join("clipboard.sock");
    match fs::symlink_metadata(&socket) {
        Ok(metadata)
            if metadata.file_type().is_socket() && metadata.uid() == geteuid().as_raw() =>
        {
            fs::remove_file(&socket)?;
        }
        Ok(_) => return Err("Clipboard socket path is already occupied".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let db = state.join("clipboard.sqlite3");
    match fs::symlink_metadata(&db) {
        Ok(metadata) if metadata.is_file() && metadata.uid() == geteuid().as_raw() => {
            if metadata.mode() & 0o077 != 0 {
                fs::set_permissions(&db, fs::Permissions::from_mode(0o600))?;
            }
        }
        Ok(_) => return Err("Clipboard state path is already occupied".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let service = Arc::new(Service::new(History::open(&db)?));
    let listener = UnixListener::bind(socket)?;
    let policy = Arc::clone(&service);
    std::thread::Builder::new()
        .name("clipboard-policy".into())
        .spawn(move || policy::run(&policy))?;
    let capture = Arc::clone(&service);
    std::thread::Builder::new()
        .name("clipboard-capture".into())
        .spawn(move || capture.watch_wayland())?;
    let expiry = Arc::clone(&service);
    std::thread::Builder::new()
        .name("clipboard-expiry".into())
        .spawn(move || expiry.reap_expired())?;

    let workers = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let stream = stream?;
        if !socket_peercred(&stream).is_ok_and(|peer| peer.uid == geteuid()) {
            continue;
        }
        if workers.fetch_add(1, Ordering::Relaxed) >= MAX_WORKERS {
            workers.fetch_sub(1, Ordering::Relaxed);
            continue;
        }
        let service = Arc::clone(&service);
        let workers = Arc::clone(&workers);
        std::thread::spawn(move || {
            let _ = handle(&service, stream);
            workers.fetch_sub(1, Ordering::Relaxed);
        });
    }
    Ok(())
}

fn absolute_env(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

fn prepare_directory(path: &Path) -> Result<(), Error> {
    fs::create_dir_all(path.parent().expect("absolute application directory"))?;
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != geteuid().as_raw() {
        return Err("Clipboard directory is not privately owned".into());
    }
    if metadata.mode() & 0o077 != 0 {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn lock_directory(path: &Path) -> Result<rustix::fd::OwnedFd, Error> {
    let lock = open(
        path.join("clipboard.lock"),
        OFlags::CREATE | OFlags::WRONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::from_bits_retain(0o600),
    )?;
    flock(&lock, FlockOperation::NonBlockingLockExclusive)
        .map_err(|_| "The clipboard service already holds this directory")?;
    Ok(lock)
}

fn handle(service: &Service, mut stream: UnixStream) -> io::Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    match read_request(&mut stream) {
        Ok(Request::Watch) => match service.watch_updates(stream.try_clone()?) {
            Ok(()) => Ok(()),
            Err(error) => reply::<()>(stream, Err(error)),
        },
        Ok(Request::Reveal { id }) => {
            let revealed = service.reveal(&id);
            reply(stream, Ok(revealed.as_deref().map(String::as_str)))
        }
        Ok(Request::Image { id }) => reply(BufWriter::new(stream), service.image(&id)),
        Ok(request) => reply(BufWriter::new(stream), service.execute(request)),
        Err(error) => reply::<()>(stream, Err(error)),
    }
}

fn read_request(stream: &mut UnixStream) -> Result<Request, Error> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut input = Vec::new();
    let mut chunk = [0; 8192];
    loop {
        if !wayland::wait_readable(stream, deadline)? {
            return Err("Clipboard request timed out".into());
        }
        let length = stream.read(&mut chunk)?;
        if length == 0 {
            return Err("Clipboard request requires one JSON line".into());
        }
        let line = &chunk[..length];
        let end = line.iter().position(|byte| *byte == b'\n');
        input.extend_from_slice(&line[..end.unwrap_or(length)]);
        if input.len() > MAX_REQUEST {
            return Err("Clipboard request is too large".into());
        }
        if end.is_some() {
            return serde_json::from_slice(&input).map_err(|_| "Invalid clipboard request".into());
        }
    }
}
