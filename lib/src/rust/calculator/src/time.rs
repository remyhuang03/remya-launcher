use crate::protocol::{Candidate, Diagnostic, Format};
use jiff::civil::{Date, DateTime, Time};
use jiff::tz::{AmbiguousOffset, Offset, TimeZone};
use jiff::{Timestamp, Zoned};
use serde_json::json;

const TZDB_VERSION: &str = "2026e";

#[derive(Clone, Debug)]
enum ZoneSpec {
    Iana {
        id: String,
        tz: TimeZone,
        alias_from: Option<String>,
    },
    Fixed {
        id: String,
        offset: Offset,
    },
}

impl ZoneSpec {
    fn id(&self) -> &str {
        match self {
            Self::Iana { id, .. } | Self::Fixed { id, .. } => id,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Iana { .. } => "iana",
            Self::Fixed { .. } => "fixed_offset",
        }
    }

    fn time_zone(&self) -> TimeZone {
        match self {
            Self::Iana { tz, .. } => tz.clone(),
            Self::Fixed { offset, .. } => TimeZone::fixed(*offset),
        }
    }
}

pub(crate) fn preview(
    query: &str,
    now: Timestamp,
    pinned: &[String],
) -> Option<Result<Vec<Candidate>, Diagnostic>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (lhs, rhs) = split_conversion(trimmed);
    let target = match rhs {
        Some(r) => resolve_single_zone(r),
        None => None,
    };
    if rhs.is_some() && target.is_none() {
        return None;
    }

    let parsed = parse_lhs(lhs)?;
    let source_zones = parsed.zones;
    let target_zone = target.unwrap_or_else(|| ZoneSpec::Fixed {
        id: "UTC".into(),
        offset: Offset::UTC,
    });

    let is_ambiguous_abbrev = source_zones.len() > 1;
    let mut candidates = Vec::new();

    for (z_idx, src) in source_zones.iter().enumerate() {
        let src_tz = src.time_zone();
        let (date_val, time_val, date_assumed) = match (parsed.date, parsed.time) {
            (Some(d), Some(t)) => (d, t, false),
            (None, Some(t)) => (now.to_zoned(src_tz.clone()).date(), t, true),
            (Some(d), None) => (d, Time::midnight(), false),
            (None, None) => {
                let z = now.to_zoned(src_tz.clone());
                (z.date(), z.time(), true)
            }
        };

        let civil = date_val.to_datetime(time_val);
        let instants = match resolve_civil(&src_tz, civil, src.kind()) {
            Ok(instants) => instants,
            Err(error) => return Some(Err(error)),
        };

        for (i_idx, (zoned_src, dst_res, mut diags)) in instants.into_iter().enumerate() {
            let instant = zoned_src.timestamp();
            let zoned_tgt = instant.to_zoned(target_zone.time_zone());

            let mut assumptions = Vec::new();
            if is_ambiguous_abbrev {
                let abbrev = parsed.raw_zone.as_deref().unwrap_or("");
                let mut d = Diagnostic::warning("ambiguous_timezone");
                d.parameters = json!({
                    "abbreviation": abbrev,
                    "resolvedZone": src.id(),
                    "offsetSeconds": zoned_src.offset().seconds(),
                });
                diags.push(d);
                assumptions.push(format!(
                    "{abbrev} interpreted as {} ({})",
                    src.id(),
                    format_offset(zoned_src.offset())
                ));
            } else if let ZoneSpec::Fixed { id, offset } = src
                && id != "UTC"
                && id != "GMT"
                && id != "Z"
            {
                assumptions.push(format!(
                    "{id} resolved as fixed offset {}",
                    format_offset(*offset)
                ));
            }
            if let ZoneSpec::Iana {
                id,
                alias_from: Some(alias),
                ..
            } = src
            {
                assumptions.push(format!("City alias '{alias}' resolved to {id}"));
            }
            if let ZoneSpec::Iana {
                id,
                alias_from: Some(alias),
                ..
            } = &target_zone
            {
                assumptions.push(format!("City alias '{alias}' resolved to {id}"));
            }
            if date_assumed && parsed.time.is_some() {
                assumptions.push(format!(
                    "Source date {} assumed from source-local clock",
                    date_val
                ));
            }

            let src_civil_str = format_civil(zoned_src.datetime());
            let tgt_civil_str = format_civil(zoned_tgt.datetime());
            let src_hm = format_hm(zoned_src.datetime());
            let tgt_hm = format_hm(zoned_tgt.datetime());

            let normalized = format!("{} {} → {}", src_hm, src.id(), target_zone.id());
            let interpretation = format!(
                "{src_hm} {} ({}) → {tgt_hm} {} ({})",
                src.id(),
                format_offset(zoned_src.offset()),
                target_zone.id(),
                format_offset(zoned_tgt.offset()),
            );

            let value = json!({
                "tag": "time",
                "instantUtc": instant.to_string(),
                "sourceCivil": src_civil_str,
                "sourceZoneId": src.id(),
                "sourceOffsetSeconds": zoned_src.offset().seconds(),
                "sourceZoneKind": src.kind(),
                "targetCivil": tgt_civil_str,
                "targetZoneId": target_zone.id(),
                "targetOffsetSeconds": zoned_tgt.offset().seconds(),
                "dateAssumption": if date_assumed { "assumed_source_local" } else { "explicit" },
                "dstResolution": dst_res,
                "tzdbVersion": TZDB_VERSION,
            });

            let iso = zoned_tgt.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string();
            let unix = instant.as_second().to_string();
            let mut formats = vec![
                Format::new(
                    "target",
                    format!("{tgt_hm} {}", target_zone.id()),
                    format!("{} {}", tgt_civil_str.replace('T', " "), target_zone.id()),
                ),
                Format::new("iso", &iso, &iso),
                Format::new("unix", format!("Unix {unix}"), unix),
            ];
            for p in pinned {
                if let Some(pz) = resolve_single_zone(p) {
                    let zp = instant.to_zoned(pz.time_zone());
                    let p_hm = format_hm(zp.datetime());
                    formats.push(Format::new(
                        &format!("pinned:{}", pz.id()),
                        format!("{p_hm} {} ({})", pz.id(), format_offset(zp.offset())),
                        format!(
                            "{} {}",
                            format_civil(zp.datetime()).replace('T', " "),
                            pz.id()
                        ),
                    ));
                }
            }

            let id = format!("time-{}-{}", z_idx + 1, i_idx + 1);
            let mut cand =
                Candidate::new(id, "time_conversion", &normalized, &interpretation, value);
            cand.formats = formats;
            cand.assumptions = assumptions;
            cand.diagnostics = diags;
            cand.provenance = Some(json!({
                "kind": "jiff_tzdb",
                "tzdbVersion": TZDB_VERSION,
            }));
            candidates.push(cand);
        }
    }

    if candidates.is_empty() {
        None
    } else {
        Some(Ok(candidates))
    }
}

