use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

static NEXT_STATE: AtomicU64 = AtomicU64::new(0);

struct State(PathBuf);

impl State {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "remya-calc-test-{}-{}",
            std::process::id(),
            NEXT_STATE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for State {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Sidecar {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    id: u64,
}

impl Sidecar {
    fn spawn(state: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_remya-launcher-calculator"))
            .env("XDG_STATE_HOME", state)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
            id: 0,
        }
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.output.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    }

    fn send(&mut self, mut fields: Value) -> String {
        self.id += 1;
        let id = format!("r{}", self.id);
        fields["v"] = json!(1);
        fields["id"] = json!(id);
        writeln!(self.input, "{fields}").unwrap();
        id
    }

    fn call(&mut self, fields: Value) -> Value {
        let id = self.send(fields);
        let response = self.read();
        assert_eq!(response["id"], id);
        response
    }

    fn ok(&mut self, fields: Value) -> Value {
        let response = self.call(fields);
        assert_eq!(response["ok"], true, "{response}");
        response["data"].clone()
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn preview(revision: u64, query: &str) -> Value {
    json!({"op":"preview","scope":"calc","query":query,"revision":revision})
}

#[test]
fn protocol_bounds_stale_frames_and_shutdown() {
    let state = State::new();
    let mut sidecar = Sidecar::spawn(&state.0);
    writeln!(sidecar.input, r#"{{"v":2,"id":"bad","op":"hello"}}"#).unwrap();
    assert_eq!(sidecar.read()["error"]["code"], "unsupported_version");
    assert_eq!(
        sidecar.call(preview(1, "1+1"))["error"]["code"],
        "handshake_required"
    );
    assert_eq!(sidecar.ok(json!({"op":"hello"}))["protocolVersion"], 1);

    sidecar.input.write_all(b"\xff\n").unwrap();
    assert_eq!(sidecar.read()["error"]["code"], "invalid_frame");
    sidecar.input.write_all(&vec![b'x'; 65_536]).unwrap();
    writeln!(sidecar.input).unwrap();
    assert_eq!(sidecar.read()["error"]["code"], "invalid_frame");
    writeln!(sidecar.input, r#"{{"v":1,"id":"r2","op":"hello"}}"#).unwrap();
    assert_eq!(sidecar.read()["error"]["code"], "duplicate_id");
    assert_eq!(
        sidecar.call(json!({"op":"hello","unexpected":true}))["error"]["code"],
        "invalid_request"
    );

    sidecar.ok(json!({"op":"cancel","revision":4}));
    assert_eq!(
        sidecar.call(preview(3, "1+1"))["error"]["code"],
        "interrupted"
    );
    assert_eq!(
        sidecar.ok(preview(4, "1+1"))["candidates"][0]["value"]["decimal"],
        "2"
    );
    assert_eq!(
        sidecar.call(preview(4, "2+2"))["error"]["code"],
        "stale_revision"
    );

    let first = sidecar.send(preview(5, "2^(2^9999)"));
    let latest = sidecar.send(preview(6, "3pi sqrt 2"));
    let replies = [sidecar.read(), sidecar.read()];
    assert!(replies.iter().any(|reply| reply["id"] == first));
    let last = replies.iter().find(|reply| reply["id"] == latest).unwrap();
    assert_eq!(
        last["data"]["candidates"][0]["value"]["text"],
        "approx. 13.3286488145"
    );
    let filename = sidecar
        .ok(json!({"op":"preview","scope":"all","query":"wallpaper-1920x1080.png","revision":7}));
    assert_eq!(filename["status"], "empty");

    let completion = sidecar.ok(preview(8, "1 kilo"));
    assert_eq!(completion["status"], "completion");
    assert!(
        completion["completions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["canonicalSymbol"] == "kWh")
    );
    let catalog = sidecar.ok(preview(9, "speed of light"));
    assert_eq!(catalog["status"], "catalog");
    assert_eq!(catalog["catalogEntries"][0]["canonicalSymbol"], "c");
    let bounded = sidecar.ok(preview(10, "1e-2147483648 kg to g"));
    assert_eq!(bounded["diagnostics"][0]["code"], "numeric_overflow");

    sidecar.ok(json!({"op":"shutdown"}));
    assert!(sidecar.child.wait().unwrap().success());
}

#[test]
fn durable_idempotency_variable_isolation_and_history() {
    let state = State::new();
    let mut sidecar = Sidecar::spawn(&state.0);
    sidecar.ok(json!({"op":"hello"}));
    for revision in 1..=1000 {
        sidecar.ok(preview(revision, "userx=2"));
    }
    assert_eq!(
        sidecar.ok(json!({"op":"historyList"}))["entries"],
        json!([])
    );
    assert_eq!(
        sidecar.ok(json!({"op":"variablesList"}))["entries"],
        json!([])
    );
    assert_eq!(sidecar.ok(preview(1001, "userx"))["candidates"], json!([]));

    sidecar
        .ok(json!({"op":"saveVariable","name":"userx","expression":"2","idempotencyKey":"var1"}));
    sidecar.ok(
        json!({"op":"saveVariable","name":"usery","expression":"userx+1","idempotencyKey":"var2"}),
    );
    let cyclic = sidecar.call(
        json!({"op":"saveVariable","name":"userx","expression":"usery","idempotencyKey":"var3"}),
    );
    assert_eq!(cyclic["error"]["code"], "variable_cycle");
    assert_eq!(
        sidecar.call(
            json!({"op":"saveVariable","name":"pi","expression":"2","idempotencyKey":"var4"})
        )["error"]["code"],
        "invalid_variable"
    );
    let data = sidecar.ok(preview(1002, "usery+1"));
    assert_eq!(data["candidates"][0]["value"]["text"], "4");

    let candidate = &data["primaryCandidateId"];
    let stale = sidecar.call(
        json!({"op":"commit","candidateId":candidate,"revision":1001,"idempotencyKey":"stale"}),
    );
    assert_eq!(stale["error"]["code"], "stale_revision");
    let request =
        json!({"op":"commit","candidateId":candidate,"revision":1002,"idempotencyKey":"commit1"});
    let committed = sidecar.ok(request.clone());
    assert_eq!(committed["copyText"], "4");
    assert_eq!(sidecar.ok(request.clone()), committed);
    let hist_id = &committed["historyId"];
    sidecar.ok(json!({"op":"historyPin","historyId":hist_id,"pinned":true,"idempotencyKey":"pin"}));
    sidecar.ok(json!({"op":"shutdown"}));
    assert!(sidecar.child.wait().unwrap().success());

    let mut reopened = Sidecar::spawn(&state.0);
    reopened.ok(json!({"op":"hello"}));
    assert_eq!(reopened.ok(request), committed);
    let conflict = reopened.call(
        json!({"op":"commit","candidateId":candidate,"revision":1003,"idempotencyKey":"commit1"}),
    );
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
    let history = reopened.ok(json!({"op":"historyList"}));
    assert_eq!(history["entries"].as_array().unwrap().len(), 1);
    assert_eq!(history["entries"][0]["pinned"], true);
    assert_eq!(history["entries"][0]["query"], "usery+1");
    reopened.ok(json!({"op":"historyClear","idempotencyKey":"clear1"}));
    assert_eq!(
        reopened.ok(json!({"op":"historyList"}))["entries"],
        history["entries"]
    );
    reopened.ok(json!({"op":"historyDelete","historyId":hist_id,"idempotencyKey":"delete1"}));
    assert_eq!(
        reopened.ok(json!({"op":"historyList"}))["entries"],
        json!([])
    );
}

#[test]
fn future_or_corrupt_storage_preserves_data_and_math() {
    for future_version in [false, true] {
        let state = State::new();
        let directory = state.0.join("remya/launcher");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("calculator.sqlite3");
        if future_version {
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch("CREATE TABLE future_data(value TEXT); INSERT INTO future_data VALUES ('keep'); PRAGMA user_version=2;").unwrap();
        } else {
            fs::write(&path, b"invalid SQLite file").unwrap();
        }
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let original = fs::read(&path).unwrap();

        let mut sidecar = Sidecar::spawn(&state.0);
        sidecar.ok(json!({"op":"hello"}));
        assert_eq!(sidecar.ok(json!({"op":"status"}))["status"], "degraded");
        assert_eq!(
            sidecar.ok(preview(1, "2+2"))["candidates"][0]["value"]["decimal"],
            "4"
        );
        assert_eq!(
            sidecar.call(json!({"op":"historyList"}))["error"]["code"],
            "storage_unavailable"
        );
        sidecar.ok(json!({"op":"shutdown"}));
        assert!(sidecar.child.wait().unwrap().success());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
}
