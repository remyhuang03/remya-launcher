use std::env;
use std::fs::File;
use std::io;
use std::os::fd::{FromRawFd, IntoRawFd};

use rustix::fs::{Mode, OFlags, open};
use rustix::io::fcntl_dupfd_cloexec;
use rustix::process::{DumpableBehavior, set_dumpable_behavior, umask};
use rustix::stdio::{dup2_stdin, dup2_stdout};

use remya_launcher_apps::daemon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    umask(Mode::from_bits_retain(0o077));
    let _ = set_dumpable_behavior(DumpableBehavior::NotDumpable);

    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("--daemon") => {
            if args.next().is_some() {
                return Err("Unexpected argument for --daemon".into());
            }
            redirect_stdio_to_null()?;
            daemon::run_daemon().map_err(Into::into)
        }
        Some("--standalone") => {
            if args.next().is_some() {
                return Err("Unexpected argument for --standalone".into());
            }
            let (stdin, stdout) = isolate_ndjson_stdio()?;
            daemon::serve_standalone(stdin, stdout).map_err(Into::into)
        }
        None => {
            let (stdin, stdout) = isolate_ndjson_stdio()?;
            if let Some(stream) = daemon::try_connect_proxy() {
                return daemon::run_proxy(stream, stdin, stdout).map_err(Into::into);
            }
            daemon::serve_standalone(stdin, stdout).map_err(Into::into)
        }
        Some(other) => Err(format!("Unexpected argument: {other}").into()),
    }
}

fn isolate_ndjson_stdio() -> io::Result<(File, File)> {
    // Move NDJSON pipe fds onto private CLOEXEC descriptors and point fd 0/1 to /dev/null
    // so launched child processes or GLib diagnostics never pollute the NDJSON stream.
    let in_fd = fcntl_dupfd_cloexec(io::stdin(), 3)?;
    let out_fd = fcntl_dupfd_cloexec(io::stdout(), 3)?;
    redirect_stdio_to_null()?;
    let in_file = unsafe { File::from_raw_fd(in_fd.into_raw_fd()) };
    let out_file = unsafe { File::from_raw_fd(out_fd.into_raw_fd()) };
    Ok((in_file, out_file))
}

fn redirect_stdio_to_null() -> io::Result<()> {
    let devnull = open("/dev/null", OFlags::RDWR | OFlags::CLOEXEC, Mode::empty())
        .map_err(|e| io::Error::from_raw_os_error(e.raw_os_error()))?;
    dup2_stdin(&devnull).map_err(|e| io::Error::from_raw_os_error(e.raw_os_error()))?;
    dup2_stdout(&devnull).map_err(|e| io::Error::from_raw_os_error(e.raw_os_error()))?;
    Ok(())
}
