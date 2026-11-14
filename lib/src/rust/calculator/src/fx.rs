//! Currency conversion adapter (P5) with exact decimal/rational arithmetic,
//! strict ECB daily XML provider validation, and offline snapshot freshness checks.
//!
//! Design invariants (spec §07, §10):
//! - `preview` NEVER performs network I/O; it evaluates only against an injected `Snapshot`.
//! - `refresh` fetches exclusively from the allowlisted HTTPS ECB daily feed using `ureq` 3
//!   with TLS required, redirects disabled, strict timeouts, and a bounded payload limit.
//! - Arithmetic uses exact `BigRational` (`num-rational` / `num-bigint`) with explicit
//!   half-up rounding for display — never binary `f64`.
//! - Ambiguous currency symbols (`¥`, `$`, `kr`) surface all candidate readings from the
//!   same immutable rate-table snapshot and disclose assumptions/diagnostics.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::Duration;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{Signed, Zero};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::protocol::{Candidate, Diagnostic, Format};

/// Allowlisted HTTPS endpoint for the European Central Bank daily reference rates XML.
pub(crate) const ECB_DAILY_XML_URL: &str =
    "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";

/// Stable provider identifier recorded in snapshots and provenance.
pub(crate) const ECB_PROVIDER_ID: &str = "ecb_eurofxref";

/// Human-readable provider name.
pub(crate) const ECB_PROVIDER_NAME: &str = "European Central Bank (ECB)";

/// Mandatory source attribution per ESCB statistics reuse policy.
pub(crate) const ECB_ATTRIBUTION: &str =
    "Source: European Central Bank, euro foreign exchange reference rates";

/// Terms summary recorded in candidate provenance.
const ECB_TERMS_SUMMARY: &str = "ECB source rates are attributed. Converted amounts and cross-rates are derived values. Reference rates are for information purposes.";

/// Default fresh window: 24 hours (ECB publishes once per TARGET working day ~16:00 CET).
pub(crate) const DEFAULT_TTL_SECONDS: u64 = 24 * 60 * 60;

/// Default maximum stale-usable window: 7 days (covers long TARGET holiday weekends + offline use).
pub(crate) const DEFAULT_MAX_STALE_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Upper bounds enforced by `Snapshot::validate` against tampered persisted snapshots.
const MAX_ALLOWED_TTL_SECONDS: u64 = 7 * 24 * 60 * 60;
const MAX_ALLOWED_STALE_SECONDS: u64 = 30 * 24 * 60 * 60;
const MAX_FUTURE_CLOCK_SKEW_SECONDS: i64 = 300;
const MAX_EFFECTIVE_FETCH_LEAD_SECONDS: i64 = 48 * 60 * 60;

/// Strict network and payload bounds for `refresh`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(10);
pub(crate) const MAX_PAYLOAD_BYTES: u64 = 64 * 1024;
const MAX_CURRENCIES_PER_SNAPSHOT: usize = 256;

/// Input and decimal bounds to prevent unbounded parsing/BigInt allocation.
const MAX_QUERY_BYTES: usize = 4096;
const MAX_RATE_STR_LEN: usize = 32;
const MAX_RATE_FRAC_DIGITS: usize = 18;
const MAX_INPUT_NUM_DIGITS: usize = 64;
const PRECISE_DECIMAL_SCALE: usize = 8;

/// Freshness classification of an FX snapshot at evaluation time `now`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Freshness {
    Fresh,
    StaleUsable,
    ExpiredBlocked,
    Invalid,
}

impl Freshness {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::StaleUsable => "stale_usable",
            Self::ExpiredBlocked => "expired_blocked",
            Self::Invalid => "invalid",
        }
    }
}

/// Immutable, validated rate-table snapshot relative to base currency `EUR`.
///
/// One snapshot serves all primary and alternative candidates of a preview query so
/// cross-rates and ambiguous interpretations never mix quotations across timestamps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub id: String,
    pub provider_id: String,
    pub provider_name: String,
    pub source_url: String,
    pub attribution: String,
    pub base_currency: String,
    pub effective_date: String,
    pub effective_at_utc: String,
    pub fetched_at_utc: String,
    pub ttl_seconds: u64,
    pub max_stale_seconds: u64,
    pub rates: BTreeMap<String, String>,
}

impl Snapshot {
    /// Validate structure, allowlisted provider metadata, positive finite decimal rates,
    /// timestamps, and expiry against `now`.
    ///
    /// Returns `Ok(())` when the snapshot is `Fresh` or `StaleUsable`.
    /// Returns `Err(Diagnostic)` when `Invalid` (`rate_unavailable`) or `ExpiredBlocked` (`rate_stale`).
    pub(crate) fn validate(&self, now: Timestamp) -> Result<(), Diagnostic> {
        self.validate_internal(now).map(|_| ())
    }

    /// Classify snapshot freshness against `now`.
    pub(crate) fn freshness(&self, now: Timestamp) -> Freshness {
        match self.validate_internal(now) {
            Ok((freshness, _)) => freshness,
            Err(err) => {
                if err.parameters.get("freshness").and_then(Value::as_str)
                    == Some("expired_blocked")
                {
                    Freshness::ExpiredBlocked
                } else {
                    Freshness::Invalid
                }
            }
        }
    }

    fn validate_internal(&self, now: Timestamp) -> Result<(Freshness, i64), Diagnostic> {
        if !is_safe_identifier(&self.id, 128) {
            return Err(invalid_snapshot_diag("invalid_id"));
        }
        if self.provider_id != ECB_PROVIDER_ID {
            return Err(invalid_snapshot_diag("untrusted_provider"));
        }
        if !is_safe_label(&self.provider_name, 128) {
            return Err(invalid_snapshot_diag("invalid_provider_name"));
        }
        if self.source_url != ECB_DAILY_XML_URL || !self.source_url.starts_with("https://") {
            return Err(invalid_snapshot_diag("untrusted_source_url"));
        }
        if !is_safe_label(&self.attribution, 256) {
            return Err(invalid_snapshot_diag("invalid_attribution"));
        }
        if self.base_currency != "EUR" {
            return Err(invalid_snapshot_diag("invalid_base_currency"));
        }

        let effective_date = parse_strict_iso_date(&self.effective_date)
            .ok_or_else(|| invalid_snapshot_diag("invalid_effective_date"))?;
        if effective_date < Date::constant(1999, 1, 1) {
            return Err(invalid_snapshot_diag("effective_date_out_of_range"));
        }

        let effective_at: Timestamp = self
            .effective_at_utc
            .parse()
            .map_err(|_| invalid_snapshot_diag("invalid_effective_timestamp"))?;
        let fetched_at: Timestamp = self
            .fetched_at_utc
            .parse()
            .map_err(|_| invalid_snapshot_diag("invalid_fetched_timestamp"))?;

        if effective_at.as_second()
            > fetched_at
                .as_second()
                .saturating_add(MAX_EFFECTIVE_FETCH_LEAD_SECONDS)
        {
            return Err(invalid_snapshot_diag("effective_after_fetch"));
        }

        let raw_age_secs = now.as_second().saturating_sub(fetched_at.as_second());
        if raw_age_secs < -MAX_FUTURE_CLOCK_SKEW_SECONDS {
            return Err(invalid_snapshot_diag("fetched_in_future"));
        }
        let age_secs = raw_age_secs.max(0);

        if self.ttl_seconds == 0 || self.ttl_seconds > MAX_ALLOWED_TTL_SECONDS {
            return Err(invalid_snapshot_diag("invalid_ttl"));
        }
        if self.max_stale_seconds < self.ttl_seconds
            || self.max_stale_seconds > MAX_ALLOWED_STALE_SECONDS
        {
            return Err(invalid_snapshot_diag("invalid_max_stale"));
        }

        if self.rates.len() < 2 || self.rates.len() > MAX_CURRENCIES_PER_SNAPSHOT {
            return Err(invalid_snapshot_diag("invalid_rate_table_size"));
        }
        let Some(eur_rate_str) = self.rates.get("EUR") else {
            return Err(invalid_snapshot_diag("missing_base_rate"));
        };
        let eur_rate = parse_positive_rate(eur_rate_str)
            .ok_or_else(|| invalid_snapshot_diag("invalid_base_rate"))?;
        if eur_rate != BigRational::from_integer(BigInt::from(1)) {
            return Err(invalid_snapshot_diag("non_unit_base_rate"));
        }

        for (code, rate_str) in &self.rates {
            if !is_iso_code(code) {
                return Err(invalid_snapshot_diag("invalid_currency_code"));
            }
            if parse_positive_rate(rate_str).is_none() {
                return Err(invalid_snapshot_diag("invalid_rate_value"));
            }
        }

        if age_secs > self.max_stale_seconds as i64 {
            let mut diag = Diagnostic::error("rate_stale");
            diag.parameters = json!({
                "freshness": "expired_blocked",
                "provider": self.provider_id,
                "rateId": self.id,
                "effectiveDate": self.effective_date,
                "fetchedAtUtc": self.fetched_at_utc,
                "ageSeconds": age_secs,
                "ttlSeconds": self.ttl_seconds,
                "maxStaleSeconds": self.max_stale_seconds,
            });
            return Err(diag);
        }

        let freshness = if age_secs <= self.ttl_seconds as i64 {
            Freshness::Fresh
        } else {
            Freshness::StaleUsable
        };
        Ok((freshness, age_secs))
    }
}

