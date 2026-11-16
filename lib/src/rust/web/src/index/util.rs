use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn monogram_for(title: &str, host: &str) -> String {
    if let Some(ch) = title.trim().chars().find(|c| c.is_alphanumeric()) {
        return ch.to_uppercase().collect();
    }
    let clean_host = host.strip_prefix("www.").unwrap_or(host);
    if let Some(ch) = clean_host.chars().find(|c| c.is_alphanumeric()) {
        return ch.to_uppercase().collect();
    }
    "W".to_owned()
}

pub(super) fn sanitize_text(raw: &str) -> String {
    raw.chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>()
        .trim()
        .to_owned()
}

pub(super) fn truncate_utf8(raw: &str, max_bytes: usize) -> &str {
    if raw.len() <= max_bytes {
        return raw;
    }
    let mut end = max_bytes;
    while end > 0 && !raw.is_char_boundary(end) {
        end -= 1;
    }
    &raw[..end]
}

pub(super) fn title_case_first(raw: &str) -> String {
    let mut chars = raw.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().collect();
    out.push_str(chars.as_str());
    out
}

pub(super) fn max_opt_i64(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x.max(y)),
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (None, None) => None,
    }
}

pub(super) fn current_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
