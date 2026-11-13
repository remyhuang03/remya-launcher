use crate::content::{Content, Entry, Format, IMAGE_TYPES, PLAIN_TYPES, Summary};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params};
use serde::Serialize;
use std::path::Path;

const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub enabled: bool,
    pub max_items: u32,
    pub retention_days: u32,
}

pub struct History {
    db: Connection,
    preferences: Option<Preferences>,
    policy_known: bool,
}

impl History {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let db = Connection::open_with_flags(
            path,
            OpenFlags::default() | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        Self::initialize(db)
    }

    fn initialize(mut db: Connection) -> rusqlite::Result<Self> {
        db.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA auto_vacuum = INCREMENTAL;
             PRAGMA secure_delete = FAST;",
        )?;
        let transaction = db.transaction()?;
        transaction.execute_batch(
            "CREATE TABLE IF NOT EXISTS entries (
                 id TEXT NOT NULL UNIQUE,
                 copied_at INTEGER NOT NULL,
                 times_copied INTEGER NOT NULL DEFAULT 1,
                 pinned INTEGER NOT NULL DEFAULT 0,
                 summary TEXT NOT NULL,
                 search_text TEXT NOT NULL,
                 detail TEXT NOT NULL,
                 thumbnail BLOB
             );
             CREATE INDEX IF NOT EXISTS entries_retention ON entries(pinned DESC, copied_at DESC);
             CREATE TABLE IF NOT EXISTS formats (
                 entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
                 mime TEXT NOT NULL,
                 payload BLOB NOT NULL,
                 PRIMARY KEY(entry_id, mime)
             ) WITHOUT ROWID;",
        )?;
        transaction.commit()?;
        Ok(Self {
            db,
            preferences: None,
            policy_known: false,
        })
    }

    pub fn enabled(&self) -> bool {
        self.policy().is_some_and(|policy| policy.enabled)
    }

    fn policy(&self) -> Option<Preferences> {
        self.preferences.filter(|_| self.policy_known)
    }

    pub fn store(&mut self, content: Content, now: i64) -> rusqlite::Result<()> {
        if !self.enabled() {
            return Ok(());
        }
        let policy = self.policy().unwrap();
        let transaction = self.db.transaction()?;
        if !refresh_existing(&transaction, &content.id, now)? {
            // shortcut: search bounded previews; add a full-text index if users need deep matches.
            let search = format!(
                "{}\n{}\n{}\n{}",
                content.description.title,
                content.description.summary,
                content.detail.text.as_deref().unwrap_or_default(),
                content
                    .detail
                    .files
                    .as_deref()
                    .unwrap_or_default()
                    .join("\n"),
            )
            .to_lowercase();
            transaction.execute(
                "INSERT INTO entries (id, copied_at, summary, search_text, detail, thumbnail)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    content.id,
                    now,
                    json_encode(&content.description)?,
                    search,
                    json_encode(&content.detail)?,
                    content.thumbnail,
                ],
            )?;
            let mut insert_format = transaction
                .prepare("INSERT INTO formats (entry_id, mime, payload) VALUES (?1, ?2, ?3)")?;
            for format in content.formats {
                insert_format.execute(params![content.id, format.mime, format.data])?;
            }
        }
        let removed = prune(&transaction, policy, now)?;
        transaction.commit()?;
        self.reclaim(removed);
        Ok(())
    }

    pub fn refresh_existing(&self, id: &str, now: i64) -> rusqlite::Result<bool> {
        if !self.enabled() {
            return Ok(false);
        }
        refresh_existing(&self.db, id, now)
    }

    pub fn entries(&self, query: &str) -> rusqlite::Result<Vec<Entry>> {
        let mut statement = self.db.prepare_cached(
            "SELECT id, summary, copied_at, times_copied, pinned FROM entries
             WHERE ?1 = '' OR instr(search_text, ?1) > 0
             ORDER BY pinned DESC, copied_at DESC, rowid DESC",
        )?;
        statement
            .query_map([query.to_lowercase()], |row| {
                let raw: String = row.get(1)?;
                let description: Summary = serde_json::from_str(&raw).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        1,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(Entry {
                    id: row.get(0)?,
                    description,
                    copied_at: row.get(2)?,
                    times_copied: row.get(3)?,
                    pinned: row.get(4)?,
                    expires_at: None,
                })
            })?
            .collect()
    }

    pub fn detail(&self, id: &str) -> rusqlite::Result<Option<(String, Option<Vec<u8>>)>> {
        self.db
            .query_row(
                "SELECT detail, thumbnail FROM entries WHERE id = ?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
    }

    pub fn thumbnail(&self, id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
        self.db
            .query_row("SELECT thumbnail FROM entries WHERE id = ?1", [id], |row| {
                row.get::<_, Option<Vec<u8>>>(0)
            })
            .optional()
            .map(Option::flatten)
    }

    pub fn formats(&self, id: &str) -> rusqlite::Result<Vec<Format>> {
        let mut statement = self.db.prepare_cached(
            "SELECT mime, payload FROM formats WHERE entry_id = ?1 ORDER BY mime",
        )?;
        statement
            .query_map([id], |row| {
                Ok(Format {
                    mime: row.get(0)?,
                    data: row.get(1)?,
                })
            })?
            .collect()
    }

    pub fn text(&self, id: &str) -> rusqlite::Result<Option<String>> {
        Ok(self
            .first_format(id, PLAIN_TYPES)?
            .and_then(|mut format| String::from_utf8(std::mem::take(&mut format.data)).ok()))
    }

    pub fn image(&self, id: &str) -> rusqlite::Result<Option<Format>> {
        self.first_format(id, IMAGE_TYPES)
    }

    fn first_format(&self, id: &str, types: &[&str]) -> rusqlite::Result<Option<Format>> {
        let mut statement = self
            .db
            .prepare_cached("SELECT payload FROM formats WHERE entry_id = ?1 AND mime = ?2")?;
        for &mime in types {
            if let Some(data) = statement
                .query_row(params![id, mime], |row| row.get(0))
                .optional()?
            {
                return Ok(Some(Format {
                    mime: mime.into(),
                    data,
                }));
            }
        }
        Ok(None)
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool, now: i64) -> rusqlite::Result<bool> {
        let policy = self.policy();
        let transaction = self.db.transaction()?;
        let updated = transaction.execute(
            "UPDATE entries SET pinned = ?2 WHERE id = ?1",
            params![id, pinned],
        )?;
        let removed = if let Some(policy) = policy.filter(|_| !pinned) {
            prune(&transaction, policy, now)?
        } else {
            0
        };
        transaction.commit()?;
        self.reclaim(removed);
        Ok(updated != 0)
    }

    pub fn delete(&mut self, id: &str) -> rusqlite::Result<bool> {
        let removed = self.db.execute("DELETE FROM entries WHERE id = ?1", [id])?;
        self.reclaim(removed);
        Ok(removed != 0)
    }

    pub fn clear(&mut self) -> rusqlite::Result<()> {
        self.db.execute("DELETE FROM entries", [])?;
        let _ = self.db.execute_batch("PRAGMA incremental_vacuum;");
        Ok(())
    }

    pub fn set_policy(&mut self, policy: Option<Preferences>, now: i64) -> rusqlite::Result<()> {
        let Some(policy) = policy else {
            self.policy_known = false;
            return Ok(());
        };
        let transaction = self.db.transaction()?;
        let removed = prune(&transaction, policy, now)?;
        transaction.commit()?;
        self.preferences = Some(policy);
        self.policy_known = true;
        self.reclaim(removed);
        Ok(())
    }

    pub fn prune(&mut self, now: i64) -> rusqlite::Result<bool> {
        let Some(policy) = self.policy() else {
            return Ok(false);
        };
        let transaction = self.db.transaction()?;
        let removed = prune(&transaction, policy, now)?;
        transaction.commit()?;
        self.reclaim(removed);
        Ok(removed != 0)
    }

    pub fn next_expiry(&self) -> rusqlite::Result<Option<i64>> {
        let Some(policy) = self.policy() else {
            return Ok(None);
        };
        self.db
            .query_row(
                "SELECT copied_at FROM entries WHERE pinned = 0 ORDER BY copied_at LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map(|copied| copied.map(|time| time + i64::from(policy.retention_days) * DAY_MS))
    }

    fn reclaim(&self, removed: usize) {
        if removed != 0 {
            // Free pages in small batches during normal capture and settings edits.
            let _ = self.db.execute_batch("PRAGMA incremental_vacuum(128);");
        }
    }
}

fn refresh_existing(db: &Connection, id: &str, now: i64) -> rusqlite::Result<bool> {
    db.execute(
        "UPDATE entries SET copied_at = ?2, times_copied = times_copied + 1 WHERE id = ?1",
        params![id, now],
    )
    .map(|updated| updated != 0)
}

fn prune(
    transaction: &Transaction<'_>,
    preferences: Preferences,
    now: i64,
) -> rusqlite::Result<usize> {
    let cutoff = now - i64::from(preferences.retention_days) * DAY_MS;
    let expired = transaction.execute(
        "DELETE FROM entries WHERE pinned = 0 AND copied_at <= ?1",
        [cutoff],
    )?;
    let excess = transaction.execute(
        "DELETE FROM entries WHERE rowid IN (
             SELECT rowid FROM entries WHERE pinned = 0
             ORDER BY copied_at DESC, rowid DESC LIMIT -1 OFFSET ?1
         )",
        [preferences.max_items],
    )?;
    Ok(expired + excess)
}