fn invalid_snapshot_diag(reason: &str) -> Diagnostic {
    let mut diag = Diagnostic::error("rate_unavailable");
    diag.parameters = json!({
        "reason": "invalid_snapshot",
        "detail": reason,
    });
    diag
}

/// Injectable FX XML feed provider for deterministic offline testing.
pub(crate) trait FxProvider {
    fn fetch_daily_xml(&self) -> Result<String, Diagnostic>;
}

/// Production ECB daily euro reference rates provider over HTTPS (`ureq` 3).
pub(crate) struct EcbDailyXmlProvider;

impl FxProvider for EcbDailyXmlProvider {
    fn fetch_daily_xml(&self) -> Result<String, Diagnostic> {
        fetch_allowlisted_ecb_xml(ECB_DAILY_XML_URL)
    }
}

fn fetch_allowlisted_ecb_xml(url: &str) -> Result<String, Diagnostic> {
    if url != ECB_DAILY_XML_URL || !url.starts_with("https://") {
        let mut diag = Diagnostic::error("rate_unavailable");
        diag.parameters = json!({
            "reason": "untrusted_endpoint",
            "provider": ECB_PROVIDER_ID,
        });
        return Err(diag);
    }

    let config = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .user_agent("remya-launcher-calculator/0.1")
        .build();
    let agent: ureq::Agent = config.into();

    let mut response = agent.get(url).call().map_err(map_ureq_error)?;

    if let Some(len_header) = response.headers().get("content-length")
        && let Ok(len_str) = len_header.to_str()
        && let Ok(declared_len) = len_str.trim().parse::<u64>()
        && declared_len > MAX_PAYLOAD_BYTES
    {
        let mut diag = Diagnostic::error("rate_unavailable");
        diag.parameters = json!({
            "reason": "payload_too_large",
            "provider": ECB_PROVIDER_ID,
            "limitBytes": MAX_PAYLOAD_BYTES,
            "declaredBytes": declared_len,
        });
        return Err(diag);
    }

    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_PAYLOAD_BYTES + 1)
        .read_to_vec()
        .map_err(map_ureq_error)?;

    if (bytes.len() as u64) > MAX_PAYLOAD_BYTES {
        let mut diag = Diagnostic::error("rate_unavailable");
        diag.parameters = json!({
            "reason": "payload_too_large",
            "provider": ECB_PROVIDER_ID,
            "limitBytes": MAX_PAYLOAD_BYTES,
        });
        return Err(diag);
    }

    String::from_utf8(bytes).map_err(|_| {
        let mut diag = Diagnostic::error("rate_unavailable");
        diag.parameters = json!({
            "reason": "invalid_utf8_payload",
            "provider": ECB_PROVIDER_ID,
        });
        diag
    })
}

fn map_ureq_error(err: ureq::Error) -> Diagnostic {
    let reason = match &err {
        ureq::Error::Timeout(_) => "timeout",
        ureq::Error::BodyExceedsLimit(_) => "payload_too_large",
        ureq::Error::StatusCode(_) => "http_status_error",
        _ => "network_error",
    };
    let mut diag = Diagnostic::error("rate_unavailable");
    diag.parameters = json!({
        "reason": reason,
        "provider": ECB_PROVIDER_ID,
        "detail": err.to_string(),
    });
    diag
}

/// Explicitly fetch and validate a fresh ECB rate snapshot (never called from `preview`).
pub(crate) fn refresh(now: Timestamp) -> Result<Snapshot, Diagnostic> {
    refresh_with(&EcbDailyXmlProvider, now)
}

/// Fetch and validate a fresh snapshot using an injected `FxProvider`.
pub(crate) fn refresh_with<P: FxProvider>(
    provider: &P,
    now: Timestamp,
) -> Result<Snapshot, Diagnostic> {
    let xml = provider.fetch_daily_xml()?;
    parse_ecb_daily_xml(&xml, now)
}

/// Parse and strictly validate an ECB `eurofxref-daily.xml` payload into an immutable `Snapshot`.
pub(crate) fn parse_ecb_daily_xml(xml: &str, now: Timestamp) -> Result<Snapshot, Diagnostic> {
    if (xml.len() as u64) > MAX_PAYLOAD_BYTES {
        let mut diag = Diagnostic::error("rate_unavailable");
        diag.parameters = json!({
            "reason": "payload_too_large",
            "provider": ECB_PROVIDER_ID,
            "limitBytes": MAX_PAYLOAD_BYTES,
        });
        return Err(diag);
    }
    use quick_xml::{Reader, events::Event};

    let mut reader = Reader::from_str(xml);
    let mut depth = 0usize;
    let mut dated_cube = None;
    let mut effective_date = None;
    let mut rates = BTreeMap::new();
    loop {
        let event = reader
            .read_event()
            .map_err(|_| malformed_xml_diag("invalid_xml"))?;
        let (tag, empty) = match event {
            Event::Start(tag) => {
                depth += 1;
                if depth > 8 {
                    return Err(malformed_xml_diag("excessive_depth"));
                }
                (tag, false)
            }
            Event::Empty(tag) => (tag, true),
            Event::End(_) => {
                if dated_cube == Some(depth) {
                    dated_cube = None;
                }
                depth -= 1;
                continue;
            }
            Event::DocType(_) | Event::PI(_) | Event::GeneralRef(_) => {
                return Err(malformed_xml_diag("disallowed_dtd_or_entity"));
            }
            Event::Eof if depth == 0 => break,
            Event::Eof => return Err(malformed_xml_diag("unclosed_tag")),
            _ => continue,
        };
        if tag.local_name().as_ref() != b"Cube" {
            continue;
        }
        let mut date = None;
        let mut currency = None;
        let mut rate = None;
        for attribute in tag.attributes() {
            let attribute = attribute.map_err(|_| malformed_xml_diag("invalid_attribute"))?;
            let field = match attribute.key.as_ref() {
                b"time" => &mut date,
                b"currency" => &mut currency,
                b"rate" => &mut rate,
                _ => return Err(malformed_xml_diag("unexpected_cube_attribute")),
            };
            *field = Some(
                std::str::from_utf8(&attribute.value)
                    .map_err(|_| malformed_xml_diag("invalid_utf8_payload"))?
                    .to_owned(),
            );
        }
        match (date, currency, rate) {
            (Some(date), None, None) if !empty && effective_date.is_none() => {
                dated_cube = Some(depth);
                effective_date = Some(date);
            }
            (None, Some(code), Some(rate)) if empty && dated_cube == Some(depth) => {
                if !is_iso_code(&code) || code == "EUR" || parse_positive_rate(&rate).is_none() {
                    return Err(malformed_xml_diag("invalid_rate"));
                }
                if rates.len() >= MAX_CURRENCIES_PER_SNAPSHOT - 1
                    || rates.insert(code, rate).is_some()
                {
                    return Err(malformed_xml_diag("duplicate_or_excessive_rates"));
                }
            }
            (None, None, None) if !empty && dated_cube.is_none() => {}
            _ => return Err(malformed_xml_diag("invalid_cube")),
        }
    }

    let effective_date = effective_date.ok_or_else(|| malformed_xml_diag("missing_time_cube"))?;
    if rates.is_empty() {
        return Err(malformed_xml_diag("empty_rates"));
    }
    rates.insert("EUR".to_string(), "1".to_string());

    let parsed_date = parse_strict_iso_date(&effective_date)
        .ok_or_else(|| malformed_xml_diag("invalid_cube_date"))?;
    let effective_ts = ecb_concertation_timestamp(parsed_date)?;
    let snapshot_id = compute_snapshot_id(&effective_date, &rates);

    let snapshot = Snapshot {
        id: snapshot_id,
        provider_id: ECB_PROVIDER_ID.to_string(),
        provider_name: ECB_PROVIDER_NAME.to_string(),
        source_url: ECB_DAILY_XML_URL.to_string(),
        attribution: ECB_ATTRIBUTION.to_string(),
        base_currency: "EUR".to_string(),
        effective_date,
        effective_at_utc: effective_ts.to_string(),
        fetched_at_utc: now.to_string(),
        ttl_seconds: DEFAULT_TTL_SECONDS,
        max_stale_seconds: DEFAULT_MAX_STALE_SECONDS,
        rates,
    };

    snapshot.validate(now)?;
    Ok(snapshot)
}

fn malformed_xml_diag(detail: &str) -> Diagnostic {
    let mut diag = Diagnostic::error("rate_unavailable");
    diag.parameters = json!({
        "reason": "malformed_provider_payload",
        "provider": ECB_PROVIDER_ID,
        "detail": detail,
    });
    diag
}

fn ecb_concertation_timestamp(date: Date) -> Result<Timestamp, Diagnostic> {
    let cet_dt = date.at(14, 15, 0, 0);
    TimeZone::get("Europe/Berlin")
        .and_then(|zone| zone.to_timestamp(cet_dt))
        .map_err(|_| malformed_xml_diag("timestamp_conversion_failed"))
}

fn compute_snapshot_id(effective_date: &str, rates: &BTreeMap<String, String>) -> String {
    // FNV-1a 64-bit deterministic content hash over canonical provider, date, and sorted rates.
    let mut hash: u64 = 0xcbf29ce484222325;
    for part in [ECB_PROVIDER_ID, effective_date] {
        hash = fnv1a_update(hash, part.as_bytes());
        hash = fnv1a_update(hash, b"\n");
    }
    for (k, v) in rates {
        hash = fnv1a_update(hash, k.as_bytes());
        hash = fnv1a_update(hash, b"=");
        hash = fnv1a_update(hash, v.as_bytes());
        hash = fnv1a_update(hash, b"\n");
    }
    format!("ecb-{effective_date}-{hash:016x}")
}

