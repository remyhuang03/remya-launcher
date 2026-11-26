use rusqlite::{OptionalExtension, TransactionBehavior, params};
use url::Url;

use super::Index;
use super::store::visible_url_token;
use super::util::current_time_ms;

const MAX_ICONS_PER_PROFILE: i64 = 256;
const MAX_TOTAL_ICONS: i64 = 1024;
const REFRESH_AFTER_MS: i64 = 24 * 60 * 60 * 1000;
pub(crate) const MAX_FAVICON_PNG_BYTES: usize = 24 * 1024;

pub(crate) struct StoredFavicon {
    pub(crate) png: Vec<u8>,
    pub(crate) needs_refresh: bool,
}

impl Index {
    pub(crate) fn stored_favicon(
        &self,
        profile_id: &str,
        url: &str,
        consent_token: &str,
    ) -> rusqlite::Result<Option<StoredFavicon>> {
        self.conn
            .query_row(
                "SELECT f.png, f.fetched_ms
                 FROM web_favicons f
                 JOIN opt_ins o ON o.profile_id = f.profile_id AND o.enabled = 1
                 WHERE f.profile_id = ?1 AND f.normalized_url = ?2 AND o.consent_token = ?3
                   AND LENGTH(f.png) BETWEEN 1 AND ?4",
                params![profile_id, url, consent_token, MAX_FAVICON_PNG_BYTES as i64],
                |row| {
                    let fetched_ms: i64 = row.get(1)?;
                    let age = current_time_ms().saturating_sub(fetched_ms);
                    Ok(StoredFavicon {
                        png: row.get(0)?,
                        needs_refresh: !(0..REFRESH_AFTER_MS).contains(&age),
                    })
                },
            )
            .optional()
    }

    pub(crate) fn store_favicon(
        &mut self,
        profile_id: &str,
        url: &Url,
        consent_token: &str,
        png: &[u8],
    ) -> rusqlite::Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if visible_url_token(&tx, profile_id, url.as_str())?.as_deref() != Some(consent_token) {
            return Ok(());
        }
        tx.execute(
            "INSERT INTO web_favicons (profile_id, normalized_url, host, png, fetched_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(profile_id, normalized_url) DO UPDATE SET
               png = excluded.png, fetched_ms = excluded.fetched_ms",
            params![
                profile_id,
                url.as_str(),
                url.host_str().unwrap_or_default(),
                png,
                current_time_ms(),
            ],
        )?;
        tx.execute(
            "DELETE FROM web_favicons WHERE rowid IN (
               SELECT rowid FROM web_favicons WHERE profile_id = ?1
               ORDER BY fetched_ms DESC, rowid DESC LIMIT -1 OFFSET ?2
             )",
            params![profile_id, MAX_ICONS_PER_PROFILE],
        )?;
        tx.execute(
            "DELETE FROM web_favicons WHERE rowid IN (
               SELECT rowid FROM web_favicons
               ORDER BY fetched_ms DESC, rowid DESC LIMIT -1 OFFSET ?1
             )",
            [MAX_TOTAL_ICONS],
        )?;
        tx.commit()
    }
}
