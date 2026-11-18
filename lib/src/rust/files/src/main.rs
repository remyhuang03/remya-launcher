mod actions;
mod history;
mod model;
mod preview;
mod query;
mod search;

use model::{Policy, decode_id};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

static OUTPUT: OnceLock<Mutex<BufWriter<File>>> = OnceLock::new();

fn reply(id: &Value, result: Result<Value, String>) {
    let response = match result {
        Ok(data) => json!({"id":id,"data":data}),
        Err(error) => json!({"id":id,"error":error}),
    };
    let mut out = OUTPUT.get().expect("initialized output").lock().unwrap();
    let _ = serde_json::to_writer(&mut *out, &response);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}

#[derive(Default)]
struct Searches {
    pending: Option<(Value, Arc<AtomicBool>)>,
    running: Option<(Value, Arc<AtomicBool>)>,
    finished: bool,
}

fn service(
    request: &Value,
    actions: &mut actions::Actions,
    opened: &mut VecDeque<String>,
) -> Result<Value, String> {
    let prefs = &request["preferences"];
    let policy = Policy::new(prefs);
    let op = request["op"].as_str().unwrap_or("");
    if op.starts_with("history.") {
        return history::dispatch(request, &policy);
    }
    let action = request["action"].as_str().unwrap_or("");
    let path = if op == "action" && action == "undo" {
        PathBuf::new()
    } else {
        decode_id(request["entryId"].as_str().ok_or("Select a file first.")?)?
    };
    match op {
        "preview" => preview::read(&path, request, |p| policy.allowed(p)),
        "action" => {
            let opening = matches!(action, "open" | "openWith" | "editor");
            let operation = request["operationId"].as_str().unwrap_or_default();
            if opening {
                if operation.is_empty() || operation.len() > 128 {
                    return Err("Invalid operation ID.".into());
                }
                if opened.iter().any(|id| id == operation) {
                    return Err("This open request has already been handled.".into());
                }
                if opened.len() >= 128 {
                    opened.pop_front();
                }
                opened.push_back(operation.to_owned());
            }
            let mut data = actions.perform(&path, request, |p| policy.allowed(p))?;
            if opening && data["dispatched"] == true {
                data["history"] =
                    history::record(prefs, &policy, &path, operation, action == "editor")
                        .unwrap_or_else(|e| json!({"error":e}));
            }
            if let (Some(before), Some(after)) =
                (data["previousEntryId"].as_str(), data["entryId"].as_str())
                && let (Ok(before), Ok(after)) = (decode_id(before), decode_id(after))
                && let Err(error) = history::renamed(&before, &after, prefs)
            {
                data["history"] = json!({"error":error});
            }
            Ok(data)
        }
        _ => Err("Unknown Files operation.".into()),
    }
}

fn main() -> io::Result<()> {
    if std::env::args_os()
        .nth(1)
        .is_some_and(|s| s == "--preview-image")
    {
        preview::image_worker();
    }
    // A GIO-launched desktop application can inherit fd 0 and 1 and print arbitrary text.
    let input = File::from(rustix::io::fcntl_dupfd_cloexec(io::stdin(), 3)?);
    let output = File::from(rustix::io::fcntl_dupfd_cloexec(io::stdout(), 3)?);
    let null = File::options().read(true).write(true).open("/dev/null")?;
    rustix::stdio::dup2_stdin(&null)?;
    rustix::stdio::dup2_stdout(&null)?;
    OUTPUT
        .set(Mutex::new(BufWriter::new(output)))
        .expect("single service");
    let state = Arc::new((Mutex::new(Searches::default()), Condvar::new()));
    let worker_state = Arc::clone(&state);
    let worker = std::thread::spawn(move || {
        let (lock, signal) = &*worker_state;
        loop {
            let mut jobs = lock.lock().unwrap();
            while jobs.pending.is_none() && !jobs.finished {
                jobs = signal.wait(jobs).unwrap();
            }
            let Some((request, cancel)) = jobs.pending.take() else {
                break;
            };
            jobs.running = Some((request["id"].clone(), Arc::clone(&cancel)));
            drop(jobs);
            let result =
                search::search(&request, &cancel, |batch| reply(&request["id"], Ok(batch)));
            reply(&request["id"], result);
            lock.lock().unwrap().running = None;
        }
    });
    let mut actions = actions::Actions::default();
    let mut opened = VecDeque::new();
    let (lock, signal) = &*state;
    let mut input = BufReader::new(input);
    let mut line = Vec::new();
    const MAX_FRAME_BYTES: u64 = 4 * 1024 * 1024;
    loop {
        line.clear();
        // Bound a frame before allocating for it, including on malformed input.
        use std::io::Read;
        let Ok(n) = (&mut input)
            .take(MAX_FRAME_BYTES + 1)
            .read_until(b'\n', &mut line)
        else {
            break;
        };
        if n == 0 {
            break;
        }
        if n as u64 > MAX_FRAME_BYTES {
            reply(&Value::Null, Err("Files request is too large.".into()));
            break;
        }
        let Ok(request) = serde_json::from_slice::<Value>(&line) else {
            reply(&Value::Null, Err("Invalid Files request.".into()));
            continue;
        };
        let id = &request["id"];
        match request["op"].as_str().unwrap_or("") {
            "search" | "cancel" => {
                let mut jobs = lock.lock().unwrap();
                let new_search = request["op"] == "search";
                let target = &request["targetRequestId"];
                if let Some((rid, token)) = &jobs.running
                    && (new_search || target.is_null() || target == rid)
                {
                    token.store(true, Ordering::Relaxed);
                }
                if jobs
                    .pending
                    .as_ref()
                    .is_some_and(|(r, _)| new_search || target.is_null() || target == &r["id"])
                    && let Some((old, _)) = jobs.pending.take()
                {
                    reply(&old["id"], Err("Search cancelled.".into()));
                }
                if new_search {
                    jobs.pending = Some((request, Arc::new(AtomicBool::new(false))));
                    signal.notify_one();
                } else {
                    reply(id, Ok(json!({"cancelled":true})));
                }
            }
            "shutdown" => {
                reply(id, Ok(json!({})));
                break;
            }
            _ => reply(id, service(&request, &mut actions, &mut opened)),
        }
    }
    lock.lock().unwrap().finished = true;
    signal.notify_one();
    let _ = worker.join();
    Ok(())
}