fn fnv1a_update(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Evaluate a currency conversion query against an offline `Snapshot`.
///
/// Returns:
/// - `None` if `query` is not a currency conversion request (so other routers/engines handle it).
/// - `Some(Ok(candidates))` with ranked candidates (primary first, alternates preserved) all
///   computed from the single `snapshot` using exact rational arithmetic.
/// - `Some(Err(diagnostic))` for missing/invalid/expired snapshots, unsupported currencies, or
///   ambiguous/malformed numeric literals in a recognized currency query.
pub(crate) fn preview(
    query: &str,
    now: Timestamp,
    snapshot: Option<&Snapshot>,
    preferred: Option<&str>,
) -> Option<Result<Vec<Candidate>, Diagnostic>> {
    if query.is_empty() || query.len() > MAX_QUERY_BYTES || query.chars().any(char::is_control) {
        return None;
    }

    let parsed = match parse_currency_query(query)? {
        Ok(q) => q,
        Err(diag) => return Some(Err(diag)),
    };

    let Some(snap) = snapshot else {
        let mut diag = Diagnostic::error("rate_unavailable");
        diag.parameters = json!({
            "reason": "missing_snapshot",
            "provider": ECB_PROVIDER_ID,
            "sourceCandidates": parsed.source.codes,
            "targetCandidates": parsed.target.codes,
        });
        return Some(Err(diag));
    };

    let (freshness, age_secs) = match snap.validate_internal(now) {
        Ok(ok) => ok,
        Err(diag) => return Some(Err(diag)),
    };

    let preferred_iso = normalize_preferred(preferred);
    let src_codes = order_codes(&parsed.source.codes, preferred_iso.as_deref());
    let dst_codes = order_codes(&parsed.target.codes, preferred_iso.as_deref());

    let mut pairs: Vec<(&'static str, &'static str)> = Vec::new();
    for &s in &src_codes {
        for &d in &dst_codes {
            pairs.push((s, d));
        }
    }

    let mut candidates = Vec::new();
    let mut missing_codes: Vec<&'static str> = Vec::new();

    for &(src, dst) in &pairs {
        let Some(src_rate_str) = snap.rates.get(src) else {
            if !missing_codes.contains(&src) {
                missing_codes.push(src);
            }
            continue;
        };
        let Some(dst_rate_str) = snap.rates.get(dst) else {
            if !missing_codes.contains(&dst) {
                missing_codes.push(dst);
            }
            continue;
        };

        let Some(src_rate) = parse_positive_rate(src_rate_str) else {
            return Some(Err(invalid_snapshot_diag("corrupt_source_rate")));
        };
        let Some(dst_rate) = parse_positive_rate(dst_rate_str) else {
            return Some(Err(invalid_snapshot_diag("corrupt_target_rate")));
        };

        let effective_rate = &dst_rate / &src_rate;
        let converted = &parsed.amount * &effective_rate;

        let candidate = build_candidate(
            &parsed,
            src,
            dst,
            src_rate_str,
            dst_rate_str,
            &effective_rate,
            &converted,
            snap,
            freshness,
            age_secs,
            &src_codes,
            &dst_codes,
        );
        candidates.push(candidate);
    }

    if candidates.is_empty() {
        let mut diag = Diagnostic::error("rate_unavailable");
        diag.parameters = json!({
            "reason": "unsupported_currency",
            "provider": snap.provider_id,
            "rateId": snap.id,
            "missingCurrencies": missing_codes,
        });
        return Some(Err(diag));
    }

    Some(Ok(candidates))
}

#[allow(clippy::too_many_arguments)]
fn build_candidate(
    parsed: &ParsedQuery,
    src: &'static str,
    dst: &'static str,
    src_rate_str: &str,
    dst_rate_str: &str,
    effective_rate: &BigRational,
    converted: &BigRational,
    snap: &Snapshot,
    freshness: Freshness,
    age_secs: i64,
    ordered_src_codes: &[&'static str],
    ordered_dst_codes: &[&'static str],
) -> Candidate {
    let dst_meta = currency_meta(dst);
    let src_meta = currency_meta(src);
    let minor_units = dst_meta.map(|m| m.minor_units).unwrap_or(2);
    let src_name = src_meta.map(|m| m.name).unwrap_or(src);
    let dst_name = dst_meta.map(|m| m.name).unwrap_or(dst);

    let display_num = format_rational_decimal(converted, minor_units, false);
    let precise_num = format_rational_decimal(converted, PRECISE_DECIMAL_SCALE, true);
    let rate_decimal = format_rational_decimal(effective_rate, PRECISE_DECIMAL_SCALE, true);

    let display_with_code = format!("{display_num} {dst}");
    let precise_with_code = format!("{precise_num} {dst}");
    let canonical_expr = format!("{} {src} to {dst}", parsed.amount_canonical);

    let id = format!(
        "fx-{}-{}",
        src.to_ascii_lowercase(),
        dst.to_ascii_lowercase()
    );
    let normalized = format!("{} {src} → {dst}", parsed.amount_canonical);
    let interpretation = format!(
        "{} {src_name} ({src}) → {dst_name} ({dst})",
        parsed.amount_canonical
    );

    let is_stale = freshness == Freshness::StaleUsable;
    let source_rate = parse_positive_rate(src_rate_str);
    let favourites: Vec<_> = ["EUR", "JPY", "HKD"]
        .into_iter()
        .filter(|code| *code != dst && *code != src)
        .filter_map(|code| {
            let rate = parse_positive_rate(snap.rates.get(code)?)?;
            let source_rate = source_rate.as_ref()?;
            let meta = currency_meta(code)?;
            let amount = format_rational_decimal(
                &(&parsed.amount * rate / source_rate),
                meta.minor_units,
                false,
            );
            Some(json!({"currency":code, "name":meta.name, "display":format!("{amount} {code}")}))
        })
        .collect();
    let value = json!({
        "tag": "currency",
        "favourites": favourites,
        "amount": precise_num,
        "display": display_with_code,
        "source": src,
        "sourceAmount": parsed.amount_canonical,
        "target": dst,
        "rateDecimal": rate_decimal,
        "rateId": snap.id,
        "asOf": snap.effective_at_utc,
        "effectiveDate": snap.effective_date,
        "fetchedAt": snap.fetched_at_utc,
        "provider": snap.provider_id,
        "freshness": freshness.as_str(),
        "stale": is_stale,
    });

    let mut candidate = Candidate::new(id, "currency", &normalized, &interpretation, value);

    let mut assumptions = Vec::new();
    if parsed.source.is_ambiguous() {
        let alts: Vec<&str> = ordered_src_codes
            .iter()
            .copied()
            .filter(|&c| c != src)
            .collect();
        assumptions.push(format!(
            "Interpreted '{}' as {} ({}); alternatives: {}",
            parsed.source.raw_token,
            src,
            src_name,
            alts.join(", ")
        ));
    }
    if parsed.target.is_ambiguous() {
        let alts: Vec<&str> = ordered_dst_codes
            .iter()
            .copied()
            .filter(|&c| c != dst)
            .collect();
        assumptions.push(format!(
            "Interpreted target '{}' as {} ({}); alternatives: {}",
            parsed.target.raw_token,
            dst,
            dst_name,
            alts.join(", ")
        ));
    }
    if src == dst {
        assumptions.push(format!("Identity conversion ({src} → {dst}, rate = 1)"));
    } else if src != "EUR" && dst != "EUR" {
        assumptions.push(format!(
            "Cross-rate derived via ECB EUR reference rates (1 EUR = {src_rate_str} {src}, 1 EUR = {dst_rate_str} {dst})"
        ));
    } else {
        assumptions.push(format!(
            "Direct ECB EUR reference rate from {}",
            snap.effective_date
        ));
    }
    assumptions.push(format!(
        "Exact decimal/rational arithmetic; display rounded half-up to {minor_units} decimal places"
    ));
    if is_stale {
        assumptions.push(format!(
            "Using stale offline snapshot from {} (fetched {}); refresh rates for current quotes",
            snap.effective_date, snap.fetched_at_utc
        ));
    }
    candidate.assumptions = assumptions;

    candidate.formats = vec![
        Format::new("display", &display_with_code, &display_with_code),
        Format::new("precise", &precise_with_code, &precise_with_code),
        Format::new("number", &display_num, &display_num),
        Format::new("canonical", &canonical_expr, &canonical_expr),
    ];

    let mut diagnostics = Vec::new();
    if parsed.source.is_ambiguous() {
        let mut diag = Diagnostic::warning("ambiguous_currency");
        diag.parameters = json!({
            "symbol": parsed.source.raw_token,
            "selected": src,
            "candidates": ordered_src_codes,
        });
        diag.span = Some(json!({
            "startUtf16": parsed.source.start_utf16,
            "endUtf16": parsed.source.end_utf16,
        }));
        diag.suggestions = ordered_src_codes
            .iter()
            .take(8)
            .map(|&code| {
                let text = format!("{} {code} to {dst}", parsed.amount_canonical);
                json!({
                    "id": format!("fx-disambiguate-{}", code.to_ascii_lowercase()),
                    "label": text,
                    "replacement": {
                        "startUtf16": 0,
                        "endUtf16": parsed.query_end_utf16,
                        "text": text,
                    }
                })
            })
            .collect();
        diagnostics.push(diag);
    }
    if is_stale {
        let mut diag = Diagnostic::warning("rate_stale");
        diag.parameters = json!({
            "freshness": freshness.as_str(),
            "provider": snap.provider_id,
            "rateId": snap.id,
            "effectiveDate": snap.effective_date,
            "fetchedAtUtc": snap.fetched_at_utc,
            "ageSeconds": age_secs,
            "ttlSeconds": snap.ttl_seconds,
            "maxStaleSeconds": snap.max_stale_seconds,
        });
        diagnostics.push(diag);
    }
    candidate.diagnostics = diagnostics;

    candidate.provenance = Some(json!({
        "kind": "fx_snapshot",
        "rateId": snap.id,
        "providerId": snap.provider_id,
        "providerName": snap.provider_name,
        "sourceUrl": snap.source_url,
        "attribution": snap.attribution,
        "baseCurrency": snap.base_currency,
        "effectiveDate": snap.effective_date,
        "effectiveAtUtc": snap.effective_at_utc,
        "fetchedAtUtc": snap.fetched_at_utc,
        "freshness": freshness.as_str(),
        "stale": is_stale,
        "terms": ECB_TERMS_SUMMARY,
    }));

    candidate
}

// ---------------------------------------------------------------------------
// Currency Metadata Registry
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct CurrencyMeta {
    code: &'static str,
    name: &'static str,
    minor_units: usize,
}

const CURRENCY_REGISTRY: &[CurrencyMeta] = &[
    // ECB eurofxref daily currencies
    CurrencyMeta {
        code: "EUR",
        name: "Euro",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "USD",
        name: "US Dollar",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "JPY",
        name: "Japanese Yen",
        minor_units: 0,
    },
    CurrencyMeta {
        code: "BGN",
        name: "Bulgarian Lev",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "CZK",
        name: "Czech Koruna",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "DKK",
        name: "Danish Krone",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "GBP",
        name: "Pound Sterling",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "HUF",
        name: "Hungarian Forint",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "PLN",
        name: "Polish Zloty",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "RON",
        name: "Romanian Leu",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "SEK",
        name: "Swedish Krona",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "CHF",
        name: "Swiss Franc",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "ISK",
        name: "Icelandic Krona",
        minor_units: 0,
    },
    CurrencyMeta {
        code: "NOK",
        name: "Norwegian Krone",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "TRY",
        name: "Turkish Lira",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "AUD",
        name: "Australian Dollar",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "BRL",
        name: "Brazilian Real",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "CAD",
        name: "Canadian Dollar",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "CNY",
        name: "Chinese Yuan",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "HKD",
        name: "Hong Kong Dollar",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "IDR",
        name: "Indonesian Rupiah",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "ILS",
        name: "Israeli New Shekel",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "INR",
        name: "Indian Rupee",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "KRW",
        name: "South Korean Won",
        minor_units: 0,
    },
    CurrencyMeta {
        code: "MXN",
        name: "Mexican Peso",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "MYR",
        name: "Malaysian Ringgit",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "NZD",
        name: "New Zealand Dollar",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "PHP",
        name: "Philippine Peso",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "SGD",
        name: "Singapore Dollar",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "THB",
        name: "Thai Baht",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "ZAR",
        name: "South African Rand",
        minor_units: 2,
    },
    // Additional ISO 4217 codes recognized so unsupported queries yield `rate_unavailable`
    CurrencyMeta {
        code: "TWD",
        name: "New Taiwan Dollar",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "VND",
        name: "Vietnamese Dong",
        minor_units: 0,
    },
    CurrencyMeta {
        code: "AED",
        name: "UAE Dirham",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "SAR",
        name: "Saudi Riyal",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "RUB",
        name: "Russian Ruble",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "UAH",
        name: "Ukrainian Hryvnia",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "CLP",
        name: "Chilean Peso",
        minor_units: 0,
    },
    CurrencyMeta {
        code: "COP",
        name: "Colombian Peso",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "PEN",
        name: "Peruvian Sol",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "ARS",
        name: "Argentine Peso",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "EGP",
        name: "Egyptian Pound",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "NGN",
        name: "Nigerian Naira",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "PKR",
        name: "Pakistani Rupee",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "BDT",
        name: "Bangladeshi Taka",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "LKR",
        name: "Sri Lankan Rupee",
        minor_units: 2,
    },
    CurrencyMeta {
        code: "KWD",
        name: "Kuwaiti Dinar",
        minor_units: 3,
    },
    CurrencyMeta {
        code: "BHD",
        name: "Bahraini Dinar",
        minor_units: 3,
    },
    CurrencyMeta {
        code: "OMR",
        name: "Omani Rial",
        minor_units: 3,
    },
    CurrencyMeta {
        code: "JOD",
        name: "Jordanian Dinar",
        minor_units: 3,
    },
    CurrencyMeta {
        code: "QAR",
        name: "Qatari Riyal",
        minor_units: 2,
    },
];

fn currency_meta(code: &str) -> Option<&'static CurrencyMeta> {
    CURRENCY_REGISTRY.iter().find(|m| m.code == code)
}

#[derive(Debug, Clone)]
struct CurrencyDesignator {
    codes: Vec<&'static str>,
    raw_token: String,
    start_utf16: usize,
    end_utf16: usize,
}

impl CurrencyDesignator {
    fn is_ambiguous(&self) -> bool {
        self.codes.len() > 1
    }
}

/// Ordered symbol & alias table. Longer prefixes must be checked before shorter ones
/// when matching attached prefix/suffix tokens (e.g. `HK$` before `$`).
const SYMBOL_PREFIXES: &[(&str, &[&str])] = &[
    ("CN¥", &["CNY"]),
    ("CN￥", &["CNY"]),
    ("JP¥", &["JPY"]),
    ("JP￥", &["JPY"]),
    ("US$", &["USD"]),
    ("HK$", &["HKD"]),
    ("CA$", &["CAD"]),
    ("AU$", &["AUD"]),
    ("NZ$", &["NZD"]),
    ("SG$", &["SGD"]),
    ("MX$", &["MXN"]),
    ("NT$", &["TWD"]),
    ("C$", &["CAD"]),
    ("A$", &["AUD"]),
    ("S$", &["SGD"]),
    ("R$", &["BRL"]),
    ("¥", &["CNY", "JPY"]),
    ("￥", &["CNY", "JPY"]),
    ("$", &["USD", "CAD", "AUD", "HKD", "SGD", "NZD", "MXN"]),
    ("€", &["EUR"]),
    ("£", &["GBP"]),
    ("₹", &["INR"]),
    ("₩", &["KRW"]),
    ("₪", &["ILS"]),
    ("₱", &["PHP"]),
    ("฿", &["THB"]),
    ("₺", &["TRY"]),
    ("zł", &["PLN"]),
    ("ZŁ", &["PLN"]),
    ("RM", &["MYR"]),
    ("Rp", &["IDR"]),
    ("kr", &["SEK", "NOK", "DKK", "ISK"]),
    ("KR", &["SEK", "NOK", "DKK", "ISK"]),
];

fn resolve_currency_token(token: &str) -> Option<Vec<&'static str>> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return None;
    }

    for &(sym, codes) in SYMBOL_PREFIXES {
        if trimmed == sym {
            return Some(codes.to_vec());
        }
        // Case-insensitive match for multi-letter ASCII symbols like `us$`, `hk$`, `kr`, `rm`
        if sym.is_ascii() && trimmed.eq_ignore_ascii_case(sym) {
            return Some(codes.to_vec());
        }
    }

    let upper = trimmed.to_ascii_uppercase();
    if let Some(meta) = currency_meta(&upper) {
        return Some(vec![meta.code]);
    }

    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "rmb" | "yuan" | "renminbi" => Some(vec!["CNY"]),
        "yen" => Some(vec!["JPY"]),
        "euro" | "euros" => Some(vec!["EUR"]),
        "dollar" | "dollars" => Some(vec!["USD", "CAD", "AUD", "HKD", "SGD", "NZD", "MXN"]),
        "pound" | "pounds" | "sterling" => Some(vec!["GBP"]),
        "franc" | "francs" => Some(vec!["CHF"]),
        "rupee" | "rupees" => Some(vec!["INR"]),
        "won" => Some(vec!["KRW"]),
        "baht" => Some(vec!["THB"]),
        "peso" | "pesos" => Some(vec!["MXN", "PHP"]),
        "krona" | "kronor" => Some(vec!["SEK", "ISK"]),
        "krone" | "kroner" => Some(vec!["NOK", "DKK"]),
        "zloty" | "złoty" => Some(vec!["PLN"]),
        "lira" => Some(vec!["TRY"]),
        "shekel" | "shekels" => Some(vec!["ILS"]),
        "rand" => Some(vec!["ZAR"]),
        _ => None,
    }
}

