use crate::content::{self, Entry, Format, Kind, Summary, TEXT_LIMIT, classify};
use crate::history::{History, Preferences};
use crate::wayland::{self, Selection};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{self, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use wl_clipboard_rs::{
    paste::Seat,
    watch::{ClipboardEvent, ClipboardType, Watcher},
};
use zeroize::Zeroizing;

const SECRET_TTL: Duration = Duration::from_secs(45);
const MAX_SECRETS: usize = 8;
const PUBLISH_TIMEOUT: Duration = Duration::from_secs(3);
const WATCH_UPDATE: &[u8] = b"{\"ok\":true,\"value\":null}\n";

pub type Error = Box<dyn std::error::Error + Send + Sync>;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Request {
    List {
        #[serde(default)]
        query: String,
    },
    Detail {
        id: String,
    },
    Thumbnail {
        id: String,
    },
    Text {
        id: String,
    },
    Image {
        id: String,
    },
    Reveal {
        id: String,
    },
    SetPinned {
        id: String,
        pinned: bool,
    },
    Delete {
        id: String,
    },
    Clear,
    Publish {
        id: Option<String>,
        mode: Option<PasteMode>,
        text: Option<String>,
    },
    Watch,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PasteMode {
    Original,
    Plain,
    Paths,
}

struct Secret {
    id: String,
    copied_at: i64,
    expires: Instant,
    formats: Vec<Format>,
}

impl Secret {
    fn entry(&self) -> Entry {
        Entry {
            id: self.id.clone(),
            description: Summary {
                kind: Kind::Hidden,
                title: "Hidden clipboard item".into(),
                summary: "Sensitive · stored in memory".into(),
                mime_types: vec![],
                thumbnail_only: false,
                color: None,
            },
            copied_at: self.copied_at,
            times_copied: 1,
            pinned: false,
            expires_at: Some(self.copied_at + SECRET_TTL.as_millis() as i64),
        }
    }
}

struct PublishTarget {
    marker: String,
    secret: Option<String>,
}

struct Publishing {
    target: PublishTarget,
    observed: bool,
}

struct OwnedSelection {
    target: PublishTarget,
    _process: Selection,
}

struct State {
    history: History,
    secrets: Vec<Secret>,
    watchers: Vec<UnixStream>,
    generation: u64,
    serial: u64,
    available: bool,
    publishing: Option<Publishing>,
    owned: Option<OwnedSelection>,
}

impl State {
    fn notify(&mut self) {
        self.watchers.retain_mut(|watcher| {
            watcher
                .write(WATCH_UPDATE)
                .is_ok_and(|len| len == WATCH_UPDATE.len())
        });
    }

    fn next_number(&mut self) -> u64 {
        self.serial = self.serial.wrapping_add(1);
        self.serial
    }

    fn active_secret(&self, id: &str) -> Option<&Secret> {
        self.secrets
            .iter()
            .find(|secret| secret.id == id && secret.expires > Instant::now())
    }

    fn purge_secrets(&mut self, predicate: impl Fn(&Secret) -> bool) -> bool {
        let before = self.secrets.len();
        self.secrets.retain(|secret| !predicate(secret));
        if self.secrets.len() == before {
            return false;
        }
        let removed = |id: &str| self.secrets.iter().all(|secret| secret.id != id);
        if self
            .owned
            .as_ref()
            .and_then(|owned| owned.target.secret.as_deref())
            .is_some_and(removed)
        {
            self.owned = None;
        }
        if self
            .publishing
            .as_ref()
            .and_then(|pending| pending.target.secret.as_deref())
            .is_some_and(removed)
        {
            self.publishing = None;
        }
        true
    }
}

pub struct Service {
    state: Mutex<State>,
    changed: Condvar,
    publishing: Mutex<()>,
    marker_prefix: String,
}

impl Service {
    pub fn new(history: History) -> Self {
        Self {
            state: Mutex::new(State {
                history,
                secrets: Vec::new(),
                watchers: Vec::new(),
                generation: 0,
                serial: 0,
                available: false,
                publishing: None,
                owned: None,
            }),
            changed: Condvar::new(),
            publishing: Mutex::new(()),
            marker_prefix: format!(
                "application/x-remya-launcher-{}-{}-",
                std::process::id(),
                now_ms(),
            ),
        }
    }

    #[cfg(test)]
    pub fn policy_enabled(&self) -> bool {
        self.state.lock().unwrap().history.enabled()
    }

    pub fn apply_policy(&self, policy: Option<Preferences>) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        state.history.set_policy(policy, now_ms())?;
        state.generation = state.generation.wrapping_add(1);
        if !state.history.enabled() {
            state.purge_secrets(|_| true);
        }
        state.notify();
        self.changed.notify_all();
        Ok(())
    }

    pub fn watch_updates(&self, mut stream: UnixStream) -> Result<(), Error> {
        let mut state = self.state.lock().unwrap();
        state.watchers.retain(|peer| {
            let mut fds = [rustix::event::PollFd::new(
                peer,
                rustix::event::PollFlags::empty(),
            )];
            let timeout = rustix::event::Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            rustix::event::poll(&mut fds, Some(&timeout)).is_ok()
                && !fds[0]
                    .revents()
                    .intersects(rustix::event::PollFlags::HUP | rustix::event::PollFlags::ERR)
        });
        if state.watchers.len() >= 16 {
            return Err("Too many clipboard history subscribers".into());
        }
        stream.set_nonblocking(true)?;
        stream.write_all(WATCH_UPDATE)?;
        state.watchers.push(stream);
        Ok(())
    }

    pub fn watch_wayland(&self) {
        let mut last_failure = false;
        loop {
            let mut watcher = match Watcher::new(ClipboardType::Regular, Seat::Unspecified) {
                Ok(watcher) => watcher,
                Err(_) => {
                    if !last_failure {
                        eprintln!("Clipboard watcher is waiting for Wayland data control");
                    }
                    last_failure = true;
                    std::thread::sleep(Duration::from_secs(3));
                    continue;
                }
            };
            last_failure = false;
            self.state.lock().unwrap().available = true;
            let mut initial = true;
            while let Ok(Some(event)) = watcher.next_event() {
                match event {
                    ClipboardEvent::Changed {
                        mime_types,
                        mut offer,
                        ..
                    } => {
                        let Some(generation) = self.begin_capture(&mime_types, initial) else {
                            initial = false;
                            continue;
                        };
                        let copied_at = now_ms();
                        let copied = Instant::now();
                        let capture = wayland::capture(&mut offer, &mime_types);
                        self.store_capture(capture, generation, copied_at, copied);
                    }
                    ClipboardEvent::Cleared { .. } => {
                        self.begin_capture(&[], initial);
                    }
                }
                initial = false;
            }
            {
                let mut state = self.state.lock().unwrap();
                state.available = false;
                state.publishing = None;
                state.owned = None;
                self.changed.notify_all();
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn begin_capture(&self, mimes: &[String], initial: bool) -> Option<u64> {
        let mut state = self.state.lock().unwrap();
        let is_self = |marker: &str| mimes.iter().any(|mime| mime == marker);
        if let Some(pending) = &mut state.publishing {
            if is_self(&pending.target.marker) {
                pending.observed = true;
                self.changed.notify_all();
            } else if pending.observed {
                state.publishing = None;
                self.changed.notify_all();
            }
        }
        if state
            .owned
            .as_ref()
            .is_some_and(|owned| !is_self(&owned.target.marker))
        {
            state.owned = None;
        }
        let own_marker = mimes
            .iter()
            .any(|mime| mime.starts_with(&self.marker_prefix));
        (!own_marker && !initial && state.history.enabled()).then_some(state.generation)
    }

    fn store_capture(
        &self,
        mut capture: wayland::Capture,
        generation: u64,
        copied_at: i64,
        copied: Instant,
    ) {
        if capture.formats.is_empty() {
            return;
        }
        if capture.sensitive {
            let mut state = self.state.lock().unwrap();
            if generation != state.generation || !state.history.enabled() {
                return;
            }
            if state.secrets.len() >= MAX_SECRETS {
                // shortcut: keep only the eight most recent secrets to cap in-memory payloads.
                let oldest = state.secrets[0].id.clone();
                state.purge_secrets(|secret| secret.id == oldest);
            }
            let number = state.next_number();
            state.secrets.push(Secret {
                id: format!("hidden-{number}"),
                copied_at,
                expires: copied + SECRET_TTL,
                formats: capture.formats,
            });
            state.notify();
            self.changed.notify_all();
            return;
        }
        let id = content::content_id(&mut capture.formats);
        {
            let mut state = self.state.lock().unwrap();
            if generation != state.generation || !state.history.enabled() {
                return;
            }
            match state.history.refresh_existing(&id, copied_at) {
                Ok(true) => {
                    state.notify();
                    self.changed.notify_all();
                    return;
                }
                Ok(false) => {}
                Err(_) => {
                    eprintln!("Could not update the clipboard history database");
                    return;
                }
            }
        }
        let Some(content) = classify(id, capture.formats) else {
            return;
        };
        let mut state = self.state.lock().unwrap();
        if generation != state.generation || !state.history.enabled() {
            return;
        }
        if state.history.store(content, copied_at).is_ok() {
            state.notify();
            self.changed.notify_all();
        } else {
            eprintln!("Could not update the clipboard history database");
        }
    }

    pub fn reap_expired(&self) {
        let mut state = self.state.lock().unwrap();
        loop {
            let now = Instant::now();
            let wall_clock = now_ms();
            let secrets_changed = state.purge_secrets(|secret| secret.expires <= now);
            let mut history_due = state.history.next_expiry().ok().flatten();
            let mut history_changed = false;
            if history_due.is_some_and(|expires| expires <= wall_clock) {
                match state.history.prune(wall_clock) {
                    Ok(pruned) => {
                        history_changed = pruned;
                        history_due = state.history.next_expiry().ok().flatten();
                    }
                    Err(_) => {
                        eprintln!("Could not expire clipboard history");
                        history_due = Some(wall_clock + 60_000);
                    }
                }
            }
            if secrets_changed || history_changed {
                state.notify();
                self.changed.notify_all();
            }
            let history_wait =
                history_due.map(|at| Duration::from_millis(at.saturating_sub(wall_clock) as u64));
            let secret_wait = state
                .secrets
                .iter()
                .map(|secret| secret.expires.saturating_duration_since(now))
                .min();
            state = match history_wait.into_iter().chain(secret_wait).min() {
                Some(wait) => self.changed.wait_timeout(state, wait).unwrap().0,
                None => self.changed.wait(state).unwrap(),
            };
        }
    }

    pub fn execute(&self, request: Request) -> Result<Value, Error> {
        if let Request::Publish { id, mode, text } = request {
            self.publish(id, mode, text)?;
            return Ok(Value::Null);
        }

        let _publication = matches!(request, Request::Delete { .. } | Request::Clear)
            .then(|| {
                self.publishing
                    .try_lock()
                    .map_err(|_| "Clipboard publication is in progress")
            })
            .transpose()?;
        let mut state = self.state.lock().unwrap();
        match request {
            Request::List { query } => {
                let mut entries = state.history.entries(&query)?;
                if query.is_empty() {
                    entries.extend(
                        state
                            .secrets
                            .iter()
                            .filter(|secret| secret.expires > Instant::now())
                            .map(Secret::entry),
                    );
                    entries.sort_by_key(|entry| {
                        (
                            std::cmp::Reverse(entry.pinned),
                            std::cmp::Reverse(entry.copied_at),
                        )
                    });
                }
                Ok(serde_json::to_value(entries)?)
            }
            Request::Detail { id } => {
                if state.active_secret(&id).is_some() {
                    return Ok(json!({}));
                }
                let (detail, thumbnail) = state
                    .history
                    .detail(&id)?
                    .ok_or("Clipboard item is unavailable")?;
                let mut detail: Value = serde_json::from_str(&detail)?;
                if let Some(data) = thumbnail {
                    detail["thumbnail"] = json!(STANDARD.encode(data));
                }
                Ok(detail)
            }
            Request::Thumbnail { id } => Ok(json!(
                state
                    .history
                    .thumbnail(&id)?
                    .map(|data| STANDARD.encode(data))
            )),
            Request::Text { id } => Ok(json!(
                state
                    .history
                    .text(&id)?
                    .ok_or("Plain text is unavailable")?
            )),
            Request::SetPinned { id, pinned } => {
                if !state.history.set_pinned(&id, pinned, now_ms())? {
                    return Err("Clipboard item cannot be pinned".into());
                }
                state.notify();
                self.changed.notify_all();
                Ok(Value::Null)
            }
            Request::Delete { id } => {
                if !state.purge_secrets(|secret| secret.id == id) && !state.history.delete(&id)? {
                    return Err("Clipboard item is unavailable".into());
                }
                state.notify();
                self.changed.notify_all();
                Ok(Value::Null)
            }
            Request::Clear => {
                state.history.clear()?;
                state.generation = state.generation.wrapping_add(1);
                state.purge_secrets(|_| true);
                state.notify();
                self.changed.notify_all();
                Ok(Value::Null)
            }
            Request::Publish { .. }
            | Request::Watch
            | Request::Image { .. }
            | Request::Reveal { .. } => unreachable!(),
        }
    }

    pub fn reveal(&self, id: &str) -> Option<Zeroizing<String>> {
        self.state
            .lock()
            .unwrap()
            .active_secret(id)
            .and_then(|secret| content::plain(&secret.formats))
            .map(|text| Zeroizing::new(text.to_owned()))
    }

    pub fn image(&self, id: &str) -> Result<impl Serialize, Error> {
        #[derive(Serialize)]
        struct Image {
            mime: String,
            data: String,
        }
        let format = self
            .state
            .lock()
            .unwrap()
            .history
            .image(id)?
            .ok_or("Original image is unavailable")?;
        Ok(Image {
            mime: format.mime.clone(),
            data: STANDARD.encode(&format.data),
        })
    }

    fn publish(
        &self,
        id: Option<String>,
        mode: Option<PasteMode>,
        text: Option<String>,
    ) -> Result<(), Error> {
        let _one_at_a_time = self.publishing.lock().unwrap();
        let (formats, marker, expiry) = {
            let mut state = self.state.lock().unwrap();
            if !state.available {
                return Err("Wayland clipboard is unavailable".into());
            }
            let (formats, secret, expiry) = match (id, mode, text) {
                (None, None, Some(text)) if text.len() <= TEXT_LIMIT => {
                    (vec![content::plain_format(text)], None, None)
                }
                (Some(id), Some(mode), None) => {
                    if let Some(secret) = state.active_secret(&id) {
                        if !matches!(mode, PasteMode::Original) {
                            return Err("Use original format for a sensitive item".into());
                        }
                        let formats = secret
                            .formats
                            .iter()
                            .map(|format| Format {
                                mime: format.mime.clone(),
                                data: format.data.clone(),
                            })
                            .collect();
                        (formats, Some(id), Some(secret.expires))
                    } else {
                        let formats = match mode {
                            PasteMode::Original => state.history.formats(&id)?,
                            PasteMode::Plain => vec![content::plain_format(
                                state
                                    .history
                                    .text(&id)?
                                    .ok_or("Plain text is unavailable")?,
                            )],
                            PasteMode::Paths => vec![content::plain_format(
                                content::file_paths(&state.history.formats(&id)?)
                                    .ok_or("File paths are unavailable")?
                                    .join("\n"),
                            )],
                        };
                        (formats, None, None)
                    }
                }
                _ => return Err("Invalid clipboard publication request".into()),
            };
            if formats.is_empty() {
                return Err("Original clipboard content is unavailable".into());
            }
            let marker = format!("{}{}", self.marker_prefix, state.next_number());
            state.publishing = Some(Publishing {
                target: PublishTarget {
                    marker: marker.clone(),
                    secret,
                },
                observed: false,
            });
            (formats, marker, expiry)
        };
        let selection = Selection::start(&formats, &marker, expiry.is_some());
        drop(formats);
        let mut state = self.state.lock().unwrap();
        let selection = match selection {
            Ok(selection) => selection,
            Err(_) => {
                state.publishing = None;
                return Err("Could not start clipboard publication".into());
            }
        };
        let mut deadline = Instant::now() + PUBLISH_TIMEOUT;
        if let Some(expiry) = expiry {
            deadline = deadline.min(expiry);
        }
        loop {
            if Instant::now() >= deadline || state.publishing.is_none() {
                state.publishing = None;
                return Err("Could not confirm clipboard ownership".into());
            }
            if state
                .publishing
                .as_ref()
                .is_some_and(|pending| pending.observed)
            {
                let publishing = state.publishing.take().unwrap();
                state.owned = Some(OwnedSelection {
                    target: publishing.target,
                    _process: selection,
                });
                return Ok(());
            }
            state = self
                .changed
                .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
                .unwrap()
                .0;
        }
    }
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn reply<T: Serialize>(mut stream: impl Write, result: Result<T, Error>) -> io::Result<()> {
    #[derive(Serialize)]
    struct Success<T> {
        ok: bool,
        value: T,
    }
    match result {
        Ok(value) => serde_json::to_writer(&mut stream, &Success { ok: true, value })?,
        Err(error) => serde_json::to_writer(
            &mut stream,
            &json!({"ok": false, "error": error.to_string()}),
        )?,
    }
    stream.write_all(b"\n")?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{HTML, plain_format};

    #[test]
    fn unknown_policy_rejects_regular_sensitive_and_inflight_capture() {
        let service = Service::new(History::open(std::path::Path::new(":memory:")).unwrap());
        let capture = |sensitive| wayland::Capture {
            sensitive,
            formats: vec![plain_format("synthetic fixture".into())],
        };
        assert_eq!(service.begin_capture(&[HTML.into()], false), None);
        service.store_capture(capture(false), 0, now_ms(), Instant::now());
        service.store_capture(capture(true), 0, now_ms(), Instant::now());
        service
            .apply_policy(Some(crate::history::tests::default_policy()))
            .unwrap();
        let generation = service.begin_capture(&[HTML.into()], false).unwrap();
        service.apply_policy(None).unwrap();
        service.store_capture(capture(false), generation, now_ms(), Instant::now());
        service.store_capture(capture(true), generation, now_ms(), Instant::now());
        let state = service.state.lock().unwrap();
        assert!(state.secrets.is_empty());
        assert!(state.history.entries("").unwrap().is_empty());
        assert!(serde_json::from_str::<Request>(r#"{"op":"preferences"}"#).is_err());
        assert!(
            serde_json::from_str::<Request>(
                r#"{"op":"setPreference","key":"enabled","value":true}"#
            )
            .is_err()
        );
    }

    #[test]
    fn secrets_expire_and_never_enter_durable_history() {
        let mut history = History::open(std::path::Path::new(":memory:")).unwrap();
        history
            .set_policy(Some(crate::history::tests::default_policy()), 0)
            .unwrap();
        let service = Service::new(history);
        let copied = Instant::now();
        let capture = || wayland::Capture {
            sensitive: true,
            formats: vec![plain_format("synthetic test secret".into())],
        };
        service.store_capture(capture(), 0, now_ms(), copied);
        let list = service
            .execute(Request::List {
                query: String::new(),
            })
            .unwrap();
        assert_eq!(list[0]["kind"], "hidden");
        assert!(!list.to_string().contains("synthetic test secret"));
        let id = list[0]["id"].as_str().unwrap().to_owned();
        assert_eq!(&*service.reveal(&id).unwrap(), "synthetic test secret");
        assert!(service.execute(Request::Text { id: id.clone() }).is_err());
        let mut state = service.state.lock().unwrap();
        assert!(state.history.entries("").unwrap().is_empty());
        assert!(state.purge_secrets(|secret| secret.expires <= copied + SECRET_TTL));
        assert!(state.active_secret(&id).is_none());
    }

    #[test]
    fn self_published_formats_do_not_capture_and_ack_requires_matching_marker() {
        let mut history = History::open(std::path::Path::new(":memory:")).unwrap();
        history
            .set_policy(Some(crate::history::tests::default_policy()), 0)
            .unwrap();
        let service = Service::new(history);
        let marker = format!("{}fixture", service.marker_prefix);
        service.state.lock().unwrap().publishing = Some(Publishing {
            target: PublishTarget {
                marker: marker.clone(),
                secret: None,
            },
            observed: false,
        });
        assert_eq!(service.begin_capture(&[HTML.into()], false), Some(0));
        assert!(
            !service
                .state
                .lock()
                .unwrap()
                .publishing
                .as_ref()
                .unwrap()
                .observed
        );
        assert_eq!(service.begin_capture(&[marker], false), None);
        assert!(
            service
                .state
                .lock()
                .unwrap()
                .publishing
                .as_ref()
                .unwrap()
                .observed
        );
        assert_eq!(service.begin_capture(&[HTML.into()], false), Some(0));
        assert!(service.state.lock().unwrap().publishing.is_none());
    }
}
