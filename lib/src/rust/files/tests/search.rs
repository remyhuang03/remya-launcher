use serde_json::{Value, json};
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::{Command, Stdio};

fn request(home: &Path, mut request: Value) -> Value {
    request["op"] = json!("search");
    send(home, request)
}

fn send(home: &Path, mut request: Value) -> Value {
    request["id"] = json!(1);
    request["preferences"]["files-search"]["roots"] = json!([home]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_remya-launcher-files"))
        .arg("--stdio")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_STATE_HOME", home.join(".state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_DATA_HOME", home.join(".data"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{request}").unwrap();
    let response = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map(|line| serde_json::from_str::<Value>(&line.unwrap()).unwrap())
        .last()
        .unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(response["id"], 1);
    assert!(response["error"].is_null(), "{response}");
    response["data"].clone()
}

#[test]
fn path_and_content_search_report_pin_state() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    fs::write(home.join("report.txt"), "first needle\nsecond needle\n").unwrap();
    let path = request(home, json!({"query":"~/report"}));
    let id = path["entries"][0]["id"].as_str().unwrap();
    assert!(path["entries"][0].get("pinned").is_none());
    assert!(!home.join(".data").exists());
    send(home, json!({"op":"history.pin","entryId":id}));

    for query in ["~/report", "rg needle"] {
        let result = request(home, json!({"query":query}));
        assert_eq!(result["entries"][0]["id"], id);
        assert_eq!(result["entries"][0]["group"], "results");
        assert_eq!(result["entries"][0]["pinned"], true);
    }

    send(home, json!({"op":"history.unpin","entryId":id}));
    let result = request(home, json!({"query":"rg needle"}));
    assert!(result["entries"][0].get("pinned").is_none());
}

#[test]
fn names_ignore_content_and_byte_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    for sub in ["bar/docs", ".ssh", "vault[2]", "skipped"] {
        fs::create_dir_all(home.join(sub)).unwrap();
    }
    fs::write(home.join(".gitignore"), "skipped/\n").unwrap();
    for file in [
        "bar/docs/spec.tar.gz",
        ".ssh/spec.tar.gz",
        "vault[2]/spec.tar.gz",
        "skipped/spec.tar.gz",
    ] {
        fs::write(home.join(file), "first needle\nsecond needle\n").unwrap();
    }
    symlink(home.join(".ssh/spec.tar.gz"), home.join("visible-link")).unwrap();
    let prefs = json!({"files-search":{"exclusions":[home.join("vault[2]")],"show_hidden":true}});
    let out = request(
        home,
        json!({"query":"bar/docs spc ext:tar.gz type:file","preferences":prefs}),
    );
    assert_eq!(out["entries"].as_array().unwrap().len(), 1, "{out}");
    assert_eq!(out["entries"][0]["name"], "spec.tar.gz");
    let out = request(home, json!({"query":"rg needle","preferences":prefs}));
    assert_eq!(out["entries"].as_array().unwrap().len(), 1, "{out}");
    assert_eq!(out["entries"][0]["hitCount"], 2);
    assert_eq!(out["entries"][0]["hits"][1]["line"], 2);
    assert_eq!(out["entries"][0]["hits"][1]["snippet"], "second needle");
    for b in [0xfe, 0xff] {
        fs::write(home.join(OsString::from_vec(vec![b'a', b'b', b])), []).unwrap();
    }
    let out = request(home, json!({"query":"ab","preferences":prefs}));
    let entries = out["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["path"], entries[1]["path"]);
    assert_ne!(entries[0]["id"], entries[1]["id"]);
    assert!(entries[0]["completion"].is_null());
}

#[test]
fn path_completion_creation_and_real_places() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    fs::create_dir_all(home.join(".config")).unwrap();
    fs::create_dir(home.join("Reports")).unwrap();
    fs::write(
        home.join(".config/user-dirs.dirs"),
        "XDG_DOCUMENTS_DIR=\"$HOME/Reports\"\n",
    )
    .unwrap();
    for n in ["file10", "file2", ".dot"] {
        fs::write(home.join(n), []).unwrap();
    }
    let empty = request(home, json!({"query":""}));
    assert!(
        empty["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "Home")
    );
    assert!(
        empty["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == "Reports")
    );
    let path = request(home, json!({"query":"~/file"}));
    assert_eq!(path["entries"][0]["name"], "file2");
    let dots = request(home, json!({"query":"~/."}));
    assert!(
        dots["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["name"] == ".dot")
    );
    let literal = "folder with  two spaces and \"quotes\"";
    fs::create_dir(home.join(literal)).unwrap();
    fs::write(home.join(literal).join("draft.md"), []).unwrap();
    fs::write(home.join(literal).join("draft.txt"), []).unwrap();
    let spaced = request(
        home,
        json!({"query":format!("~/{literal}/"),"filters":["ext:md"]}),
    );
    assert_eq!(spaced["entries"].as_array().unwrap().len(), 1);
    assert_eq!(spaced["entries"][0]["name"], "draft.md");
    let unfinished = request(home, json!({"query":"size:>oops"}));
    assert!(unfinished["diagnostic"].is_string());
    let create = request(home, json!({"query":"~/brand new/nested/leaf/"}));
    assert_eq!(create["createName"], "leaf");
    assert_eq!(create["createRelative"], "brand new/nested/leaf");
    assert_eq!(create["createAncestors"], json!(["brand new", "nested"]));
    assert_eq!(create["createDirectoryOnly"], true);
    assert!(create["directoryVersion"].is_string());
    assert!(!home.join("brand new").exists());
}
