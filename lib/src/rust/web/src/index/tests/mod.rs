mod engines;
mod query;
mod store;

use std::path::PathBuf;

use tempfile::tempdir;

use super::Index;
use crate::browsers::{Entry, Family, Profile};

fn make_profile(id: &str, browser_id: &str, browser_label: &str, label: &str) -> Profile {
    Profile {
        id: id.to_owned(),
        browser_id: browser_id.to_owned(),
        browser_label: browser_label.to_owned(),
        label: label.to_owned(),
        path: PathBuf::from(format!("/synthetic/{id}")),
        family: if browser_id == "firefox" {
            Family::Gecko
        } else {
            Family::Chromium
        },
    }
}

fn make_entry(
    source_id: &str,
    bookmark: bool,
    url: &str,
    title: &str,
    visited_count: i64,
    last_visited_ms: Option<i64>,
    folder_path: &[&str],
) -> Entry {
    Entry {
        source_id: source_id.to_owned(),
        bookmark,
        url: url.to_owned(),
        title: title.to_owned(),
        visited_count,
        last_visited_ms,
        folder_path: folder_path.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn open_test_index() -> (tempfile::TempDir, Index) {
    let dir = tempdir().expect("tempdir");
    let db_path = dir.path().join("index.sqlite");
    let idx = Index::open(&db_path).expect("open index");
    (dir, idx)
}
