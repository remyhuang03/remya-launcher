use std::collections::HashSet;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use rusqlite::{Connection, ErrorCode, OptionalExtension, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use url::Url;

use crate::browsers::{Entry, Profile};

use super::Index;
use super::MAX_FREQUENT_SITES;
use super::dto::EntryKind;
use super::engines::{DEFAULT_ENGINE_ID, is_supported_default_engine, validate_url};
use super::schema::{init_schema, purge_profile_data};
use super::util::{current_time_ms, sanitize_text};

const BUSY_TIMEOUT: Duration = Duration::from_millis(500);
pub(super) const MAX_LAUNCHER_OPENS: i64 = 2048;
pub(super) const OPEN_RETENTION_MS: i64 = 90 * 86_400_000;

impl Index {
    /// Opens or initializes the private launcher SQLite index at `path`.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA secure_delete = ON;",
        )?;
        enable_wal(&conn)?;
        conn.busy_timeout(BUSY_TIMEOUT)?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        init_schema(&tx)?;
        tx.commit()?;

        Ok(Self { conn })
    }

    /// Returns the set of currently opted-in (enabled) profile IDs.
    pub fn enabled_ids(&self) -> rusqlite::Result<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT profile_id FROM opt_ins WHERE enabled = 1")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut ids = HashSet::new();
        for id in rows {
            ids.insert(id?);
        }
        Ok(ids)
    }

    /// Enables or disables a profile atomically; disabling purges source rows, opens, pins, and hidden rules.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> rusqlite::Result<()> {
        if id.trim().is_empty() {
            return Ok(());
        }
        let now_ms = current_time_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if enabled {
            tx.execute(
                "INSERT INTO opt_ins (profile_id, enabled, updated_ms, consent_token)
                 VALUES (?1, 1, ?2, lower(hex(randomblob(16))))
                 ON CONFLICT(profile_id) DO UPDATE SET
                   enabled = 1,
                   updated_ms = excluded.updated_ms,
                   consent_token = CASE
                     WHEN opt_ins.enabled = 0 OR opt_ins.consent_token IS NULL
                     THEN excluded.consent_token
                     ELSE opt_ins.consent_token
                   END",
                params![id, now_ms],
            )?;
        } else {
            tx.execute(
                "INSERT INTO opt_ins (profile_id, enabled, updated_ms, consent_token)
                 VALUES (?1, 0, ?2, NULL)
                 ON CONFLICT(profile_id) DO UPDATE SET
                   enabled = 0,
                   consent_token = NULL,
                   updated_ms = excluded.updated_ms",
                params![id, now_ms],
            )?;
            purge_profile_data(&tx, id)?;
        }
        tx.commit()
    }

    /// Returns the persisted opt-in consent token if `profile_id` is currently enabled.
    pub fn consent_token(&self, profile_id: &str) -> rusqlite::Result<Option<String>> {
        if profile_id.trim().is_empty() {
            return Ok(None);
        }
        let token: Option<Option<String>> = self
            .conn
            .query_row(
                "SELECT consent_token FROM opt_ins WHERE profile_id = ?1 AND enabled = 1",
                params![profile_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(token.flatten())
    }

    /// Atomically reconciles a profile's projection snapshot if the profile is still opted in.
    pub fn replace(&mut self, profile: &Profile, entries: Vec<Entry>) -> rusqlite::Result<()> {
        let Some(token) = self.consent_token(&profile.id)? else {
            return Ok(());
        };
        let _ = self.replace_if_current(profile, entries, &token)?;
        Ok(())
    }

    /// Atomically reconciles a profile snapshot iff `token` matches the active consent token.
    pub fn replace_if_current(
        &mut self,
        profile: &Profile,
        entries: Vec<Entry>,
        token: &str,
    ) -> rusqlite::Result<bool> {
        if token.trim().is_empty() {
            return Ok(false);
        }
        let now_ms = current_time_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let current: Option<(i64, Option<String>)> = tx
            .query_row(
                "SELECT enabled, consent_token FROM opt_ins WHERE profile_id = ?1",
                params![&profile.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let is_current = matches!(
            current,
            Some((1, Some(ref current_token))) if current_token == token
        );
        if !is_current {
            return Ok(false);
        }

        tx.execute(
            "UPDATE opt_ins
             SET browser_id = ?2,
                 browser_label = ?3,
                 profile_label = ?4,
                 updated_ms = ?5
             WHERE profile_id = ?1",
            params![
                &profile.id,
                &profile.browser_id,
                &profile.browser_label,
                &profile.label,
                now_ms
            ],
        )?;

        tx.execute(
            "INSERT INTO sync_generations (profile_id, generation)
             VALUES (?1, 1)
             ON CONFLICT(profile_id) DO UPDATE SET generation = generation + 1",
            params![&profile.id],
        )?;
        let generation: i64 = tx.query_row(
            "SELECT generation FROM sync_generations WHERE profile_id = ?1",
            params![&profile.id],
            |row| row.get(0),
        )?;

        {
            let mut upsert = tx.prepare(
                "INSERT INTO entries (
                   entry_id, profile_id, browser_id, browser_label, profile_label,
                   kind, source_id, raw_url, normalized_url, host, origin,
                   title, folder_path_json, folder_path_text,
                   visited_count, last_visited_ms, seen_generation
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5,
                   ?6, ?7, ?8, ?9, ?10, ?11,
                   ?12, ?13, ?14,
                   ?15, ?16, ?17
                 )
                 ON CONFLICT(profile_id, kind, source_id) DO UPDATE SET
                   browser_id = excluded.browser_id,
                   browser_label = excluded.browser_label,
                   profile_label = excluded.profile_label,
                   raw_url = excluded.raw_url,
                   normalized_url = excluded.normalized_url,
                   host = excluded.host,
                   origin = excluded.origin,
                   title = excluded.title,
                   folder_path_json = excluded.folder_path_json,
                   folder_path_text = excluded.folder_path_text,
                   visited_count = excluded.visited_count,
                   last_visited_ms = excluded.last_visited_ms,
                   seen_generation = excluded.seen_generation",
            )?;

            for entry in entries {
                let Some(parsed) = validate_url(&entry.url) else {
                    continue;
                };
                let kind = if entry.bookmark {
                    EntryKind::Bookmark
                } else {
                    EntryKind::History
                };
                let entry_id = compute_entry_id(&profile.id, kind, &entry.source_id);
                let normalized_url = parsed.as_str();
                let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
                let origin = parsed.origin().ascii_serialization();
                let title = sanitize_text(&entry.title);
                let folder_clean: Vec<String> = entry
                    .folder_path
                    .iter()
                    .map(|segment| sanitize_text(segment))
                    .filter(|segment| !segment.is_empty())
                    .collect();
                let folder_json =
                    serde_json::to_string(&folder_clean).unwrap_or_else(|_| "[]".to_owned());
                let folder_text = folder_clean.join(" / ");
                let visited_count = entry.visited_count.max(0);

                upsert.execute(params![
                    entry_id,
                    &profile.id,
                    &profile.browser_id,
                    &profile.browser_label,
                    &profile.label,
                    kind.as_str(),
                    &entry.source_id,
                    &entry.url,
                    normalized_url,
                    host,
                    origin,
                    title,
                    folder_json,
                    folder_text,
                    visited_count,
                    entry.last_visited_ms,
                    generation,
                ])?;
            }
        }

        tx.execute(
            "DELETE FROM entries WHERE profile_id = ?1 AND seen_generation <> ?2",
            params![&profile.id, generation],
        )?;

        tx.commit()?;
        Ok(true)
    }

    /// Resolves an opaque `entryId` to `(normalized_url, profile_id)` if enabled and unhidden.
    pub fn entry(&self, id: &str) -> rusqlite::Result<Option<(String, String)>> {
        if id.trim().is_empty() {
            return Ok(None);
        }
        self.conn
            .query_row(
                "SELECT e.normalized_url, e.profile_id
                 FROM entries e
                 JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
                 WHERE e.entry_id = ?1
                   AND NOT EXISTS (
                     SELECT 1 FROM hidden h
                     WHERE h.profile_id = e.profile_id
                       AND ((h.scope = 'url' AND h.target = e.normalized_url)
                         OR (h.scope = 'host' AND h.target = e.host))
                   )",
                params![id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
    }

    /// Hides a page or site in Launcher across all enabled profiles and suppresses its open records.
    pub fn hide(&mut self, id: &str, site: bool) -> rusqlite::Result<usize> {
        if id.trim().is_empty() {
            return Ok(0);
        }
        let now_ms = current_time_ms();
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        let target: Option<(String, String)> = tx
            .query_row(
                "SELECT e.normalized_url, e.host
                 FROM entries e
                 JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
                 WHERE e.entry_id = ?1
                   AND NOT EXISTS (
                     SELECT 1 FROM hidden h
                     WHERE h.profile_id = e.profile_id
                       AND ((h.scope = 'url' AND h.target = e.normalized_url)
                         OR (h.scope = 'host' AND h.target = e.host))
                   )",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((normalized_url, host)) = target else {
            return Ok(0);
        };

        let (scope, target_key) = if site {
            ("host", host.as_str())
        } else {
            ("url", normalized_url.as_str())
        };

        let affected_count: usize = if site {
            tx.query_row(
                "SELECT COUNT(DISTINCT e.profile_id)
                 FROM entries e
                 JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
                 WHERE e.host = ?1
                   AND NOT EXISTS (
                     SELECT 1 FROM hidden h
                     WHERE h.profile_id = e.profile_id
                       AND ((h.scope = 'url' AND h.target = e.normalized_url)
                         OR (h.scope = 'host' AND h.target = e.host))
                   )",
                params![target_key],
                |row| row.get::<_, i64>(0),
            )? as usize
        } else {
            tx.query_row(
                "SELECT COUNT(DISTINCT e.profile_id)
                 FROM entries e
                 JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
                 WHERE e.normalized_url = ?1
                   AND NOT EXISTS (
                     SELECT 1 FROM hidden h
                     WHERE h.profile_id = e.profile_id
                       AND ((h.scope = 'url' AND h.target = e.normalized_url)
                         OR (h.scope = 'host' AND h.target = e.host))
                   )",
                params![target_key],
                |row| row.get::<_, i64>(0),
            )? as usize
        };

        tx.execute(
            "INSERT INTO hidden (profile_id, scope, target, hidden_ms)
             SELECT profile_id, ?1, ?2, ?3
             FROM opt_ins
             WHERE enabled = 1
             ON CONFLICT(profile_id, scope, target) DO UPDATE SET
               hidden_ms = excluded.hidden_ms",
            params![scope, target_key, now_ms],
        )?;

        if site {
            tx.execute(
                "DELETE FROM pins
                 WHERE entry_id IN (SELECT entry_id FROM entries WHERE host = ?1)",
                params![target_key],
            )?;
            tx.execute(
                "DELETE FROM launcher_opens
                 WHERE host = ?1
                   AND (profile_id IS NULL OR profile_id IN (SELECT profile_id FROM opt_ins WHERE enabled = 1))",
                params![target_key],
            )?;
        } else {
            tx.execute(
                "DELETE FROM pins
                 WHERE entry_id IN (SELECT entry_id FROM entries WHERE normalized_url = ?1)",
                params![target_key],
            )?;
            tx.execute(
                "DELETE FROM launcher_opens
                 WHERE normalized_url = ?1
                   AND (profile_id IS NULL OR profile_id IN (SELECT profile_id FROM opt_ins WHERE enabled = 1))",
                params![target_key],
            )?;
        }

        tx.commit()?;
        Ok(affected_count.max(1))
    }

    /// Returns whether `entry_id` is a visible source entry pinned to Launcher (deduped by URL).
    pub fn is_pinned(&self, entry_id: &str) -> rusqlite::Result<bool> {
        let Some(url) = visible_entry_url(&self.conn, entry_id)? else {
            return Ok(false);
        };
        self.conn.query_row(
            "SELECT EXISTS(
               SELECT 1
               FROM pins p
               JOIN entries e ON e.entry_id = p.entry_id
               JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
               WHERE e.normalized_url = ?1
                 AND NOT EXISTS (
                   SELECT 1 FROM hidden h
                   WHERE h.profile_id = e.profile_id
                     AND ((h.scope = 'url' AND h.target = e.normalized_url)
                       OR (h.scope = 'host' AND h.target = e.host))
                 )
             )",
            params![url],
            |row| row.get(0),
        )
    }

    /// Toggles pin state for a visible `entry_id`, deduplicating by URL and capping at 6 pins.
    pub fn toggle_pin(&mut self, entry_id: &str) -> rusqlite::Result<Option<bool>> {
        if entry_id.trim().is_empty() {
            return Ok(None);
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(url) = visible_entry_url(&tx, entry_id)? else {
            return Ok(None);
        };

        let existing_pins: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT p.entry_id
                 FROM pins p
                 JOIN entries e ON e.entry_id = p.entry_id
                 JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
                 WHERE e.normalized_url = ?1
                   AND NOT EXISTS (
                     SELECT 1 FROM hidden h
                     WHERE h.profile_id = e.profile_id
                       AND ((h.scope = 'url' AND h.target = e.normalized_url)
                         OR (h.scope = 'host' AND h.target = e.host))
                   )",
            )?;
            let rows = stmt.query_map(params![&url], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };

        if !existing_pins.is_empty() {
            for pinned_id in existing_pins {
                tx.execute("DELETE FROM pins WHERE entry_id = ?1", params![pinned_id])?;
            }
            tx.commit()?;
            return Ok(Some(false));
        }

        let pinned_url_count: usize = tx.query_row(
            "SELECT COUNT(DISTINCT e.normalized_url)
             FROM pins p
             JOIN entries e ON e.entry_id = p.entry_id
             JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
             WHERE NOT EXISTS (
               SELECT 1 FROM hidden h
               WHERE h.profile_id = e.profile_id
                 AND ((h.scope = 'url' AND h.target = e.normalized_url)
                   OR (h.scope = 'host' AND h.target = e.host))
             )",
            [],
            |row| row.get::<_, i64>(0),
        )? as usize;

        if pinned_url_count >= MAX_FREQUENT_SITES {
            return Ok(Some(false));
        }

        tx.execute("INSERT INTO pins (entry_id) VALUES (?1)", params![entry_id])?;
        tx.commit()?;
        Ok(Some(true))
    }

    /// Records a non-private launcher open deduplicated by `event_id` with age/count caps.
    pub fn record_open(
        &mut self,
        url: &Url,
        profile_id: Option<&str>,
        event_id: &str,
    ) -> rusqlite::Result<()> {
        self.record_open_at(url, profile_id, event_id, current_time_ms())
    }

    pub(super) fn record_open_at(
        &mut self,
        url: &Url,
        profile_id: Option<&str>,
        event_id: &str,
        now_ms: i64,
    ) -> rusqlite::Result<()> {
        let Some(validated) = validate_url(url.as_str()) else {
            return Ok(());
        };
        let event_id = event_id.trim();
        if event_id.is_empty() || event_id.len() > 128 {
            return Ok(());
        }

        let normalized_url = validated.as_str();
        let host = validated
            .host_str()
            .unwrap_or_default()
            .to_ascii_lowercase();

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;

        if let Some(pid) = profile_id {
            let enabled: Option<i64> = tx
                .query_row(
                    "SELECT enabled FROM opt_ins WHERE profile_id = ?1",
                    params![pid],
                    |row| row.get(0),
                )
                .optional()?;
            if enabled != Some(1) {
                return Ok(());
            }

            let is_hidden: bool = tx.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM hidden
                   WHERE profile_id = ?1
                     AND ((scope = 'url' AND target = ?2) OR (scope = 'host' AND target = ?3))
                 )",
                params![pid, normalized_url, &host],
                |row| row.get(0),
            )?;
            if is_hidden {
                return Ok(());
            }
        } else {
            let is_hidden: bool = tx.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM hidden h
                   JOIN opt_ins o ON o.profile_id = h.profile_id AND o.enabled = 1
                   WHERE (h.scope = 'url' AND h.target = ?1) OR (h.scope = 'host' AND h.target = ?2)
                 )",
                params![normalized_url, &host],
                |row| row.get(0),
            )?;
            if is_hidden {
                return Ok(());
            }
        }

        tx.execute(
            "INSERT INTO launcher_opens (event_id, profile_id, normalized_url, host, opened_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(event_id) DO NOTHING",
            params![event_id, profile_id, normalized_url, host, now_ms],
        )?;

        let cutoff_ms = now_ms.saturating_sub(OPEN_RETENTION_MS);
        tx.execute(
            "DELETE FROM launcher_opens WHERE opened_ms < ?1",
            params![cutoff_ms],
        )?;
        tx.execute(
            "DELETE FROM launcher_opens
             WHERE event_id NOT IN (
               SELECT event_id FROM launcher_opens
               ORDER BY opened_ms DESC, rowid DESC
               LIMIT ?1
             )",
            params![MAX_LAUNCHER_OPENS],
        )?;

        tx.commit()
    }

    /// Opts out of all profiles and purges every table in the local index.
    pub fn clear(&mut self) -> rusqlite::Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("DELETE FROM pins", [])?;
        tx.execute("DELETE FROM entries", [])?;
        tx.execute("DELETE FROM launcher_opens", [])?;
        tx.execute("DELETE FROM hidden", [])?;
        tx.execute("DELETE FROM opt_ins", [])?;
        tx.execute("DELETE FROM sync_generations", [])?;
        tx.commit()
    }

    /// Reads the persisted default search engine ID, falling back to `duckduckgo`.
    pub fn default_engine_id(&self) -> rusqlite::Result<String> {
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT default_engine_id FROM web_preferences WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok(stored
            .filter(|id| is_supported_default_engine(id))
            .unwrap_or_else(|| DEFAULT_ENGINE_ID.to_owned()))
    }

    /// Persists the default search engine ID (`duckduckgo` or `google`).
    pub fn set_default_engine_id(&mut self, engine_id: &str) -> rusqlite::Result<()> {
        if !is_supported_default_engine(engine_id) {
            return Err(rusqlite::Error::InvalidParameterName(engine_id.to_owned()));
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "INSERT INTO web_preferences (id, default_engine_id)
             VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET default_engine_id = excluded.default_engine_id",
            params![engine_id],
        )?;
        tx.commit()
    }

    /// Returns true iff `url` is indexed under enabled profile `id` and is not hidden.
    pub fn has_url(&self, id: &str, url: &str) -> rusqlite::Result<bool> {
        if id.trim().is_empty() {
            return Ok(false);
        }
        let Some(parsed) = validate_url(url) else {
            return Ok(false);
        };
        let normalized_url = parsed.as_str();
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();

        self.conn.query_row(
            "SELECT EXISTS(
               SELECT 1
               FROM entries e
               JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
               WHERE e.profile_id = ?1
                 AND e.normalized_url = ?2
                 AND NOT EXISTS (
                   SELECT 1 FROM hidden h
                   WHERE h.profile_id = e.profile_id
                     AND ((h.scope = 'url' AND h.target = e.normalized_url)
                       OR (h.scope = 'host' AND h.target = ?3))
                 )
             )",
            params![id, normalized_url, host],
            |row| row.get(0),
        )
    }
}