fn normalize_preferred(preferred: Option<&str>) -> Option<String> {
    let raw = preferred?.trim();
    if raw.is_empty() {
        return None;
    }
    let upper = raw.to_ascii_uppercase();
    if currency_meta(&upper).is_some() {
        return Some(upper);
    }
    let lower = raw.to_ascii_lowercase().replace('-', "_");
    match lower.as_str() {
        "zh" | "zh_cn" | "zh_hans" | "zh_hans_cn" => Some("CNY".to_string()),
        "ja" | "ja_jp" => Some("JPY".to_string()),
        "en_us" => Some("USD".to_string()),
        "en_gb" => Some("GBP".to_string()),
        "en_ca" | "fr_ca" => Some("CAD".to_string()),
        "en_au" => Some("AUD".to_string()),
        "zh_hk" | "en_hk" => Some("HKD".to_string()),
        "en_sg" => Some("SGD".to_string()),
        "en_nz" => Some("NZD".to_string()),
        "sv" | "sv_se" => Some("SEK".to_string()),
        "nb" | "nn" | "no" | "nb_no" => Some("NOK".to_string()),
        "da" | "da_dk" => Some("DKK".to_string()),
        "is" | "is_is" => Some("ISK".to_string()),
        _ => None,
    }
}

fn order_codes(codes: &[&'static str], preferred: Option<&str>) -> Vec<&'static str> {
    let mut ordered = Vec::with_capacity(codes.len());
    if let Some(pref) = preferred
        && let Some(&m) = codes.iter().find(|&&c| c == pref)
    {
        ordered.push(m);
    }
    for &c in codes {
        if !ordered.contains(&c) {
            ordered.push(c);
        }
    }
    ordered
}

// ---------------------------------------------------------------------------
// Query Parser (Span-preserving)
// ---------------------------------------------------------------------------

struct ParsedQuery {
    amount: BigRational,
    amount_canonical: String,
    source: CurrencyDesignator,
    target: CurrencyDesignator,
    query_end_utf16: usize,
}

fn utf16_offset(s: &str, byte_idx: usize) -> usize {
    s[..byte_idx].encode_utf16().count()
}

fn parse_currency_query(query: &str) -> Option<Result<ParsedQuery, Diagnostic>> {
    let trimmed = query.trim();
    if trimmed.is_empty() || trimmed.contains("://") {
        return None;
    }

    let query_end_utf16 = query.encode_utf16().count();
    let (left_range, right_range) = split_conversion_connector(query)?;

    let right_slice = &query[right_range.0..right_range.1];
    let (r_trim_start, r_trim_end) = trim_byte_range(query, right_range.0, right_range.1)?;
    let target_codes = resolve_currency_token(right_slice.trim())?;
    let target = CurrencyDesignator {
        codes: target_codes,
        raw_token: query[r_trim_start..r_trim_end].to_string(),
        start_utf16: utf16_offset(query, r_trim_start),
        end_utf16: utf16_offset(query, r_trim_end),
    };

    let (l_start, l_end) = trim_byte_range(query, left_range.0, left_range.1)?;
    match parse_source_quantity(query, l_start, l_end) {
        Some(Ok((amount, amount_canonical, source))) => Some(Ok(ParsedQuery {
            amount,
            amount_canonical,
            source,
            target,
            query_end_utf16,
        })),
        Some(Err(diag)) => Some(Err(diag)),
        None => None,
    }
}

fn trim_byte_range(s: &str, mut start: usize, mut end: usize) -> Option<(usize, usize)> {
    while start < end {
        let ch = s[start..end].chars().next()?;
        if is_space_like(ch) {
            start += ch.len_utf8();
        } else {
            break;
        }
    }
    while start < end {
        let ch = s[start..end].chars().next_back()?;
        if is_space_like(ch) {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }
    if start < end {
        Some((start, end))
    } else {
        None
    }
}

fn is_space_like(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{00A0}'
}

fn split_conversion_connector(query: &str) -> Option<((usize, usize), (usize, usize))> {
    // Prefer explicit arrow connectors `->` / `=>` if present once.
    for arrow in ["->", "=>"] {
        if let Some(idx) = query.find(arrow) {
            let right_start = idx + arrow.len();
            if query[right_start..].contains("->") || query[right_start..].contains("=>") {
                return None;
            }
            return Some(((0, idx), (right_start, query.len())));
        }
    }

    // Otherwise scan whitespace-bounded tokens for `in`, `to`, `into` (case-insensitive).
    let mut tokens: Vec<(usize, usize)> = Vec::new();
    let mut in_tok = false;
    let mut tok_start = 0usize;
    for (idx, ch) in query.char_indices() {
        if is_space_like(ch) {
            if in_tok {
                tokens.push((tok_start, idx));
                in_tok = false;
            }
        } else if !in_tok {
            tok_start = idx;
            in_tok = true;
        }
    }
    if in_tok {
        tokens.push((tok_start, query.len()));
    }

    let mut match_idx: Option<(usize, usize)> = None;
    for &(s, e) in &tokens {
        let word = &query[s..e];
        if word.eq_ignore_ascii_case("in")
            || word.eq_ignore_ascii_case("to")
            || word.eq_ignore_ascii_case("into")
        {
            if match_idx.is_some() {
                // Multiple connectors like "how to go in usd" -> not a clean conversion query.
                return None;
            }
            match_idx = Some((s, e));
        }
    }

    let (conn_s, conn_e) = match_idx?;
    if conn_s == 0 || conn_e >= query.len() {
        return None;
    }
    Some(((0, conn_s), (conn_e, query.len())))
}

fn parse_source_quantity(
    query: &str,
    start: usize,
    end: usize,
) -> Option<Result<(BigRational, String, CurrencyDesignator), Diagnostic>> {
    let raw = &query[start..end];
    if raw.is_empty() {
        return None;
    }

    // Case 1: Optional leading sign, then currency prefix, then optional whitespace, then number
    // e.g. "¥2999", "HK$ 500", "-$50", "USD 100", "CNY2999"
    let (sign_str, after_sign_offset) = strip_leading_sign(raw);
    if let Some((codes, sym_start_rel, sym_end_rel)) =
        match_currency_prefix(&raw[after_sign_offset..])
    {
        let sym_abs_start = start + after_sign_offset + sym_start_rel;
        let sym_abs_end = start + after_sign_offset + sym_end_rel;
        let rest_start = sym_abs_end;
        if let Some((num_s, num_e)) = trim_byte_range(query, rest_start, end) {
            let num_slice = &query[num_s..num_e];
            // Handle `$-50` (sign after currency symbol) when no leading sign preceded symbol
            let combined_num = if sign_str.is_empty() {
                num_slice.to_string()
            } else if num_slice.starts_with('-')
                || num_slice.starts_with('\u{2212}')
                || num_slice.starts_with('+')
            {
                return Some(Err(invalid_number_diag(query, start, end, None)));
            } else {
                format!("{sign_str}{num_slice}")
            };

            if looks_numeric_attempt(&combined_num) {
                return Some(parse_amount_and_wrap(
                    query,
                    &combined_num,
                    num_s,
                    num_e,
                    codes,
                    sym_abs_start,
                    sym_abs_end,
                    raw[sym_start_rel + after_sign_offset..sym_end_rel + after_sign_offset]
                        .to_string(),
                ));
            }
            return None;
        }
        return None;
    }

    // Case 2: Number followed by optional whitespace and currency suffix
    // e.g. "2999 JPY", "2999¥", "100.50 EUR", "100€", "2999 yuan"
    if let Some((codes, sym_start_rel, sym_end_rel)) = match_currency_suffix(raw) {
        let sym_abs_start = start + sym_start_rel;
        let sym_abs_end = start + sym_end_rel;
        if let Some((num_s, num_e)) = trim_byte_range(query, start, sym_abs_start) {
            let num_slice = &query[num_s..num_e];
            if looks_numeric_attempt(num_slice) {
                return Some(parse_amount_and_wrap(
                    query,
                    num_slice,
                    num_s,
                    num_e,
                    codes,
                    sym_abs_start,
                    sym_abs_end,
                    raw[sym_start_rel..sym_end_rel].to_string(),
                ));
            }
        }
    }

    None
}

fn strip_leading_sign(s: &str) -> (&'static str, usize) {
    if let Some(rest) = s.strip_prefix('-') {
        let off = s.len() - rest.len();
        ("-", off)
    } else if let Some(rest) = s.strip_prefix('\u{2212}') {
        let off = s.len() - rest.len();
        ("-", off)
    } else if let Some(rest) = s.strip_prefix('+') {
        let off = s.len() - rest.len();
        ("", off)
    } else {
        ("", 0)
    }
}

fn match_currency_prefix(s: &str) -> Option<(Vec<&'static str>, usize, usize)> {
    let trimmed_start = s.len() - s.trim_start_matches(is_space_like).len();
    let tail = &s[trimmed_start..];
    if tail.is_empty() {
        return None;
    }

    // Check explicit symbols (longest match first in `SYMBOL_PREFIXES`)
    for &(sym, codes) in SYMBOL_PREFIXES {
        let matched_len = if tail.starts_with(sym) {
            Some(sym.len())
        } else if sym.is_ascii()
            && tail.len() >= sym.len()
            && tail.is_char_boundary(sym.len())
            && tail[..sym.len()].eq_ignore_ascii_case(sym)
        {
            // Avoid matching ASCII word-like symbols (e.g. `RM`, `kr`) if glued to letters
            let after = tail[sym.len()..].chars().next();
            if sym.chars().all(|c| c.is_ascii_alphabetic())
                && after.is_some_and(|c| c.is_ascii_alphabetic())
            {
                None
            } else {
                Some(sym.len())
            }
        } else {
            None
        };

        if let Some(len) = matched_len {
            let after = tail[len..].chars().next();
            if after.is_some_and(|c| is_space_like(c) || is_digit_or_sign_or_dot(c)) {
                return Some((codes.to_vec(), trimmed_start, trimmed_start + len));
            }
        }
    }

    // Check 3-letter ISO code or word token before whitespace or digit (e.g. `USD 100`, `USD100`)
    let word_len = tail
        .find(|c: char| !c.is_ascii_alphabetic())
        .unwrap_or(tail.len());
    if word_len >= 3 {
        let candidate = &tail[..word_len];
        let after = tail[word_len..].chars().next();
        if after.is_some_and(|c| is_space_like(c) || is_digit_or_sign_or_dot(c))
            && let Some(codes) = resolve_currency_token(candidate)
        {
            return Some((codes, trimmed_start, trimmed_start + word_len));
        }
    }

    None
}

fn match_currency_suffix(s: &str) -> Option<(Vec<&'static str>, usize, usize)> {
    // Check explicit symbols at the end of `s`
    for &(sym, codes) in SYMBOL_PREFIXES {
        if let Some(sym_start) = s.len().checked_sub(sym.len())
            && s.get(sym_start..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(sym))
        {
            let before = s[..sym_start].chars().next_back();
            // For alphabetic symbol suffixes like `kr` or `RM`, require boundary from ASCII letters
            let ascii_alpha_sym = sym.chars().all(|c| c.is_ascii_alphabetic());
            let valid_boundary = if ascii_alpha_sym {
                before.is_some_and(|c| is_space_like(c) || is_ascii_or_fw_digit(c))
            } else {
                before.is_some_and(|c| is_space_like(c) || is_ascii_or_fw_digit(c) || c == '.')
            };
            if valid_boundary {
                return Some((codes.to_vec(), sym_start, s.len()));
            }
        }
    }

    // Check trailing alphabetic currency code/word (`2999 JPY`, `2999JPY`, `2999 yuan`)
    let mut tok_start = s.len();
    for (idx, ch) in s.char_indices().rev() {
        if ch.is_alphabetic() {
            tok_start = idx;
        } else {
            break;
        }
    }
    if tok_start < s.len() {
        let word = &s[tok_start..];
        let before = s[..tok_start].chars().next_back();
        if before.is_some_and(|c| is_space_like(c) || is_ascii_or_fw_digit(c) || c == '.')
            && let Some(codes) = resolve_currency_token(word)
        {
            return Some((codes, tok_start, s.len()));
        }
    }

    None
}

fn is_ascii_or_fw_digit(ch: char) -> bool {
    ch.is_ascii_digit() || ('０'..='９').contains(&ch)
}

fn is_digit_or_sign_or_dot(ch: char) -> bool {
    is_ascii_or_fw_digit(ch) || matches!(ch, '.' | '-' | '\u{2212}' | '+')
}

fn looks_numeric_attempt(s: &str) -> bool {
    s.chars().any(is_ascii_or_fw_digit)
        && s.chars().all(|c| {
            is_ascii_or_fw_digit(c) || matches!(c, '.' | ',' | '_' | '-' | '\u{2212}' | '+')
        })
}

#[allow(clippy::too_many_arguments)]
fn parse_amount_and_wrap(
    query: &str,
    num_raw: &str,
    num_start: usize,
    num_end: usize,
    codes: Vec<&'static str>,
    sym_abs_start: usize,
    sym_abs_end: usize,
    raw_token: String,
) -> Result<(BigRational, String, CurrencyDesignator), Diagnostic> {
    let (amount, canonical) = parse_user_decimal(query, num_raw, num_start, num_end)?;
    let source = CurrencyDesignator {
        codes,
        raw_token,
        start_utf16: utf16_offset(query, sym_abs_start),
        end_utf16: utf16_offset(query, sym_abs_end),
    };
    Ok((amount, canonical, source))
}

fn parse_user_decimal(
    query: &str,
    num_raw: &str,
    num_start: usize,
    num_end: usize,
) -> Result<(BigRational, String), Diagnostic> {
    let mut normalized = String::with_capacity(num_raw.len());
    for ch in num_raw.chars() {
        let mapped = match ch {
            '０'..='９' => char::from(b'0' + (ch as u32 - '０' as u32) as u8),
            '\u{2212}' => '-',
            _ => ch,
        };
        normalized.push(mapped);
    }

    let (neg, unsigned) = if let Some(rest) = normalized.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = normalized.strip_prefix('+') {
        (false, rest)
    } else {
        (false, normalized.as_str())
    };

    if unsigned.is_empty() || unsigned.starts_with('_') || unsigned.ends_with('_') {
        return Err(invalid_number_diag(query, num_start, num_end, None));
    }

    // Disallow ambiguous single-comma numbers like `1,234` without a `.` (spec §3.3).
    if unsigned.contains(',') {
        if let Some((int_part, frac_part)) = unsigned.split_once('.') {
            if frac_part.contains(',') || !is_valid_comma_grouped_int(int_part) {
                return Err(invalid_number_diag(query, num_start, num_end, None));
            }
        } else {
            // Check whether single comma is ambiguous `1,234` (3 trailing digits)
            let parts: Vec<&str> = unsigned.split(',').collect();
            if parts.len() == 2
                && !parts[0].is_empty()
                && parts[0].bytes().all(|b| b.is_ascii_digit())
                && parts[1].len() == 3
                && parts[1].bytes().all(|b| b.is_ascii_digit())
            {
                let thousands = format!("{}{}{}", if neg { "-" } else { "" }, parts[0], parts[1]);
                let decimal = format!("{}{}.{}", if neg { "-" } else { "" }, parts[0], parts[1]);
                let mut diag = invalid_number_diag(
                    query,
                    num_start,
                    num_end,
                    Some("ambiguous_decimal_separator"),
                );
                let s_utf16 = utf16_offset(query, num_start);
                let e_utf16 = utf16_offset(query, num_end);
                diag.suggestions = vec![
                    json!({
                        "id": "sep-thousands",
                        "label": thousands,
                        "replacement": { "startUtf16": s_utf16, "endUtf16": e_utf16, "text": thousands }
                    }),
                    json!({
                        "id": "sep-decimal",
                        "label": decimal,
                        "replacement": { "startUtf16": s_utf16, "endUtf16": e_utf16, "text": decimal }
                    }),
                ];
                return Err(diag);
            }
            if !is_valid_comma_grouped_int(unsigned) {
                return Err(invalid_number_diag(query, num_start, num_end, None));
            }
        }
    }

    let cleaned: String = unsigned.chars().filter(|&c| c != '_' && c != ',').collect();

    let (int_str, frac_str) = match cleaned.split_once('.') {
        Some((i, f)) => {
            if f.contains('.') || (i.is_empty() && f.is_empty()) {
                return Err(invalid_number_diag(query, num_start, num_end, None));
            }
            (if i.is_empty() { "0" } else { i }, f)
        }
        None => (cleaned.as_str(), ""),
    };

    if !int_str.bytes().all(|b| b.is_ascii_digit()) || !frac_str.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid_number_diag(query, num_start, num_end, None));
    }

    let total_digits = int_str.len() + frac_str.len();
    if total_digits > MAX_INPUT_NUM_DIGITS {
        let mut diag = Diagnostic::error("numeric_overflow");
        diag.parameters = json!({
            "maxDigits": MAX_INPUT_NUM_DIGITS,
            "actualDigits": total_digits,
        });
        diag.span = Some(json!({
            "startUtf16": utf16_offset(query, num_start),
            "endUtf16": utf16_offset(query, num_end),
        }));
        return Err(diag);
    }

    let combined_digits = format!("{int_str}{frac_str}");
    let mut numer = combined_digits
        .parse::<BigInt>()
        .map_err(|_| invalid_number_diag(query, num_start, num_end, None))?;
    if neg && !numer.is_zero() {
        numer = -numer;
    }
    let denom = pow10(frac_str.len());
    let rational = BigRational::new(numer, denom);

    // Canonical clean string (preserving exact entered fractional digits minus redundant separators)
    let int_trimmed = int_str.trim_start_matches('0');
    let int_canon = if int_trimmed.is_empty() {
        "0"
    } else {
        int_trimmed
    };
    let sign_prefix = if neg && !rational.is_zero() { "-" } else { "" };
    let canonical = if frac_str.is_empty() {
        format!("{sign_prefix}{int_canon}")
    } else {
        format!("{sign_prefix}{int_canon}.{frac_str}")
    };

    Ok((rational, canonical))
}

fn is_valid_comma_grouped_int(s: &str) -> bool {
    let groups: Vec<&str> = s.split(',').collect();
    if groups.len() < 2 {
        return false;
    }
    let first = groups[0];
    if first.is_empty() || first.len() > 3 || !first.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    groups[1..]
        .iter()
        .all(|g| g.len() == 3 && g.bytes().all(|b| b.is_ascii_digit()))
}

fn invalid_number_diag(
    query: &str,
    num_start: usize,
    num_end: usize,
    detail: Option<&str>,
) -> Diagnostic {
    let mut diag = Diagnostic::error("invalid_syntax");
    diag.parameters = json!({
        "detail": detail.unwrap_or("invalid_currency_amount"),
    });
    diag.span = Some(json!({
        "startUtf16": utf16_offset(query, num_start),
        "endUtf16": utf16_offset(query, num_end),
    }));
    diag
}

// ---------------------------------------------------------------------------
// Exact Decimal / Rational Helpers
// ---------------------------------------------------------------------------

fn pow10(exp: usize) -> BigInt {
    let mut acc = BigInt::from(1u8);
    let ten = BigInt::from(10u8);
    for _ in 0..exp {
        acc *= &ten;
    }
    acc
}

fn parse_positive_rate(s: &str) -> Option<BigRational> {
    if s.is_empty() || s.len() > MAX_RATE_STR_LEN {
        return None;
    }
    if s.starts_with('+') || s.starts_with('-') || s.trim() != s {
        return None;
    }
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => {
            if i.is_empty() || f.is_empty() || f.contains('.') || f.len() > MAX_RATE_FRAC_DIGITS {
                return None;
            }
            (i, f)
        }
        None => (s, ""),
    };
    if !int_part.bytes().all(|b| b.is_ascii_digit())
        || !frac_part.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let digits = format!("{int_part}{frac_part}");
    let numer = digits.parse::<BigInt>().ok()?;
    if numer <= BigInt::zero() {
        return None;
    }
    let denom = pow10(frac_part.len());
    Some(BigRational::new(numer, denom))
}

