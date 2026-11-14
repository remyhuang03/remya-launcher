use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, DirBuilder, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::PathBuf;

use fend_core::{Context, evaluate_with_interrupt};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde_json::{Value, json};

use crate::math::{Budget, normalize};
use crate::protocol::{Candidate, Diagnostic};

pub(crate) type Definitions = BTreeMap<String, String>;

const MIGRATION: &str = "
CREATE TABLE history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    committed_at TEXT NOT NULL,
    query TEXT NOT NULL CHECK(length(query) <= 4096),
    candidate TEXT NOT NULL CHECK(length(candidate) <= 12000),
    format TEXT,
    pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0,1))
);
CREATE TABLE variables (
    name TEXT PRIMARY KEY CHECK(length(name) BETWEEN 1 AND 32),
    expression TEXT NOT NULL CHECK(length(expression) BETWEEN 1 AND 512)
);
CREATE TABLE preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE alias_choices (
    alias TEXT PRIMARY KEY,
    candidate TEXT NOT NULL,
    usages INTEGER NOT NULL CHECK(usages BETWEEN 1 AND 100)
);
CREATE TABLE fx_snapshots (
    id TEXT PRIMARY KEY,
    snapshot TEXT NOT NULL CHECK(length(snapshot) <= 32768)
);
CREATE TABLE receipts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    token TEXT UNIQUE NOT NULL,
    fingerprint TEXT NOT NULL,
    result TEXT NOT NULL
);
PRAGMA user_version = 1;
";

fn state_dir() -> io::Result<PathBuf> {
    let absolute = |key| {
        env::var_os(key)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let root = absolute("XDG_STATE_HOME")
        .or_else(|| absolute("HOME").map(|home| home.join(".local/state")))
        .ok_or(io::ErrorKind::NotFound)?;
    let path = root.join("remya/launcher");
    fs::create_dir_all(root.join("remya"))?;
    match DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(&path)?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(path)
}

pub(crate) fn unavailable<T>(_: T) -> Diagnostic {
    Diagnostic::error("storage_unavailable")
}

pub(crate) struct Store(Connection);

impl Store {
    pub(crate) fn open() -> Result<Self, Diagnostic> {
        let path = state_dir().map_err(unavailable)?.join("calculator.sqlite3");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(
                rustix::fs::OFlags::NOFOLLOW.bits() as i32
                    | rustix::fs::OFlags::NONBLOCK.bits() as i32,
            )
            .open(&path)
            .map_err(unavailable)?;
        let metadata = file.metadata().map_err(unavailable)?;
        if !metadata.is_file()
            || metadata.uid() != rustix::process::geteuid().as_raw()
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(Diagnostic::error("storage_unavailable"));
        }
        let db = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )
        .map_err(unavailable)?;
        Self::migrate(db).map_err(unavailable)
    }

    fn migrate(mut db: Connection) -> rusqlite::Result<Self> {
        db.busy_timeout(std::time::Duration::from_millis(200))?;
        db.set_prepared_statement_cache_capacity(16);
        db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON;")?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let version: u32 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;
        match version {
            0 => tx.execute_batch(MIGRATION)?,
            1 => {}
            _ => return Err(rusqlite::Error::InvalidQuery),
        }
        tx.commit()?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "max_page_count", 8192)?;
        Ok(Self(db))
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.0
    }

    pub(crate) fn receipt(
        &self,
        token: &str,
        fingerprint: &str,
    ) -> Result<Option<Value>, Diagnostic> {
        receipt(&self.0, token, fingerprint)
    }

    pub(crate) fn mutate(
        &mut self,
        token: &str,
        fingerprint: &str,
        apply: impl FnOnce(&Transaction<'_>) -> Result<Value, Diagnostic>,
    ) -> Result<Value, Diagnostic> {
        let tx = self
            .0
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(unavailable)?;
        if let Some(value) = receipt(&tx, token, fingerprint)? {
            return Ok(value);
        }
        let result = apply(&tx)?;
        tx.execute(
            "INSERT INTO receipts(token,fingerprint,result) VALUES(?1,?2,?3)",
            params![token, fingerprint, result.to_string()],
        )
        .map_err(unavailable)?;
        tx.execute(
            "DELETE FROM receipts WHERE id <= last_insert_rowid() - 2048",
            [],
        )
        .map_err(unavailable)?;
        tx.commit().map_err(unavailable)?;
        Ok(result)
    }

    pub(crate) fn history(&self, limit: u32, before: Option<u64>) -> Result<Value, Diagnostic> {
        let mut stmt = self
            .0
            .prepare(
                "SELECT id,committed_at,query,candidate,format,pinned FROM history
             WHERE (?1 IS NULL OR id < ?1) ORDER BY id DESC LIMIT ?2",
            )
            .map_err(unavailable)?;
        let mut rows = stmt
            .query(params![
                before.map(|id| id.try_into().unwrap_or(i64::MAX)),
                limit
            ])
            .map_err(unavailable)?;
        let mut entries = Vec::new();
        let mut total = 0;
        while let Some(row) = rows.next().map_err(unavailable)? {
            let id: i64 = row.get(0).map_err(unavailable)?;
            let committed_at: String = row.get(1).map_err(unavailable)?;
            let query: String = row.get(2).map_err(unavailable)?;
            let candidate_json: String = row.get(3).map_err(unavailable)?;
            let candidate: Candidate =
                serde_json::from_str(&candidate_json).map_err(unavailable)?;
            let format: Option<String> = row.get(4).map_err(unavailable)?;
            let pinned: bool = row.get(5).map_err(unavailable)?;
            let entry = json!({"historyId":id.to_string(),"committedAt":committed_at,"query":query,
                "candidate":candidate,"formatId":format,"pinned":pinned});
            total += entry.to_string().len();
            if total > 48 * 1024 {
                break;
            }
            entries.push(entry);
        }
        let next = entries
            .last()
            .and_then(|entry| entry["historyId"].as_str())
            .and_then(|id| id.parse::<u64>().ok());
        Ok(json!({"entries":entries,"nextBefore":next}))
    }
}

