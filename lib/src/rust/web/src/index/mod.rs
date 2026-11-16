mod candidates;
mod dto;
mod engines;
mod query;
mod ranking;
mod schema;
mod sql;
mod store;
mod util;

#[cfg(test)]
mod tests;

use rusqlite::Connection;

pub use engines::{engine_url, engines, is_supported_default_engine, validate_url};

pub(super) const MAX_FREQUENT_SITES: usize = 6;
pub(super) const OPEN_RANK_WINDOW_MS: i64 = 30 * 86_400_000;

/// Search parameters against the local launcher web index.
#[derive(Debug, Clone, Copy)]
pub struct Search<'a> {
    pub query: &'a str,
    pub engine_id: Option<&'a str>,
    pub profile_ids: Option<&'a [String]>,
    pub limit: usize,
}

/// Private SQLite projection index for browser bookmarks, history, and launcher usage.
pub struct Index {
    pub(super) conn: Connection,
}