fn json_encode(value: &impl Serialize) -> rusqlite::Result<String> {
    serde_json::to_string(value)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::content::{HTML, fixture, plain_format};

    pub fn default_policy() -> Preferences {
        Preferences {
            enabled: true,
            max_items: 500,
            retention_days: 30,
        }
    }

    #[test]
    fn unknown_policy_never_records_prunes_or_expires() {
        let mut history = History::initialize(Connection::open_in_memory().unwrap()).unwrap();
        history.store(item("unknown"), 0).unwrap();
        assert!(history.entries("").unwrap().is_empty());
        history.set_policy(Some(default_policy()), 0).unwrap();
        let old = item("keep");
        let id = old.id.clone();
        history.store(old, 0).unwrap();
        history.set_policy(None, 40 * DAY_MS).unwrap();
        history.store(item("skip"), 40 * DAY_MS).unwrap();
        assert!(!history.refresh_existing(&id, 40 * DAY_MS).unwrap());
        history.set_pinned(&id, false, 40 * DAY_MS).unwrap();
        assert!(!history.prune(40 * DAY_MS).unwrap());
        assert_eq!(history.next_expiry().unwrap(), None);
        assert_eq!(history.entries("").unwrap().len(), 1);
        assert_eq!(history.preferences, Some(default_policy()));
        history
            .set_policy(Some(default_policy()), 40 * DAY_MS)
            .unwrap();
        assert!(history.entries("").unwrap().is_empty());
    }

    fn item(value: &str) -> Content {
        fixture(vec![plain_format(value.into())])
    }

    #[test]
    fn persistence_round_trips_formats_and_deduplicates() {
        let mut history = History::initialize(Connection::open_in_memory().unwrap()).unwrap();
        history.set_policy(Some(default_policy()), 0).unwrap();
        let rich = || {
            fixture(vec![
                plain_format(" a plain value \nsecond searchable line".into()),
                Format {
                    mime: HTML.into(),
                    data: b"<p>a plain value</p>".to_vec(),
                },
            ])
        };
        let id = rich().id;
        history.store(rich(), 0).unwrap();
        history.set_pinned(&id, true, 0).unwrap();
        history.store(rich(), 123).unwrap();
        let rows = history.entries("SEARCHABLE").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, id);
        assert_eq!(rows[0].times_copied, 2);
        assert_eq!(rows[0].copied_at, 123);
        assert!(rows[0].pinned);
        let formats = history.formats(&id).unwrap();
        assert_eq!(formats[0].mime, HTML);
        assert_eq!(formats[0].data, b"<p>a plain value</p>");
        assert_eq!(
            history.text(&id).unwrap().as_deref(),
            Some(" a plain value \nsecond searchable line")
        );
    }

    #[test]
    fn tightening_policy_prunes_only_unpinned_entries() {
        let mut history = History::initialize(Connection::open_in_memory().unwrap()).unwrap();
        history.set_policy(Some(default_policy()), 0).unwrap();
        let pinned = item("keep pinned");
        let id = pinned.id.clone();
        history.store(pinned, 0).unwrap();
        history.set_pinned(&id, true, 0).unwrap();
        history.store(item("expired"), 1).unwrap();
        history.store(item("excess"), DAY_MS + 1).unwrap();
        history.store(item("newest"), DAY_MS + 2).unwrap();
        let policy = Preferences {
            enabled: false,
            max_items: 1,
            retention_days: 1,
        };
        history.set_policy(Some(policy), DAY_MS + 2).unwrap();
        assert_eq!(history.preferences, Some(policy));
        assert_eq!(
            history
                .entries("")
                .unwrap()
                .iter()
                .map(|row| row.description.title.as_str())
                .collect::<Vec<_>>(),
            ["keep pinned", "newest"]
        );
        assert!(history.prune(2 * DAY_MS + 2).unwrap());
        assert_eq!(history.entries("").unwrap().len(), 1);
        history.clear().unwrap();
        assert!(history.entries("").unwrap().is_empty());
    }
}