fn receipt(db: &Connection, token: &str, fingerprint: &str) -> Result<Option<Value>, Diagnostic> {
    if token.is_empty()
        || token.len() > 128
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(Diagnostic::error("invalid_request"));
    }
    let row: Option<(String, String)> = db
        .query_row(
            "SELECT fingerprint,result FROM receipts WHERE token=?1",
            [token],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(unavailable)?;
    row.map(|(stored, result)| {
        if stored != fingerprint {
            return Err(Diagnostic::error("idempotency_conflict"));
        }
        serde_json::from_str(&result).map_err(unavailable)
    })
    .transpose()
}

pub(crate) fn definitions(db: &Connection) -> Result<Definitions, Diagnostic> {
    let mut stmt = db
        .prepare("SELECT name,expression FROM variables ORDER BY name LIMIT 129")
        .map_err(unavailable)?;
    let definitions = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(unavailable)?
        .collect::<rusqlite::Result<Definitions>>()
        .map_err(unavailable)?;
    if definitions.len() > 128 {
        return Err(Diagnostic::error("resource_limit"));
    }
    Ok(definitions)
}

pub(crate) fn variable_name(name: &str) -> Result<(), Diagnostic> {
    let syntax = !name.is_empty()
        && name.len() <= 32
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if !syntax || crate::discovery::is_reserved(name) {
        return Err(Diagnostic::error("invalid_variable"));
    }
    let mut context = Context::new();
    if fend_core::evaluate(name, &mut context).is_ok() {
        return Err(Diagnostic::error("invalid_variable"));
    }
    Ok(())
}

pub(crate) fn context(
    definitions: &Definitions,
    budget: &Budget<'_>,
) -> Result<Context, Diagnostic> {
    fn visit<'a>(
        name: &'a str,
        definitions: &'a Definitions,
        current: &mut BTreeSet<&'a str>,
        loaded: &mut BTreeSet<&'a str>,
        context: &mut Context,
        budget: &Budget<'_>,
    ) -> Result<(), Diagnostic> {
        if loaded.contains(name) {
            return Ok(());
        }
        if !current.insert(name) {
            return Err(Diagnostic::error("variable_cycle"));
        }
        if current.len() > 16 {
            return Err(Diagnostic::error("resource_limit"));
        }
        let expression = normalize(&definitions[name], 512)?;
        if expression.trim().is_empty() || expression.contains(['=', ';', '@', '"', '\'', '\\']) {
            return Err(Diagnostic::error("invalid_variable"));
        }
        for word in expression.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
            if let Some((dependency, _)) = definitions.get_key_value(word) {
                visit(dependency, definitions, current, loaded, context, budget)?;
            }
        }
        budget.check()?;
        evaluate_with_interrupt(&format!("{name} = ({expression})"), context, budget).map_err(
            |_| {
                budget
                    .check()
                    .err()
                    .unwrap_or_else(|| Diagnostic::error("invalid_variable"))
            },
        )?;
        current.remove(name);
        loaded.insert(name);
        Ok(())
    }

    let mut context = Context::new();
    let mut current = BTreeSet::new();
    let mut loaded = BTreeSet::new();
    for name in definitions.keys() {
        variable_name(name)?;
        visit(
            name,
            definitions,
            &mut current,
            &mut loaded,
            &mut context,
            budget,
        )?;
    }
    Ok(context)
}