fn enable_wal(conn: &Connection) -> rusqlite::Result<()> {
    let deadline = Instant::now() + BUSY_TIMEOUT;
    loop {
        conn.busy_timeout(deadline.saturating_duration_since(Instant::now()))?;
        let result = conn.pragma_update(None, "journal_mode", "WAL");
        if !result
            .as_ref()
            .is_err_and(|error| error.sqlite_error_code() == Some(ErrorCode::DatabaseBusy))
        {
            return result;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return result;
        }
        // SQLite can bypass its busy handler when upgrading the journal mode.
        thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}

fn visible_entry_url(conn: &Connection, entry_id: &str) -> rusqlite::Result<Option<String>> {
    if entry_id.trim().is_empty() {
        return Ok(None);
    }
    conn.query_row(
        "SELECT e.normalized_url
         FROM entries e
         JOIN opt_ins o ON o.profile_id = e.profile_id AND o.enabled = 1
         WHERE e.entry_id = ?1
           AND NOT EXISTS (
             SELECT 1 FROM hidden h
             WHERE h.profile_id = e.profile_id
               AND ((h.scope = 'url' AND h.target = e.normalized_url)
                 OR (h.scope = 'host' AND h.target = e.host))
           )",
        params![entry_id],
        |row| row.get(0),
    )
    .optional()
}

pub(super) fn compute_entry_id(profile_id: &str, kind: EntryKind, source_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"remya-web-entry-v1\0");
    hasher.update(profile_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(kind.as_str().as_bytes());
    hasher.update(b"\0");
    hasher.update(source_id.as_bytes());
    let digest = hasher.finalize();

    let mut out = String::with_capacity(4 + 32);
    out.push_str("web:");
    for byte in &digest[..16] {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}
