use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{TrySendError, sync_channel};

use jiff::Timestamp;
use rusqlite::params;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::math::{self, Budget};
use crate::programmer::ProgrammerSettings;
use crate::protocol::{Candidate, Diagnostic, FRAME_LIMIT, Operation, Scope, VERSION, response};
use crate::storage::{self, Store, unavailable};
use crate::{discovery, float, fx, programmer, time, units};

struct Request {
    id: String,
    operation: Operation,
    fingerprint: String,
}

#[derive(Default)]
struct Session {
    preview: Option<Preview>,
    store: Option<Store>,
    opened_store: bool,
}

struct Preview {
    raw: String,
    revision: u64,
    candidates: Vec<Candidate>,
    ambiguous: bool,
}

impl Session {
    fn store(&mut self) -> Result<&mut Store, Diagnostic> {
        if !self.opened_store {
            self.opened_store = true;
            self.store = Store::open().ok();
        }
        self.store
            .as_mut()
            .ok_or_else(|| Diagnostic::error("storage_unavailable"))
    }

    fn handle(&mut self, request: Request, latest: &AtomicU64) -> Result<Value, Diagnostic> {
        let budget = Budget::new(request.operation.revision().unwrap_or(u64::MAX), latest);
        let fingerprint = request.fingerprint.as_str();
        match request.operation {
            Operation::Hello {} => Ok(json!({
                "protocolVersion":VERSION,"engine":"remya-calculator","engineVersion":env!("CARGO_PKG_VERSION"),
                "capabilities":["math","units","bases","catalog","completion","float","time","fx","history","variables","preferences"]
            })),
            Operation::Status {} => {
                let db = self.store().ok().map(|store| store.connection());
                let status = if db.is_some() { "ready" } else { "degraded" };
                let fx = if db.is_some_and(|db| {
                    db.query_row("SELECT 1 FROM fx_snapshots LIMIT 1", [], |_| Ok(()))
                        .is_ok()
                }) {
                    "cached"
                } else {
                    "rate_unavailable"
                };
                Ok(
                    json!({"status":status,"fx":fx,"programmer":get_pref(db, "programmer").unwrap_or_else(|| json!(ProgrammerSettings::default()))}),
                )
            }
            Operation::Shutdown {} | Operation::Cancel { .. } => Ok(json!({})),
            Operation::Preview {
                query,
                revision,
                scope,
                settings,
                cursor_utf16,
            } => self.preview(query, revision, scope, settings, cursor_utf16, &budget),
            Operation::CatalogSearch { query, limit, .. } => {
                math::normalize(&query, 4096)?;
                budget.check()?;
                Ok(json!({"entries":discovery::search(&query, limit.min(16))}))
            }
            Operation::Complete {
                query,
                cursor_utf16,
                ..
            } => {
                math::normalize(&query, 4096)?;
                budget.check()?;
                Ok(json!({"completions":discovery::complete(&query, cursor_utf16)}))
            }
            Operation::HistoryList { limit, before } => self
                .store()?
                .history(limit.unwrap_or(20).clamp(1, 50), before),
            Operation::VariablesList {} => {
                let entries: Vec<_> = storage::definitions(self.store()?.connection())?
                    .into_iter()
                    .map(|(name, expression)| json!({"name":name,"expression":expression}))
                    .collect();
                let mut size = 0;
                let entries: Vec<_> = entries
                    .into_iter()
                    .take_while(|entry| {
                        size += entry.to_string().len();
                        size < FRAME_LIMIT - 1024
                    })
                    .collect();
                Ok(json!({"entries":entries}))
            }
            Operation::Commit {
                candidate_id,
                revision,
                idempotency_key,
                format_id,
                remember_choice,
            } => self.commit(
                &candidate_id,
                revision,
                &idempotency_key,
                format_id.as_deref(),
                remember_choice,
                fingerprint,
                &budget,
            ),
            Operation::HistoryDelete {
                history_id,
                idempotency_key,
            } => {
                let id = history_id
                    .parse::<i64>()
                    .map_err(|_| Diagnostic::error("invalid_request"))?;
                self.store()?.mutate(&idempotency_key, fingerprint, |tx| {
                    let changed = tx
                        .execute("DELETE FROM history WHERE id=?1", [id])
                        .map_err(unavailable)?;
                    Ok(json!({"deleted":changed}))
                })
            }
            Operation::HistoryPin {
                history_id,
                pinned,
                idempotency_key,
            } => {
                let id = history_id
                    .parse::<i64>()
                    .map_err(|_| Diagnostic::error("invalid_request"))?;
                self.store()?.mutate(&idempotency_key, fingerprint, |tx| {
                    let changed = tx
                        .execute(
                            "UPDATE history SET pinned=?1 WHERE id=?2",
                            params![pinned, id],
                        )
                        .map_err(unavailable)?;
                    Ok(json!({"updated":changed}))
                })
            }
            Operation::HistoryClear {
                include_pinned,
                idempotency_key,
            } => self.store()?.mutate(&idempotency_key, fingerprint, |tx| {
                let changed = tx
                    .execute("DELETE FROM history WHERE ?1 OR pinned=0", [include_pinned])
                    .map_err(unavailable)?;
                Ok(json!({"deleted":changed}))
            }),
            Operation::SaveVariable {
                name,
                expression,
                idempotency_key,
            } => {
                storage::variable_name(&name)?;
                let expression = math::normalize(&expression, 512)?.trim().to_owned();
                self.store()?.mutate(&idempotency_key, fingerprint, |tx| {
                    let mut definitions = storage::definitions(tx)?;
                    definitions.insert(name.clone(), expression.clone());
                    if definitions.len() > 128 {
                        return Err(Diagnostic::error("resource_limit"));
                    }
                    storage::context(&definitions, &budget)?;
                    tx.execute(
                        "INSERT INTO variables(name,expression) VALUES(?1,?2) ON CONFLICT(name) DO UPDATE SET expression=excluded.expression",
                        params![name,expression],
                    ).map_err(unavailable)?;
                    Ok(json!({"variableId":name}))
                })
            }
            Operation::VariableDelete {
                name,
                idempotency_key,
            } => {
                storage::variable_name(&name)?;
                self.store()?.mutate(&idempotency_key, fingerprint, |tx| {
                    let changed = tx
                        .execute("DELETE FROM variables WHERE name=?1", [&name])
                        .map_err(unavailable)?;
                    let definitions = storage::definitions(tx)?;
                    storage::context(&definitions, &budget)?;
                    Ok(json!({"deleted":changed}))
                })
            }
            Operation::PreferencesSet {
                key,
                value,
                idempotency_key,
            } => {
                validate_preference(&key, &value)?;
                self.store()?.mutate(&idempotency_key, fingerprint, |tx| {
                    tx.execute(
                        "INSERT INTO preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                        params![key,value.to_string()],
                    ).map_err(unavailable)?;
                    Ok(json!({"key":key,"value":value}))
                })
            }
            Operation::FxRefresh { .. } => unreachable!("refresh runs on the network worker"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn preview(
        &mut self,
        raw: String,
        revision: u64,
        scope: Scope,
        settings: Option<ProgrammerSettings>,
        cursor_utf16: Option<usize>,
        budget: &Budget<'_>,
    ) -> Result<Value, Diagnostic> {
        budget.check()?;
        self.preview = None;
        let shadow = math::normalize(
            &raw,
            if matches!(scope, Scope::All) {
                512
            } else {
                4096
            },
        )?;
        let query = shadow.trim();
        if query.is_empty() || (matches!(scope, Scope::All) && !math::admits_all(query)) {
            return Ok(json!({"status":"empty","candidates":[],"completions":[],"diagnostics":[]}));
        }
        let calc = matches!(scope, Scope::Calc);
        let mut diagnostics = Vec::new();
        let mut completions = Vec::new();
        let mut catalog = Vec::new();
        let cursor = cursor_utf16.unwrap_or_else(|| raw.encode_utf16().count());
        if calc && query.split_whitespace().count() > 1 {
            if query
                .chars()
                .all(|c| c.is_alphabetic() || c.is_whitespace() || c == '-')
                && !matches!(
                    query.split_whitespace().next(),
                    Some("sqrt" | "sin" | "cos" | "tan" | "log" | "ln" | "abs" | "roll")
                )
            {
                let entries = discovery::search(query, 6);
                if entries.first().is_some_and(|entry| {
                    matches!(entry["scoreTier"].as_str(), Some("exact" | "exact_alias"))
                }) {
                    return Ok(
                        json!({"status":"catalog","candidates":[],"completions":[],"diagnostics":[],"catalogEntries":entries}),
                    );
                }
            }
            if let Some((quantity, token)) = query.rsplit_once(char::is_whitespace)
                && quantity.trim().parse::<f64>().is_ok()
                && token.chars().all(char::is_alphabetic)
            {
                let options: Vec<_> = discovery::complete(&raw, cursor)
                    .into_iter()
                    .filter(|entry| {
                        entry["kind"] == "unit"
                            && matches!(entry["scoreTier"].as_str(), Some("prefix" | "exact_alias"))
                    })
                    .collect();
                if options.len() > 1
                    && !options
                        .iter()
                        .any(|entry| entry["canonicalSymbol"] == token)
                {
                    return Ok(
                        json!({"status":"completion","candidates":[],"completions":options,"diagnostics":[]}),
                    );
                }
            }
        }
        let db = self.store().ok().map(|store| store.connection());
        let now = Timestamp::now();
        let pinned = get_pref(db, "pinnedZones")
            .and_then(|v| serde_json::from_value::<Vec<String>>(v).ok())
            .unwrap_or_else(|| {
                ["Asia/Tokyo", "Europe/London", "UTC"]
                    .into_iter()
                    .map(str::to_owned)
                    .collect()
            });
        let preferred =
            get_pref(db, "preferredCurrency").and_then(|v| v.as_str().map(str::to_owned));
        let snapshot: Option<fx::Snapshot> = db
            .and_then(|db| {
                db.query_row(
                    "SELECT snapshot FROM fx_snapshots ORDER BY rowid DESC LIMIT 1",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .ok()
            })
            .and_then(|s| serde_json::from_str(&s).ok())
            .filter(|snapshot: &fx::Snapshot| snapshot.freshness(now) != fx::Freshness::Invalid);
        let settings = settings
            .or_else(|| get_pref(db, "programmer").and_then(|v| serde_json::from_value(v).ok()))
            .unwrap_or_default();
        if !matches!(settings.bit_width, 8 | 16 | 32 | 64 | 128) {
            return Err(Diagnostic::error("invalid_request"));
        }
        let result = float::preview(query)
            .or_else(|| time::preview(query, now, &pinned))
            .or_else(|| fx::preview(query, now, snapshot.as_ref(), preferred.as_deref()))
            .or_else(|| units::preview(&shadow, &raw))
            .or_else(|| programmer::preview(query, settings))
            .unwrap_or_else(|| {
                let context =
                    match db
                        .map(storage::definitions)
                        .transpose()
                        .and_then(|definitions| {
                            storage::context(&definitions.unwrap_or_default(), budget)
                        }) {
                        Ok(context) => context,
                        Err(error) => {
                            diagnostics.push(error);
                            fend_core::Context::new()
                        }
                    };
                math::preview(query, &context, budget).map(|candidate| vec![candidate])
            });
        budget.check()?;
        let (mut candidates, mut status) = match result {
            Ok(mut values) => {
                values.truncate(16);
                let needs_choice = values.len() > 1;
                (
                    values,
                    if needs_choice {
                        "needs_disambiguation"
                    } else {
                        "ready"
                    },
                )
            }
            Err(error) if !calc => {
                let _ = error;
                (vec![], "empty")
            }
            Err(error) => {
                if matches!(
                    error.code.as_str(),
                    "unknown_identifier" | "invalid_syntax" | "incomplete_input"
                ) {
                    completions = discovery::complete(&raw, cursor);
                    if query.starts_with(char::is_alphabetic)
                        && query
                            .chars()
                            .all(|c| c.is_alphabetic() || c.is_whitespace() || c == '-')
                    {
                        catalog = discovery::search(query, 6);
                    }
                }
                let status = if !completions.is_empty() {
                    "completion"
                } else if !catalog.is_empty() {
                    "catalog"
                } else if error.code == "incomplete_input" {
                    "incomplete"
                } else {
                    "invalid"
                };
                if matches!(status, "invalid" | "incomplete") {
                    diagnostics.push(error);
                }
                (vec![], status)
            }
        };
        let alias = alias_key(&raw, &candidates);
        if let Some((alias, db)) = alias.as_ref().zip(db) {
            let preference: Option<String> = db
                .query_row(
                    "SELECT candidate FROM alias_choices WHERE alias=?1",
                    [alias],
                    |r| r.get(0),
                )
                .ok();
            if let Some(index) = candidates
                .iter()
                .position(|candidate| Some(&candidate.id) == preference.as_ref())
            {
                candidates[..=index].rotate_right(1);
            }
        }
        if candidates.is_empty() && status == "ready" {
            status = "empty";
        }
        for candidate in &mut candidates {
            candidate.formats.truncate(16);
            candidate.diagnostics.truncate(8);
        }
        let primary = candidates
            .first()
            .and_then(|candidate| (!candidate.formats.is_empty()).then_some(candidate.id.as_str()));
        let data = json!({
            "status":status,"primaryCandidateId":primary,"diagnostics":diagnostics,"candidates":candidates,
            "completions":completions,"catalogEntries":catalog,"actions":actions(&candidates, &diagnostics),
        });
        self.preview = Some(Preview {
            raw,
            revision,
            ambiguous: candidates.len() > 1,
            candidates,
        });
        Ok(data)
    }

    #[allow(clippy::too_many_arguments)]
    fn commit(
        &mut self,
        candidate_id: &str,
        revision: u64,
        token: &str,
        format_id: Option<&str>,
        remember_choice: bool,
        fingerprint: &str,
        budget: &Budget<'_>,
    ) -> Result<Value, Diagnostic> {
        if let Some(value) = self.store()?.receipt(token, fingerprint)? {
            return Ok(value);
        }
        let stale = || Diagnostic::error("stale_revision");
        budget.check().map_err(|_| stale())?;
        let preview = self
            .preview
            .as_ref()
            .filter(|p| p.revision == revision)
            .ok_or_else(stale)?;
        let candidate = preview
            .candidates
            .iter()
            .find(|candidate| candidate.id == candidate_id)
            .ok_or_else(|| Diagnostic::error("invalid_candidate"))?;
        let format = match format_id {
            Some(id) => candidate.formats.iter().find(|format| format.id == id),
            None => candidate.formats.first(),
        }
        .ok_or_else(|| Diagnostic::error("invalid_format"))?;
        let alias = if remember_choice && preview.ambiguous {
            alias_key(&preview.raw, &preview.candidates)
        } else {
            None
        };
        let query = preview.raw.clone();
        let payload = serde_json::to_string(candidate).map_err(unavailable)?;
        if payload.len() > 12000 {
            return Err(Diagnostic::error("resource_limit"));
        }
        let format_id = format.id.clone();
        let result =
            json!({"candidate":candidate,"formatId":format_id,"copyText":format.copy_text});
        let now = Timestamp::now();
        self.store()?.mutate(token, fingerprint, |tx| {
            budget.check().map_err(|_| stale())?;
            let history_enabled = pref(tx, "historyEnabled").and_then(|v| v.as_bool()).unwrap_or(true);
            let mut result = result;
            if history_enabled {
                tx.execute("INSERT INTO history(committed_at,query,candidate,format) VALUES(?1,?2,?3,?4)",
                    params![now.to_string(), query, payload, format_id]).map_err(unavailable)?;
                result["historyId"] = json!(tx.last_insert_rowid().to_string());
                let retention = pref(tx, "retentionDays").and_then(|v| v.as_u64()).unwrap_or(30);
                let cutoff = now.checked_sub(jiff::SignedDuration::from_hours(retention as i64 * 24)).map_err(unavailable)?;
                tx.execute(
                    "DELETE FROM history WHERE pinned=0 AND (julianday(committed_at)<julianday(?1)
                    OR id NOT IN (SELECT id FROM history WHERE pinned=0 ORDER BY id DESC LIMIT 1000))",
                    [cutoff.to_string()],
                ).map_err(unavailable)?;
            }
            if let Some(alias) = alias {
                tx.execute(
                    "INSERT INTO alias_choices(alias,candidate,usages) VALUES(?1,?2,1) ON CONFLICT(alias)
                    DO UPDATE SET candidate=excluded.candidate, usages=min(100,alias_choices.usages+1)",
                    params![alias,candidate_id],
                ).map_err(unavailable)?;
                tx.execute("DELETE FROM alias_choices WHERE rowid NOT IN (SELECT rowid FROM alias_choices ORDER BY usages DESC, rowid DESC LIMIT 256)", []).map_err(unavailable)?;
            }
            Ok(result)
        })
    }
}

fn refresh(session: &mut Session, token: &str, fingerprint: &str) -> Result<Value, Diagnostic> {
    let store = session.store()?;
    if let Some(result) = store.receipt(token, fingerprint)? {
        return Ok(result);
    }
    let now = Timestamp::now();
    let snapshot = fx::refresh(now)?;
    snapshot.validate(now)?;
    let serialized = serde_json::to_string(&snapshot).map_err(unavailable)?;
    let id = format!("{}-{}", snapshot.id, snapshot.fetched_at_utc);
    store.mutate(token, fingerprint, |tx| {
            tx.execute("INSERT OR IGNORE INTO fx_snapshots(id,snapshot) VALUES(?1,?2)", params![id,serialized]).map_err(unavailable)?;
            tx.execute("DELETE FROM fx_snapshots WHERE rowid NOT IN (SELECT rowid FROM fx_snapshots ORDER BY rowid DESC LIMIT 16)", []).map_err(unavailable)?;
            Ok(json!({"snapshotId":id,"status":"ready"}))
        })
}

fn alias_key(raw: &str, candidates: &[Candidate]) -> Option<String> {
    if candidates.len() < 2
        || !matches!(candidates[0].kind.as_str(), "unit_conversion" | "currency")
    {
        return None;
    }
    // Scope preferences to the exact written unit or currency pair, excluding its quantity.
    let key: String = raw
        .trim()
        .chars()
        .filter(|c| !c.is_ascii_digit() && *c != '.')
        .collect();
    (key.len() <= 128).then(|| format!("{}:{}", candidates[0].kind, key.trim()))
}

fn actions(candidates: &[Candidate], diagnostics: &[Diagnostic]) -> Vec<Value> {
    let mut actions = vec![json!({"id":"edit_query"}), json!({"id":"copy_expression"})];
    if candidates
        .iter()
        .any(|candidate| !candidate.formats.is_empty())
    {
        actions.push(json!({"id":"copy_selected_format"}));
    }
    if candidates
        .iter()
        .any(|candidate| candidate.kind == "currency")
        || diagnostics
            .iter()
            .any(|d| matches!(d.code.as_str(), "rate_unavailable" | "rate_stale"))
    {
        actions.push(json!({"id":"refresh_rates"}));
    }
    if candidates.len() > 1 {
        actions.push(json!({"id":"choose_candidate"}));
    }
    actions
}

fn pref(db: &rusqlite::Connection, key: &str) -> Option<Value> {
    let text: String = db
        .query_row("SELECT value FROM preferences WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .ok()?;
    let value = serde_json::from_str(&text).ok()?;
    validate_preference(key, &value).ok()?;
    Some(value)
}

fn get_pref(db: Option<&rusqlite::Connection>, key: &str) -> Option<Value> {
    db.and_then(|db| pref(db, key))
}

fn validate_preference(key: &str, value: &Value) -> Result<(), Diagnostic> {
    let valid = match key {
        "historyEnabled" => value.is_boolean(),
        "retentionDays" => value.as_u64().is_some_and(|days| (1..=365).contains(&days)),
        "preferredCurrency" => {
            value.is_null()
                || matches!(
                    value.as_str(),
                    Some("CNY" | "JPY" | "USD" | "CAD" | "AUD" | "NZD" | "HKD" | "SGD")
                )
        }
        "programmer" => serde_json::from_value::<ProgrammerSettings>(value.clone())
            .is_ok_and(|settings| matches!(settings.bit_width, 8 | 16 | 32 | 64 | 128)),
        "pinnedZones" => value.as_array().is_some_and(|zones| {
            zones.len() <= 8
                && zones.iter().all(|zone| {
                    zone.as_str()
                        .is_some_and(|zone| zone.len() <= 64 && jiff::tz::db().get(zone).is_ok())
                })
        }),
        _ => false,
    };
    if !valid {
        return Err(Diagnostic::error("invalid_preference"));
    }
    Ok(())
}

fn write_response(
    output: &Mutex<impl Write>,
    id: Option<&str>,
    revision: Option<u64>,
    result: Result<Value, Diagnostic>,
) -> io::Result<()> {
    let result = response(id, revision, result);
    let mut frame = serde_json::to_vec(&result)?;
    if frame.len() >= FRAME_LIMIT {
        frame = serde_json::to_vec(&response(
            id,
            revision,
            Err(Diagnostic::error("resource_limit")),
        ))?;
    }
    frame.push(b'\n');
    let mut output = output
        .lock()
        .map_err(|_| io::Error::other("calculator output unavailable"))?;
    output.write_all(&frame)?;
    output.flush()
}

fn read_frame(input: &mut impl BufRead, frame: &mut Vec<u8>) -> io::Result<Option<bool>> {
    frame.clear();
    let mut oversized = false;
    loop {
        let chunk = input.fill_buf()?;
        if chunk.is_empty() {
            return Ok((oversized || !frame.is_empty()).then_some(false));
        }
        let end = chunk.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(chunk.len(), |index| index + 1);
        if !oversized && frame.len() + count <= FRAME_LIMIT {
            frame.extend_from_slice(&chunk[..count]);
        } else {
            oversized = true;
        }
        input.consume(count);
        if end.is_some() {
            return Ok(Some(!oversized));
        }
    }
}

pub fn serve(input: impl Read, output: impl Write + Send) -> io::Result<()> {
    let latest = AtomicU64::new(0);
    let output = Mutex::new(output);
    let mut input = BufReader::new(input);
    std::thread::scope(|scope| {
        let (sender, receiver) = sync_channel::<Request>(32);
        let output_ref = &output;
        let latest_ref = &latest;
        let worker = scope.spawn(move || {
            let mut session = Session::default();
            while let Ok(request) = receiver.recv() {
                let Request {
                    ref id,
                    ref operation,
                    ..
                } = request;
                let revision = operation.revision();
                let shutdown = matches!(operation, Operation::Shutdown {});
                let id = id.clone();
                write_response(
                    output_ref,
                    Some(&id),
                    revision,
                    session.handle(request, latest_ref),
                )?;
                if shutdown {
                    break;
                }
            }
            Ok::<_, io::Error>(())
        });
        let (refresh_sender, refresh_receiver) = sync_channel::<Request>(1);
        let network = scope.spawn(move || {
            let mut session = Session::default();
            while let Ok(request) = refresh_receiver.recv() {
                let Operation::FxRefresh { idempotency_key } = request.operation else {
                    unreachable!("network worker accepts only refresh");
                };
                let result = refresh(&mut session, &idempotency_key, &request.fingerprint);
                write_response(output_ref, Some(&request.id), None, result)?;
            }
            Ok::<_, io::Error>(())
        });
        let mut seen = HashSet::new();
        let mut frame = Vec::new();
        let read = (|| {
            let mut has_hello = false;
            let mut preview_revision = None;
            while let Some(valid) = read_frame(&mut input, &mut frame)? {
                if !valid {
                    write_response(&output, None, None, Err(Diagnostic::error("invalid_frame")))?;
                    continue;
                }
                let mut fields =
                    match serde_json::from_slice::<serde_json::Map<String, Value>>(&frame) {
                        Ok(fields) => fields,
                        Err(_) => {
                            write_response(
                                &output,
                                None,
                                None,
                                Err(Diagnostic::error("invalid_frame")),
                            )?;
                            continue;
                        }
                    };
                let id = fields
                    .remove("id")
                    .and_then(|value| value.as_str().map(str::to_owned))
                    .filter(|id| {
                        !id.is_empty()
                            && id.len() <= 64
                            && id
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"_-.".contains(&b))
                    });
                let version = fields.remove("v");
                let (Some(id), Some(version)) = (id, version) else {
                    write_response(
                        &output,
                        None,
                        None,
                        Err(Diagnostic::error("invalid_request")),
                    )?;
                    continue;
                };
                let revision = fields.get("revision").and_then(Value::as_u64);
                let error = |code| {
                    write_response(&output, Some(&id), revision, Err(Diagnostic::error(code)))
                };
                if version.as_u64() != Some(u64::from(VERSION)) {
                    error("unsupported_version")?;
                    continue;
                }
                if seen.len() >= 65_536 {
                    error("resource_limit")?;
                    break;
                }
                if !seen.insert(id.clone()) {
                    error("duplicate_id")?;
                    continue;
                }
                let fingerprint = format!(
                    "{:x}",
                    num_bigint::BigUint::from_bytes_be(&Sha256::digest(
                        Value::Object(fields.clone()).to_string()
                    ),)
                );
                let operation: Operation = match serde_json::from_value(Value::Object(fields)) {
                    Ok(operation) => operation,
                    Err(_) => {
                        error("invalid_request")?;
                        continue;
                    }
                };
                if !has_hello && !matches!(operation, Operation::Hello {} | Operation::Shutdown {})
                {
                    error("handshake_required")?;
                    continue;
                }
                has_hello = true;
                if let Operation::Preview { revision, .. } = operation {
                    if preview_revision.is_some_and(|previous| revision <= previous) {
                        error("stale_revision")?;
                        continue;
                    }
                    preview_revision = Some(revision);
                    latest.fetch_max(revision, Ordering::AcqRel);
                } else if let Operation::Cancel { revision } = operation {
                    latest.fetch_max(revision, Ordering::AcqRel);
                    write_response(&output, Some(&id), Some(revision), Ok(json!({})))?;
                    continue;
                }
                let shutdown = matches!(operation, Operation::Shutdown {});
                let request = Request {
                    id: id.clone(),
                    operation,
                    fingerprint,
                };
                if shutdown {
                    sender
                        .send(request)
                        .map_err(|_| io::Error::other("calculator worker stopped"))?;
                    break;
                }
                let queue = if matches!(request.operation, Operation::FxRefresh { .. }) {
                    &refresh_sender
                } else {
                    &sender
                };
                match queue.try_send(request) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => error("resource_limit")?,
                    Err(TrySendError::Disconnected(_)) => break,
                }
            }
            Ok::<_, io::Error>(())
        })();
        drop(sender);
        drop(refresh_sender);
        let network_result = network
            .join()
            .map_err(|_| io::Error::other("calculator network worker stopped"))?;
        let result = worker
            .join()
            .map_err(|_| io::Error::other("calculator worker stopped"))?;
        read.and(result).and(network_result)
    })
}