/// Format a `BigRational` as a decimal string rounded half-up to `scale` fractional digits.
fn format_rational_decimal(value: &BigRational, scale: usize, trim_trailing_zeros: bool) -> String {
    let is_neg = value.is_negative();
    let abs_numer = value.numer().abs();
    let denom = value.denom();
    let factor = pow10(scale);
    let scaled = &abs_numer * &factor;

    let mut q = &scaled / denom;
    let rem = &scaled % denom;

    // Half-up rounding: 2 * rem >= denom
    if (&rem * BigInt::from(2u8)) >= *denom {
        q += BigInt::from(1u8);
    }

    if q.is_zero() {
        if scale == 0 || trim_trailing_zeros {
            return "0".to_string();
        }
        return format!("0.{}", "0".repeat(scale));
    }

    let q_str = q.to_string();
    let mut out = String::new();
    if is_neg {
        out.push('-');
    }

    if scale == 0 {
        out.push_str(&q_str);
        return out;
    }

    let (int_part, frac_part) = if q_str.len() <= scale {
        let mut frac = "0".repeat(scale - q_str.len());
        frac.push_str(&q_str);
        ("0".to_string(), frac)
    } else {
        let split = q_str.len() - scale;
        (q_str[..split].to_string(), q_str[split..].to_string())
    };

    if trim_trailing_zeros {
        let trimmed_frac = frac_part.trim_end_matches('0');
        if trimmed_frac.is_empty() {
            out.push_str(&int_part);
        } else {
            let _ = write!(out, "{int_part}.{trimmed_frac}");
        }
    } else {
        let _ = write!(out, "{int_part}.{frac_part}");
    }

    out
}