type Resolution = (Zoned, &'static str, Vec<Diagnostic>);

fn resolve_civil(
    tz: &TimeZone,
    civil: DateTime,
    zone_kind: &str,
) -> Result<Vec<Resolution>, Diagnostic> {
    let range = |_| Diagnostic::error("numeric_overflow");
    let ambiguous = tz.to_ambiguous_zoned(civil);
    match ambiguous.offset() {
        AmbiguousOffset::Unambiguous { .. } => {
            let zoned = ambiguous.unambiguous().map_err(range)?;
            let resolution = if zone_kind == "fixed_offset" {
                "fixed_offset"
            } else {
                "unambiguous"
            };
            Ok(vec![(zoned, resolution, Vec::new())])
        }
        AmbiguousOffset::Gap { before, after } => {
            let mut diagnostic = Diagnostic::error("nonexistent_local_time");
            diagnostic.parameters = json!({
                "localDateTime": format_civil(civil),
                "zone": tz.iana_name(),
                "beforeOffsetSeconds": before.seconds(),
                "afterOffsetSeconds": after.seconds(),
            });
            for (id, zoned) in [
                ("earlier", tz.to_ambiguous_zoned(civil).earlier()),
                ("later", tz.to_ambiguous_zoned(civil).later()),
            ] {
                if let Ok(zoned) = zoned {
                    let text = format!(
                        "{} {}",
                        format_civil(zoned.datetime()),
                        tz.iana_name().unwrap_or("UTC")
                    );
                    diagnostic.suggestions.push(
                        json!({"id":id,"label":text,"instantUtc":zoned.timestamp().to_string()}),
                    );
                }
            }
            Err(diagnostic)
        }
        AmbiguousOffset::Fold { before, after } => {
            let earlier = tz.to_ambiguous_zoned(civil).earlier().map_err(range)?;
            let later = ambiguous.later().map_err(range)?;
            let diagnostic = |selected: Offset| {
                let mut diagnostic = Diagnostic::warning("ambiguous_local_time");
                diagnostic.parameters = json!({
                    "localDateTime": format_civil(civil),
                    "zone": tz.iana_name(),
                    "earlierOffsetSeconds": before.seconds(),
                    "laterOffsetSeconds": after.seconds(),
                    "selectedOffsetSeconds": selected.seconds(),
                });
                diagnostic
            };
            Ok(vec![
                (earlier, "fold_earlier", vec![diagnostic(before)]),
                (later, "fold_later", vec![diagnostic(after)]),
            ])
        }
    }
}

struct ParsedLhs {
    date: Option<Date>,
    time: Option<Time>,
    zones: Vec<ZoneSpec>,
    raw_zone: Option<String>,
}

fn split_conversion(s: &str) -> (&str, Option<&str>) {
    for sep in [" -> ", " → "] {
        if let Some(pos) = s.rfind(sep) {
            return (s[..pos].trim(), Some(s[pos + sep.len()..].trim()));
        }
    }
    let lower = s.to_ascii_lowercase();
    for sep in [" in ", " to "] {
        if let Some(pos) = lower.rfind(sep) {
            return (s[..pos].trim(), Some(s[pos + sep.len()..].trim()));
        }
    }
    (s, None)
}

fn parse_lhs(lhs: &str) -> Option<ParsedLhs> {
    let tokens: Vec<&str> = lhs.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let mut idx = 0;
    let mut date = None;
    let mut time = None;

    if let Ok(d) = tokens[idx].parse::<Date>() {
        date = Some(d);
        idx += 1;
    }

    if idx < tokens.len() {
        if tokens[idx].eq_ignore_ascii_case("now") {
            idx += 1;
        } else if idx + 1 < tokens.len()
            && (tokens[idx + 1].eq_ignore_ascii_case("am")
                || tokens[idx + 1].eq_ignore_ascii_case("pm"))
        {
            let combined = format!("{}{}", tokens[idx], tokens[idx + 1]);
            if let Some(t) = parse_time_token(&combined) {
                time = Some(t);
                idx += 2;
            }
        } else if let Some(t) = parse_time_token(tokens[idx]) {
            time = Some(t);
            idx += 1;
        }
    }

    let zone_str = tokens[idx..].join(" ");
    if zone_str.is_empty() {
        if tokens.len() == 1 && tokens[0].eq_ignore_ascii_case("now") {
            return Some(ParsedLhs {
                date: None,
                time: None,
                zones: vec![ZoneSpec::Fixed {
                    id: "UTC".into(),
                    offset: Offset::UTC,
                }],
                raw_zone: Some("UTC".into()),
            });
        }
        return None;
    }

    let zones = resolve_zones(&zone_str);
    if zones.is_empty() {
        return None;
    }

    Some(ParsedLhs {
        date,
        time,
        zones,
        raw_zone: Some(zone_str),
    })
}

fn parse_time_token(tok: &str) -> Option<Time> {
    let lower = tok.to_ascii_lowercase();
    let (core, is_pm, has_meridiem) = if let Some(s) = lower.strip_suffix("pm") {
        (s, true, true)
    } else if let Some(s) = lower.strip_suffix("am") {
        (s, false, true)
    } else {
        (lower.as_str(), false, false)
    };

    if has_meridiem {
        let mut parts = core.split(':');
        let h: i8 = parts.next()?.parse().ok()?;
        if !(1..=12).contains(&h) {
            return None;
        }
        let m: i8 = match parts.next() {
            Some(p) => p.parse().ok()?,
            None => 0,
        };
        let sec: i8 = match parts.next() {
            Some(p) => p.parse().ok()?,
            None => 0,
        };
        if parts.next().is_some() || !(0..=59).contains(&m) || !(0..=59).contains(&sec) {
            return None;
        }
        let hour24 = match (h, is_pm) {
            (12, false) => 0,
            (12, true) => 12,
            (h, true) => h + 12,
            (h, false) => h,
        };
        return Time::new(hour24, m, sec, 0).ok();
    }

    if !core.contains(':') {
        return None;
    }
    let mut parts = core.split(':');
    let h: i8 = parts.next()?.parse().ok()?;
    let m: i8 = parts.next()?.parse().ok()?;
    let sec: i8 = match parts.next() {
        Some(p) => p.parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Time::new(h, m, sec, 0).ok()
}

fn resolve_single_zone(raw: &str) -> Option<ZoneSpec> {
    let mut list = resolve_zones(raw);
    if list.len() == 1 { list.pop() } else { None }
}

fn resolve_zones(raw: &str) -> Vec<ZoneSpec> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let upper = trimmed.to_ascii_uppercase();

    // Ambiguous abbreviations (T06)
    if upper == "CST" {
        return vec![
            ZoneSpec::Fixed {
                id: "Asia/Shanghai".into(),
                offset: Offset::from_seconds(8 * 3600).unwrap(),
            },
            ZoneSpec::Fixed {
                id: "America/Chicago".into(),
                offset: Offset::from_seconds(-6 * 3600).unwrap(),
            },
            ZoneSpec::Fixed {
                id: "America/Havana".into(),
                offset: Offset::from_seconds(-5 * 3600).unwrap(),
            },
        ];
    }
    if upper == "IST" {
        return vec![
            ZoneSpec::Fixed {
                id: "Asia/Kolkata".into(),
                offset: Offset::from_seconds(5 * 3600 + 1800).unwrap(),
            },
            ZoneSpec::Fixed {
                id: "Europe/Dublin".into(),
                offset: Offset::from_seconds(3600).unwrap(),
            },
            ZoneSpec::Fixed {
                id: "Asia/Jerusalem".into(),
                offset: Offset::from_seconds(2 * 3600).unwrap(),
            },
        ];
    }

    // Fixed standard/daylight abbreviations
    let fixed_hours_mins: Option<(&str, i32)> = match upper.as_str() {
        "UTC" | "GMT" | "Z" => Some((upper.as_str(), 0)),
        "PST" => Some(("PST", -8 * 3600)),
        "PDT" => Some(("PDT", -7 * 3600)),
        "MST" => Some(("MST", -7 * 3600)),
        "MDT" => Some(("MDT", -6 * 3600)),
        "CDT" => Some(("CDT", -5 * 3600)),
        "EST" => Some(("EST", -5 * 3600)),
        "EDT" => Some(("EDT", -4 * 3600)),
        "JST" => Some(("JST", 9 * 3600)),
        "KST" => Some(("KST", 9 * 3600)),
        "HKT" => Some(("HKT", 8 * 3600)),
        "SGT" => Some(("SGT", 8 * 3600)),
        "CET" => Some(("CET", 3600)),
        "CEST" => Some(("CEST", 2 * 3600)),
        _ => None,
    };
    if let Some((id, secs)) = fixed_hours_mins {
        return vec![ZoneSpec::Fixed {
            id: id.into(),
            offset: Offset::from_seconds(secs).unwrap(),
        }];
    }

    // Explicit fixed offsets like GMT+8, UTC+08:00, +08:00
    if let Some(off) = parse_explicit_offset(&upper) {
        return vec![ZoneSpec::Fixed {
            id: format!("UTC{}", format_offset(off).trim_start_matches("UTC")),
            offset: off,
        }];
    }

    // Curated city alias map
    let lower = trimmed.to_ascii_lowercase();
    let alias_iana = match lower.as_str() {
        "beijing" | "shanghai" => Some("Asia/Shanghai"),
        "tokyo" => Some("Asia/Tokyo"),
        "london" => Some("Europe/London"),
        "new york" | "nyc" => Some("America/New_York"),
        "los angeles" | "la" | "san francisco" | "sf" | "seattle" | "pacific time" => {
            Some("America/Los_Angeles")
        }
        "chicago" => Some("America/Chicago"),
        "paris" => Some("Europe/Paris"),
        "berlin" => Some("Europe/Berlin"),
        "sydney" => Some("Australia/Sydney"),
        "singapore" => Some("Asia/Singapore"),
        "hong kong" | "hongkong" => Some("Asia/Hong_Kong"),
        "seoul" => Some("Asia/Seoul"),
        "taipei" => Some("Asia/Taipei"),
        "mumbai" | "delhi" => Some("Asia/Kolkata"),
        "dubai" => Some("Asia/Dubai"),
        "toronto" => Some("America/Toronto"),
        "vancouver" => Some("America/Vancouver"),
        _ => None,
    };
    if let Some(iana) = alias_iana
        && let Ok(tz) = TimeZone::get(iana)
    {
        return vec![ZoneSpec::Iana {
            id: tz.iana_name().unwrap_or(iana).into(),
            tz,
            alias_from: Some(lower),
        }];
    }

    // Direct IANA lookup
    if let Ok(tz) = TimeZone::get(trimmed) {
        let id = tz.iana_name().unwrap_or(trimmed).to_string();
        return vec![ZoneSpec::Iana {
            id,
            tz,
            alias_from: None,
        }];
    }

    Vec::new()
}

fn parse_explicit_offset(upper: &str) -> Option<Offset> {
    let rest = upper
        .strip_prefix("UTC")
        .or_else(|| upper.strip_prefix("GMT"))
        .unwrap_or(upper);
    let (sign, digits) = if let Some(d) = rest.strip_prefix('+') {
        (1, d)
    } else {
        let d = rest.strip_prefix('-')?;
        (-1, d)
    };
    if digits.is_empty() {
        return None;
    }
    let (h, m): (i32, i32) = if let Some((hs, ms)) = digits.split_once(':') {
        (hs.parse().ok()?, ms.parse().ok()?)
    } else if digits.len() == 4 && digits.chars().all(|c| c.is_ascii_digit()) {
        (digits[..2].parse().ok()?, digits[2..].parse().ok()?)
    } else {
        (digits.parse().ok()?, 0)
    };
    if !(0..=18).contains(&h) || !(0..=59).contains(&m) {
        return None;
    }
    Offset::from_seconds(sign * (h * 3600 + m * 60)).ok()
}

fn format_offset(offset: Offset) -> String {
    let total = offset.seconds();
    let sign = if total >= 0 { '+' } else { '-' };
    let abs = total.abs();
    let h = abs / 3600;
    let m = (abs % 3600) / 60;
    format!("UTC{sign}{h:02}:{m:02}")
}

fn format_civil(dt: DateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

fn format_hm(dt: DateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(s: &str) -> Timestamp {
        s.parse().expect("valid timestamp")
    }

    fn pinned_default() -> Vec<String> {
        vec!["Asia/Tokyo".into(), "Europe/London".into(), "UTC".into()]
    }

    #[test]
    fn t01_pst_to_beijing_and_pinned() {
        let now = ts("2026-11-15T06:00:00Z");
        let res = preview("2026-11-14 3pm PST in beijing", now, &pinned_default())
            .expect("recognized")
            .expect("ok");
        assert_eq!(res.len(), 1);
        let c = &res[0];
        assert_eq!(c.value["tag"], "time");
        assert_eq!(c.value["sourceZoneId"], "PST");
        assert_eq!(c.value["sourceZoneKind"], "fixed_offset");
        assert_eq!(c.value["sourceOffsetSeconds"], -28800);
        assert_eq!(c.value["targetZoneId"], "Asia/Shanghai");
        assert_eq!(c.value["targetCivil"], "2026-11-15T07:00:00");
        assert_eq!(c.value["dateAssumption"], "explicit");
        assert_eq!(c.value["tzdbVersion"], "2026e");
        let format = |id| c.formats.iter().find(|format| format.id == id).unwrap();
        assert_eq!(format("iso").copy_text, "2026-11-15T07:00:00+08:00");
        assert_eq!(format("unix").copy_text, "1794697200");
        assert!(
            format("pinned:Asia/Tokyo")
                .copy_text
                .contains("2026-11-15 08:00")
        );
        assert!(format("pinned:UTC").copy_text.contains("2026-11-14 23:00"));
    }

    #[test]
    fn t01_omitted_date_uses_source_local_date() {
        // 2026-11-15T06:00:00Z is 2026-11-15 14:00 in Beijing, but 2026-11-14 22:00 in PST
        let now = ts("2026-11-15T06:00:00Z");
        let res = preview("3pm PST in beijing", now, &[])
            .expect("recognized")
            .expect("ok");
        assert_eq!(res[0].value["sourceCivil"], "2026-11-14T15:00:00");
        assert_eq!(res[0].value["targetCivil"], "2026-11-15T07:00:00");
        assert_eq!(res[0].value["dateAssumption"], "assumed_source_local");
    }

    #[test]
    fn t02_summer_iana_uses_pdt() {
        let now = ts("2024-07-15T12:00:00Z");
        let res = preview(
            "2024-07-15 15:00 America/Los_Angeles to Asia/Shanghai",
            now,
            &[],
        )
        .unwrap()
        .unwrap();
        assert_eq!(res[0].value["sourceZoneKind"], "iana");
        assert_eq!(res[0].value["sourceOffsetSeconds"], -25200); // UTC-07:00
        assert_eq!(res[0].value["targetCivil"], "2024-07-16T06:00:00");
    }

    #[test]
    fn t03_winter_iana_uses_pst() {
        let now = ts("2024-01-15T12:00:00Z");
        let res = preview(
            "2024-01-15 15:00 America/Los_Angeles to Asia/Shanghai",
            now,
            &[],
        )
        .unwrap()
        .unwrap();
        assert_eq!(res[0].value["sourceOffsetSeconds"], -28800); // UTC-08:00
        assert_eq!(res[0].value["targetCivil"], "2024-01-16T07:00:00");
    }

    #[test]
    fn t04_dst_gap_nonexistent_local_time() {
        let now = ts("2024-03-10T12:00:00Z");
        let result = preview("2024-03-10 02:30 America/New_York", now, &[])
            .unwrap()
            .unwrap_err();
        assert_eq!(result.code, "nonexistent_local_time");
        assert_eq!(result.suggestions.len(), 2);
        let extreme = preview("9999-12-31 23:59 UTC-23:00 to UTC", now, &[]);
        assert!(extreme.is_none_or(|result| result.is_err()));
    }

    #[test]
    fn t05_dst_fold_ambiguous_local_time() {
        let now = ts("2024-11-03T12:00:00Z");
        let res = preview("2024-11-03 01:30 America/New_York", now, &[])
            .unwrap()
            .unwrap();
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].value["dstResolution"], "fold_earlier");
        assert_eq!(res[0].value["sourceOffsetSeconds"], -14400);
        assert_eq!(res[1].value["dstResolution"], "fold_later");
        assert_eq!(res[1].value["sourceOffsetSeconds"], -18000);
        assert_eq!(res[0].diagnostics[0].code, "ambiguous_local_time");
        assert_eq!(res[1].diagnostics[0].code, "ambiguous_local_time");
    }

    #[test]
    fn t06_cst_and_ist_ambiguity() {
        let now = ts("2026-11-14T12:00:00Z");
        let cst = preview("CST", now, &[]).unwrap().unwrap();
        assert_eq!(cst.len(), 3);
        assert!(
            cst.iter()
                .all(|c| c.diagnostics[0].code == "ambiguous_timezone")
        );
        assert_eq!(cst[0].value["sourceZoneId"], "Asia/Shanghai");
        assert_eq!(cst[1].value["sourceZoneId"], "America/Chicago");
        assert_eq!(cst[2].value["sourceZoneId"], "America/Havana");

        let ist = preview("IST", now, &[]).unwrap().unwrap();
        assert_eq!(ist.len(), 3);
        assert_eq!(ist[0].value["sourceZoneId"], "Asia/Kolkata");
        assert_eq!(ist[1].value["sourceZoneId"], "Europe/Dublin");
        assert_eq!(ist[2].value["sourceZoneId"], "Asia/Jerusalem");
    }

    #[test]
    fn non_time_queries_return_none() {
        let now = ts("2026-11-14T12:00:00Z");
        assert!(preview("1+1", now, &[]).is_none());
        assert!(preview("5 kg in m", now, &[]).is_none());
        assert!(preview("100 Mbps to MB/s", now, &[]).is_none());
    }
}
