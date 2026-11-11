use crate::content::{
    Format, GNOME_FILES, HTML, IMAGE_TYPES, KDE_CUT, OFFER_LIMIT, PLAIN, PLAIN_TYPES, SECRET_LIMIT,
    TEXT_LIMIT, URI_LIST,
};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::fd::AsFd;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use wl_clipboard_rs::{
    copy::{MimeSource, MimeType, Options, Source},
    watch::Offer,
};
use zeroize::Zeroizing;

pub const SECRET_MIME: &str = "x-kde-passwordManagerHint";
const OFFER_TIMEOUT: Duration = Duration::from_millis(1800);
const MAX_MIME_LENGTH: usize = 128;

pub struct Capture {
    pub formats: Vec<Format>,
    pub sensitive: bool,
}

pub fn capture(offer: &mut Offer<'_>, mime_types: &[String]) -> Capture {
    let deadline = Instant::now() + OFFER_TIMEOUT;
    let offered = |mime: &str| {
        mime_types
            .iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(mime))
    };
    let sensitive = offered(SECRET_MIME).is_some_and(|advertised| {
        read_format(
            offer,
            advertised,
            32,
            deadline.min(Instant::now() + Duration::from_millis(250)),
        )
        .is_none_or(|hint| hint.data != b"public")
    });
    let mut remaining = if sensitive { SECRET_LIMIT } else { OFFER_LIMIT };
    let mut formats = Vec::new();
    let mut take_format = |mime: &str, limit: usize| {
        let Some(advertised) = offered(mime) else {
            return;
        };
        if remaining == 0 {
            return;
        }
        let Some(mut format) = read_format(offer, advertised, limit.min(remaining), deadline)
        else {
            return;
        };
        remaining -= format.data.len();
        format.mime = mime.into();
        formats.push(format);
    };
    if !sensitive {
        for mime in IMAGE_TYPES {
            take_format(mime, OFFER_LIMIT);
        }
        for mime in [URI_LIST, GNOME_FILES, KDE_CUT, HTML] {
            take_format(mime, TEXT_LIMIT);
        }
    }
    if remaining != 0
        && let Some(mut format) = PLAIN_TYPES.iter().find_map(|mime| {
            let advertised = offered(mime)?;
            let format = read_format(offer, advertised, TEXT_LIMIT.min(remaining), deadline)?;
            (!format.data.is_empty() && std::str::from_utf8(&format.data).is_ok()).then_some(format)
        })
    {
        // Normalize equivalent plain text aliases without storing duplicate payloads.
        format.mime = PLAIN.into();
        formats.push(format);
    }
    formats.retain(|format| !format.data.is_empty());
    Capture { formats, sensitive }
}

fn read_format(
    offer: &mut Offer<'_>,
    mime: &str,
    limit: usize,
    deadline: Instant,
) -> Option<Format> {
    if Instant::now() >= deadline {
        return None;
    }
    let mut pipe = offer.receive(mime).ok()?;
    let mut format = Format {
        mime: mime.into(),
        data: Vec::new(),
    };
    let mut chunk = Zeroizing::new([0; 16 * 1024]);
    loop {
        if !wait_readable(&pipe, deadline).ok()? {
            return None;
        }
        let length = pipe.read(&mut *chunk).ok()?;
        if length == 0 {
            return Some(format);
        }
        if length > limit - format.data.len() {
            return None;
        }
        format.data.extend_from_slice(&chunk[..length]);
    }
}

pub fn wait_readable(fd: &impl AsFd, deadline: Instant) -> io::Result<bool> {
    let mut fds = [PollFd::new(fd, PollFlags::IN)];
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(false);
        };
        let timeout = Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: i64::from(remaining.subsec_nanos()),
        };
        match poll(&mut fds, Some(&timeout)) {
            Ok(ready) => return Ok(ready > 0),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Frame {
    formats: Vec<(String, usize)>,
    sensitive: bool,
}

pub struct Selection {
    child: Child,
}

impl Selection {
    pub fn start(formats: &[Format], marker: &str, sensitive: bool) -> io::Result<Self> {
        let mut selection = Self {
            child: Command::new(std::env::current_exe()?)
                .args(["--serve-selection", "--linked"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        };
        let pipe = selection.child.stdin.as_mut().expect("piped child stdin");
        serde_json::to_writer(
            &mut *pipe,
            &Frame {
                formats: formats
                    .iter()
                    .map(|format| (format.mime.clone(), format.data.len()))
                    .chain([(marker.into(), 1)])
                    .collect(),
                sensitive,
            },
        )?;
        pipe.write_all(b"\n")?;
        for format in formats {
            pipe.write_all(&format.data)?;
        }
        pipe.write_all(b"1")?;
        Ok(selection)
    }
}

impl Drop for Selection {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn serve_selection(linked: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut input = BufReader::new(io::stdin());
    let mut header = String::new();
    (&mut input).take(4096).read_line(&mut header)?;
    let Frame { formats, sensitive } = serde_json::from_str(&header)?;
    let mut remaining = OFFER_LIMIT + 1;
    if formats.is_empty() || formats.len() > 16 {
        return Err("Invalid selection formats".into());
    }
    let mut sources = Vec::with_capacity(formats.len());
    for (mime, length) in formats {
        if mime.is_empty()
            || mime.len() > MAX_MIME_LENGTH
            || mime.chars().any(char::is_control)
            || length > remaining
            || sensitive && length > SECRET_LIMIT
        {
            return Err("Invalid selection format".into());
        }
        remaining -= length;
        let mut data = vec![0; length];
        input.read_exact(&mut data)?;
        sources.push(MimeSource {
            source: Source::Bytes(data.into_boxed_slice()),
            mime_type: MimeType::Specific(mime),
        });
    }
    if linked {
        std::thread::spawn(move || {
            // The owner keeps the pipe open as a process lifetime signal.
            let _ = input.read(&mut [0]);
            std::process::exit(0);
        });
    }
    // Add plain aliases only from real plain text. HTML and URI-list are not interchangeable.
    let mut options = Options::new();
    options
        .foreground(true)
        .sensitive(sensitive)
        .omit_additional_text_mime_types(
            !sources.iter().any(
                |source| matches!(&source.mime_type, MimeType::Specific(mime) if mime == PLAIN),
            ),
        );
    if let Some(index) = sources
        .iter()
        .position(|source| matches!(&source.mime_type, MimeType::Specific(mime) if mime == PLAIN))
    {
        sources.swap(0, index);
    }
    options.prepare_copy_multi(sources)?.serve()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn pipe_wait_honors_deadline_and_eof() {
        let (read, mut write) = UnixStream::pair().unwrap();
        assert!(!wait_readable(&read, Instant::now() + Duration::from_millis(2)).unwrap());
        write.write_all(b"x").unwrap();
        assert!(wait_readable(&read, Instant::now() + Duration::from_millis(50)).unwrap());
        let mut read = read;
        read.read_exact(&mut [0]).unwrap();
        drop(write);
        assert!(wait_readable(&read, Instant::now() + Duration::from_millis(50)).unwrap());
    }
}