// ---------------------------------------------------------------------------
// Validation Utilities
// ---------------------------------------------------------------------------

fn is_iso_code(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase())
}

fn is_safe_identifier(s: &str, max_len: usize) -> bool {
    !s.is_empty()
        && s.len() <= max_len
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

fn is_safe_label(s: &str, max_len: usize) -> bool {
    !s.is_empty() && s.len() <= max_len && !s.chars().any(char::is_control)
}

fn parse_strict_iso_date(s: &str) -> Option<Date> {
    let bytes = s.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    if !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..10].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    s.parse::<Date>().ok()
}

// ---------------------------------------------------------------------------
// Unit Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_ECB_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
    <gesmes:subject>Reference rates</gesmes:subject>
    <gesmes:Sender>
        <gesmes:name>European Central Bank</gesmes:name>
    </gesmes:Sender>
    <Cube>
        <Cube time='2026-11-13'>
            <Cube currency='USD' rate='1.0800'/>
            <Cube currency='JPY' rate='165.00'/>
            <Cube currency='GBP' rate='0.8350'/>
            <Cube currency='CAD' rate='1.5000'/>
            <Cube currency='CNY' rate='7.8000'/>
            <Cube currency='HKD' rate='8.4000'/>
            <Cube currency='SEK' rate='11.5000'/>
            <Cube currency='NOK' rate='11.7500'/>
        </Cube>
    </Cube>
