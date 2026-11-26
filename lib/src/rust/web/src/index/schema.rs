use rusqlite::{Transaction, params};

const SCHEMA_VERSION: i64 = 3;

pub(super) fn init_schema(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    let version: i64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CORRUPT),
            Some("Unsupported newer web index schema version".to_owned()),
        ));
    }

    if version == 1 {
        tx.execute_batch(
            "ALTER TABLE opt_ins ADD COLUMN consent_token TEXT;
             UPDATE opt_ins
             SET consent_token = lower(hex(randomblob(16)))
             WHERE enabled = 1 AND consent_token IS NULL;",
        )?;
    }

    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS opt_ins (
           profile_id TEXT PRIMARY KEY,
           browser_id TEXT NOT NULL DEFAULT '',
           browser_label TEXT NOT NULL DEFAULT '',
           profile_label TEXT NOT NULL DEFAULT '',
           enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
           updated_ms INTEGER NOT NULL,
           consent_token TEXT
         );

         CREATE TABLE IF NOT EXISTS sync_generations (
           profile_id TEXT PRIMARY KEY,
           generation INTEGER NOT NULL DEFAULT 0
         );

         CREATE TABLE IF NOT EXISTS entries (
           rowid INTEGER PRIMARY KEY AUTOINCREMENT,
           entry_id TEXT NOT NULL UNIQUE,
           profile_id TEXT NOT NULL,
           browser_id TEXT NOT NULL,
           browser_label TEXT NOT NULL,
           profile_label TEXT NOT NULL,
           kind TEXT NOT NULL CHECK(kind IN ('bookmark', 'history')),
           source_id TEXT NOT NULL,
           raw_url TEXT NOT NULL,
           normalized_url TEXT NOT NULL,
           host TEXT NOT NULL,
           origin TEXT NOT NULL,
           title TEXT NOT NULL,
           folder_path_json TEXT NOT NULL,
           folder_path_text TEXT NOT NULL,
           visited_count INTEGER NOT NULL DEFAULT 0,
           last_visited_ms INTEGER,
           seen_generation INTEGER NOT NULL,
           UNIQUE(profile_id, kind, source_id)
         );

         CREATE INDEX IF NOT EXISTS idx_entries_profile_kind_gen
           ON entries(profile_id, kind, seen_generation);
         CREATE INDEX IF NOT EXISTS idx_entries_profile_recent
           ON entries(profile_id, last_visited_ms DESC);
         CREATE INDEX IF NOT EXISTS idx_entries_global_recent
           ON entries(COALESCE(last_visited_ms, 0) DESC, visited_count DESC, kind ASC, entry_id ASC);
         CREATE INDEX IF NOT EXISTS idx_entries_normalized_url
           ON entries(normalized_url);
         CREATE INDEX IF NOT EXISTS idx_entries_host
           ON entries(host);
         CREATE INDEX IF NOT EXISTS idx_entries_title_nocase
           ON entries(title COLLATE NOCASE);
         CREATE INDEX IF NOT EXISTS idx_entries_host_nocase
           ON entries(host COLLATE NOCASE);
         CREATE INDEX IF NOT EXISTS idx_entries_normalized_url_nocase
           ON entries(normalized_url COLLATE NOCASE);

         CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
           title,
           normalized_url,
           host,
           folder_path_text,
           content='entries',
           content_rowid='rowid',
           tokenize='trigram'
         );

         CREATE TRIGGER IF NOT EXISTS entries_ai AFTER INSERT ON entries BEGIN
           INSERT INTO entries_fts(rowid, title, normalized_url, host, folder_path_text)
           VALUES (new.rowid, new.title, new.normalized_url, new.host, new.folder_path_text);
         END;

         CREATE TRIGGER IF NOT EXISTS entries_ad AFTER DELETE ON entries BEGIN
           INSERT INTO entries_fts(entries_fts, rowid, title, normalized_url, host, folder_path_text)
           VALUES ('delete', old.rowid, old.title, old.normalized_url, old.host, old.folder_path_text);
         END;

         CREATE TRIGGER IF NOT EXISTS entries_au
         AFTER UPDATE OF title, normalized_url, host, folder_path_text ON entries
         WHEN old.title IS NOT new.title
           OR old.normalized_url IS NOT new.normalized_url
           OR old.host IS NOT new.host
           OR old.folder_path_text IS NOT new.folder_path_text
         BEGIN
           INSERT INTO entries_fts(entries_fts, rowid, title, normalized_url, host, folder_path_text)
           VALUES ('delete', old.rowid, old.title, old.normalized_url, old.host, old.folder_path_text);
           INSERT INTO entries_fts(rowid, title, normalized_url, host, folder_path_text)
           VALUES (new.rowid, new.title, new.normalized_url, new.host, new.folder_path_text);
         END;

         CREATE TABLE IF NOT EXISTS launcher_opens (
           event_id TEXT PRIMARY KEY,
           profile_id TEXT,
           normalized_url TEXT NOT NULL,
           host TEXT NOT NULL,
           opened_ms INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_launcher_opens_url_time
           ON launcher_opens(normalized_url, opened_ms DESC);
         CREATE INDEX IF NOT EXISTS idx_launcher_opens_time
           ON launcher_opens(opened_ms DESC);

         CREATE TABLE IF NOT EXISTS hidden (
           profile_id TEXT NOT NULL,
           scope TEXT NOT NULL CHECK(scope IN ('url', 'host')),
           target TEXT NOT NULL,
           hidden_ms INTEGER NOT NULL,
           PRIMARY KEY(profile_id, scope, target)
         );

         CREATE TABLE IF NOT EXISTS pins (
           entry_id TEXT PRIMARY KEY REFERENCES entries(entry_id) ON DELETE CASCADE
         );

         CREATE TABLE IF NOT EXISTS web_favicons (
           profile_id TEXT NOT NULL REFERENCES opt_ins(profile_id) ON DELETE CASCADE,
           normalized_url TEXT NOT NULL,
           host TEXT NOT NULL,
           png BLOB NOT NULL,
           fetched_ms INTEGER NOT NULL,
           PRIMARY KEY(profile_id, normalized_url)
         );

         CREATE TABLE IF NOT EXISTS web_preferences (
           id INTEGER PRIMARY KEY CHECK(id = 1),
           default_engine_id TEXT NOT NULL CHECK(default_engine_id IN ('duckduckgo', 'google'))
         );",
    )?;

    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    Ok(())
}

pub(super) fn purge_profile_data(tx: &Transaction<'_>, profile_id: &str) -> rusqlite::Result<()> {
    tx.execute(
        "DELETE FROM web_favicons WHERE profile_id = ?1",
        params![profile_id],
    )?;
    tx.execute(
        "DELETE FROM entries WHERE profile_id = ?1",
        params![profile_id],
    )?;
    tx.execute(
        "DELETE FROM launcher_opens WHERE profile_id = ?1",
        params![profile_id],
    )?;
    tx.execute(
        "DELETE FROM hidden WHERE profile_id = ?1",
        params![profile_id],
    )?;
    tx.execute(
        "DELETE FROM sync_generations WHERE profile_id = ?1",
        params![profile_id],
    )?;
    Ok(())
}