</gesmes:Envelope>"#;

    struct MockProvider {
        payload: Result<String, Diagnostic>,
    }

    impl FxProvider for MockProvider {
        fn fetch_daily_xml(&self) -> Result<String, Diagnostic> {
            self.payload.clone()
        }
    }

    fn ts(s: &str) -> Timestamp {
        s.parse().expect("valid test timestamp")
    }

    fn diag_code(d: &Diagnostic) -> String {
        serde_json::to_value(d)
            .expect("serialize diagnostic")
            .get("code")
            .and_then(Value::as_str)
            .expect("diagnostic code")
            .to_string()
    }

    fn diag_severity(d: &Diagnostic) -> String {
        serde_json::to_value(d)
            .expect("serialize diagnostic")
            .get("severity")
            .and_then(Value::as_str)
            .expect("diagnostic severity")
            .to_string()
    }

    fn fixture_snapshot(fetch_ts: &str) -> Snapshot {
        let provider = MockProvider {
            payload: Ok(SAMPLE_ECB_XML.to_string()),
        };
        refresh_with(&provider, ts(fetch_ts)).expect("valid fixture snapshot")
    }

    #[test]
    fn refresh_with_parses_ecb_xml_and_roundtrips_serde() {
        let now = ts("2026-11-14T10:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");
        assert_eq!(snap.provider_id, ECB_PROVIDER_ID);
        assert_eq!(snap.base_currency, "EUR");
        assert_eq!(snap.effective_date, "2026-11-13");
        assert_eq!(snap.rates.get("EUR").map(String::as_str), Some("1"));
        assert_eq!(snap.rates.get("CNY").map(String::as_str), Some("7.8000"));
        assert_eq!(snap.rates.get("JPY").map(String::as_str), Some("165.00"));
        assert!(snap.validate(now).is_ok());
        assert_eq!(snap.freshness(now), Freshness::Fresh);

        let encoded = serde_json::to_string(&snap).expect("serialize snapshot");
        let decoded: Snapshot = serde_json::from_str(&encoded).expect("deserialize snapshot");
        assert_eq!(decoded, snap);
        assert!(decoded.validate(now).is_ok());
    }

    #[test]
    fn x01_yen_ambiguity_prefers_cny_by_default_and_retains_jpy() {
        let now = ts("2026-11-14T12:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");

        let res = preview("¥2999 in usd", now, Some(&snap), Some("CNY"))
            .expect("matched currency query")
            .expect("valid conversion");
        assert_eq!(res.len(), 2);

        // Primary: CNY -> USD = 2999 * (1.08 / 7.8) = 26991 / 65 = 415.246153846... -> 415.25 USD
        let primary = &res[0];
        assert_eq!(primary.kind, "currency");
        assert_eq!(primary.normalized, "2999 CNY → USD");
        assert_eq!(primary.value["tag"], "currency");
        assert_eq!(primary.value["source"], "CNY");
        assert_eq!(primary.value["target"], "USD");
        assert_eq!(primary.value["display"], "415.25 USD");
        assert_eq!(primary.value["amount"], "415.24615385");
        assert_eq!(primary.value["rateId"], snap.id);
        assert_eq!(primary.value["stale"], false);
        assert_eq!(primary.value["freshness"], "fresh");
        assert_eq!(
            primary.provenance.as_ref().unwrap()["attribution"],
            ECB_ATTRIBUTION
        );
        assert!(
            primary
                .diagnostics
                .iter()
                .any(|d| diag_code(d) == "ambiguous_currency" && diag_severity(d) == "warning")
        );

        // Alternate: JPY -> USD = 2999 * (1.08 / 165) = 26991 / 1375 = 19.62981818... -> 19.63 USD
        let alt = &res[1];
        assert_eq!(alt.normalized, "2999 JPY → USD");
        assert_eq!(alt.value["source"], "JPY");
        assert_eq!(alt.value["display"], "19.63 USD");
        assert_eq!(alt.value["amount"], "19.62981818");
        // Both alternatives share the exact same immutable snapshot ID
        assert_eq!(alt.value["rateId"], primary.value["rateId"]);
    }

    #[test]
    fn x01_yen_ambiguity_honors_jpy_preference() {
        let now = ts("2026-11-14T12:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");

        let res = preview("¥2999 in usd", now, Some(&snap), Some("JPY"))
            .expect("matched")
            .expect("ok");
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].value["source"], "JPY");
        assert_eq!(res[1].value["source"], "CNY");
    }

    #[test]
    fn x02_explicit_iso_code_has_no_symbol_ambiguity() {
        let now = ts("2026-11-14T12:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");

        let res = preview("2999 JPY to USD", now, Some(&snap), Some("CNY"))
            .expect("matched")
            .expect("ok");
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].value["source"], "JPY");
        assert_eq!(res[0].value["target"], "USD");
        assert_eq!(res[0].value["display"], "19.63 USD");
        assert!(res[0].diagnostics.is_empty());
    }

    #[test]
    fn x03_stale_usable_and_expired_blocked_policies() {
        let snap = fixture_snapshot("2026-11-13T16:00:00Z");

        // 48h later: past 24h TTL, within 7d max_stale -> StaleUsable
        let stale_now = ts("2026-11-15T16:00:00Z");
        assert!(snap.validate(stale_now).is_ok());
        assert_eq!(snap.freshness(stale_now), Freshness::StaleUsable);

        let res = preview("100 EUR -> USD", stale_now, Some(&snap), None)
            .expect("matched")
            .expect("stale snapshot still computes offline");
        assert_eq!(res[0].value["display"], "108.00 USD");
        assert_eq!(res[0].value["stale"], true);
        assert_eq!(res[0].value["freshness"], "stale_usable");
        assert!(
            res[0]
                .diagnostics
                .iter()
                .any(|d| diag_code(d) == "rate_stale" && diag_severity(d) == "warning")
        );

        // 8 days later: past 7d max_stale -> ExpiredBlocked
        let expired_now = ts("2026-11-21T16:00:01Z");
        let err = snap.validate(expired_now).expect_err("must reject expired");
        assert_eq!(diag_code(&err), "rate_stale");
        assert_eq!(diag_severity(&err), "error");
        assert_eq!(err.parameters["freshness"], "expired_blocked");
        assert_eq!(snap.freshness(expired_now), Freshness::ExpiredBlocked);

        let preview_err = preview("100 EUR -> USD", expired_now, Some(&snap), None)
            .expect("matched")
            .expect_err("expired snapshot must block conversion");
        assert_eq!(diag_code(&preview_err), "rate_stale");
        assert_eq!(preview_err.parameters["freshness"], "expired_blocked");
    }

    #[test]
    fn x04_missing_snapshot_and_provider_failure() {
        let now = ts("2026-11-14T12:00:00Z");

        let err = preview("¥2999 in usd", now, None, None)
            .expect("currency query recognized")
            .expect_err("missing snapshot yields error");
        assert_eq!(diag_code(&err), "rate_unavailable");
        assert_eq!(err.parameters["reason"], "missing_snapshot");

        let failing_provider = MockProvider {
            payload: Err({
                let mut d = Diagnostic::error("rate_unavailable");
                d.parameters = json!({ "reason": "timeout", "provider": ECB_PROVIDER_ID });
                d
            }),
        };
        let refresh_err = refresh_with(&failing_provider, now).expect_err("provider error");
        assert_eq!(diag_code(&refresh_err), "rate_unavailable");
        assert_eq!(refresh_err.parameters["reason"], "timeout");
    }

    #[test]
    fn exact_decimal_arithmetic_avoids_binary_float_drift() {
        let now = ts("2026-11-14T12:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");

        // 0.1 + 0.2 style exact decimal: 0.30 EUR * 1.0800 = 0.324 exact -> 0.32 USD display
        let res = preview("0.30 EUR to USD", now, Some(&snap), None)
            .expect("matched")
            .expect("ok");
        assert_eq!(res[0].value["amount"], "0.324");
        assert_eq!(res[0].value["display"], "0.32 USD");

        // Half-up rounding boundary: 0.5 * 1.08 -> 0.54; 0.005 * 1.00? Test 1.005 EUR -> USD = 1.0854 -> 1.09
        let res2 = preview("1.005 EUR to USD", now, Some(&snap), None)
            .expect("matched")
            .expect("ok");
        assert_eq!(res2[0].value["amount"], "1.0854");
        assert_eq!(res2[0].value["display"], "1.09 USD");

        // JPY has 0 minor units: 100 USD -> JPY = 100 * 165 / 1.08 = 15277.7777... -> 15278 JPY
        let res3 = preview("100 USD to JPY", now, Some(&snap), None)
            .expect("matched")
            .expect("ok");
        assert_eq!(res3[0].value["display"], "15278 JPY");
        assert_eq!(res3[0].value["amount"], "15277.77777778");
    }

    #[test]
    fn non_currency_queries_return_none() {
        let now = ts("2026-11-14T12:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");

        for q in [
            "",
            "1+1",
            "3pi sqrt 2",
            "100 Mbps to MB/s",
            "5 kg in m",
            "3pm PST in beijing",
            "0xDEAD_BEEF >> 4 & 0xFFFF",
            "0.1 as bf16",
            "wallpaper-1920x1080.png",
            "https://example.com/100/usd/to/eur",
            "2999 in usd",
            "rate = 7.1222",
            "5 cup in ml",
        ] {
            assert!(
                preview(q, now, Some(&snap), None).is_none(),
                "expected None for non-currency query: {q}"
            );
        }
    }

    #[test]
    fn validates_and_rejects_tampered_snapshots_and_bad_xml() {
        let now = ts("2026-11-14T12:00:00Z");
        let base = fixture_snapshot("2026-11-14T10:00:00Z");

        let mut bad_url = base.clone();
        bad_url.source_url = "http://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml".into();
        assert!(bad_url.validate(now).is_err());

        let mut bad_rate = base.clone();
        bad_rate.rates.insert("USD".into(), "-1.08".into());
        assert!(bad_rate.validate(now).is_err());

        let mut zero_rate = base.clone();
        zero_rate.rates.insert("USD".into(), "0".into());
        assert!(zero_rate.validate(now).is_err());

        let mut future_fetch = base;
        future_fetch.fetched_at_utc = "2026-11-15T12:00:00Z".into();
        assert!(future_fetch.validate(now).is_err());

        let xxe_xml = r#"<?xml version="1.0"?><!DOCTYPE foo [<!ENTITY xxe "bad">]><Cube><Cube time='2026-11-13'><Cube currency='USD' rate='1.08'/></Cube></Cube>"#;
        assert!(parse_ecb_daily_xml(xxe_xml, now).is_err());

        let huge_xml = " ".repeat((MAX_PAYLOAD_BYTES as usize) + 1);
        assert!(parse_ecb_daily_xml(&huge_xml, now).is_err());
    }

    #[test]
    fn ambiguous_comma_separator_disclosed_and_unsupported_currency_flagged() {
        let now = ts("2026-11-14T12:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");

        let comma_err = preview("¥1,234 in usd", now, Some(&snap), None)
            .expect("recognized")
            .expect_err("ambiguous comma requires disambiguation");
        assert_eq!(diag_code(&comma_err), "invalid_syntax");
        assert_eq!(comma_err.suggestions.len(), 2);

        let twd_err = preview("100 TWD in USD", now, Some(&snap), None)
            .expect("recognized ISO currency")
            .expect_err("TWD not in ECB feed");
        assert_eq!(diag_code(&twd_err), "rate_unavailable");
        assert_eq!(twd_err.parameters["reason"], "unsupported_currency");
    }

    #[test]
    fn dollar_and_distinct_prefixes_work() {
        let now = ts("2026-11-14T12:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");

        let hkd = preview("HK$840 in eur", now, Some(&snap), None)
            .expect("matched")
            .expect("ok");
        assert_eq!(hkd.len(), 1);
        assert_eq!(hkd[0].value["source"], "HKD");
        assert_eq!(hkd[0].value["display"], "100.00 EUR");

        let usd_ambig = preview("$100 in eur", now, Some(&snap), Some("CAD"))
            .expect("matched")
            .expect("ok");
        assert_eq!(usd_ambig[0].value["source"], "CAD");
        assert!(usd_ambig.iter().any(|c| c.value["source"] == "USD"));
    }

    #[test]
    fn additional_edge_cases_spans_arrows_locales_and_xml_errors() {
        let now = ts("2026-11-14T12:00:00Z");
        let snap = fixture_snapshot("2026-11-14T10:00:00Z");

        // Fullwidth yen + locale preference "ja_JP" -> JPY primary, accurate UTF-16 spans
        let fw = preview("￥2999 in usd", now, Some(&snap), Some("ja_JP"))
            .expect("matched")
            .expect("ok");
        assert_eq!(fw.len(), 2);
        assert_eq!(fw[0].value["source"], "JPY");
        assert_eq!(
            fw[0].diagnostics[0].span,
            Some(json!({ "startUtf16": 0, "endUtf16": 1 }))
        );

        // Arrow connector without spaces and suffix symbol
        let arrow = preview("2999¥->usd", now, Some(&snap), None)
            .expect("matched")
            .expect("ok");
        assert_eq!(arrow.len(), 2);
        assert_eq!(arrow[0].value["source"], "CNY");

        // Nordic kr ambiguity
        let nordic = preview("100 kr in eur", now, Some(&snap), Some("NOK"))
            .expect("matched")
            .expect("ok");
        assert_eq!(nordic[0].value["source"], "NOK");
        assert!(nordic.iter().any(|c| c.value["source"] == "SEK"));

        // Allowlist check blocks non-ECB URL
        let bad_endpoint = fetch_allowlisted_ecb_xml("http://example.com/rates.xml")
            .expect_err("must reject non-allowlisted endpoint");
        assert_eq!(diag_code(&bad_endpoint), "rate_unavailable");
        assert_eq!(bad_endpoint.parameters["reason"], "untrusted_endpoint");

        // Reject duplicate currency or multi-date XML
        let dup_xml = r#"<Cube><Cube time='2026-11-13'><Cube currency='USD' rate='1.08'/><Cube currency='USD' rate='1.09'/></Cube></Cube>"#;
        assert!(parse_ecb_daily_xml(dup_xml, now).is_err());

        let multi_date_xml = r#"<Cube><Cube time='2026-11-13'><Cube currency='USD' rate='1.08'/></Cube><Cube time='2026-11-12'><Cube currency='USD' rate='1.07'/></Cube></Cube>"#;
        assert!(parse_ecb_daily_xml(multi_date_xml, now).is_err());
    }
}
