use serde_json::Value;
use url::Url;

use super::dto::{CompletionDto, EngineDto, InputDto, SearchSuggestionDto};

const MAX_URL_BYTES: usize = 8192;
pub(super) const DEFAULT_ENGINE_ID: &str = "duckduckgo";

pub(super) struct EngineDef {
    pub(super) id: &'static str,
    pub(super) label: &'static str,
    pub(super) keyword: &'static str,
    pub(super) hostname: &'static str,
    pub(super) domain_scope: &'static str,
    pub(super) base_url: &'static str,
    pub(super) query_param: &'static str,
}

const ENGINES: &[EngineDef] = &[
    EngineDef {
        id: "github",
        label: "GitHub",
        keyword: "gh",
        hostname: "github.com",
        domain_scope: "github.com",
        base_url: "https://github.com/search",
        query_param: "q",
    },
    EngineDef {
        id: "google",
        label: "Google",
        keyword: "g",
        hostname: "www.google.com",
        domain_scope: "google.com",
        base_url: "https://www.google.com/search",
        query_param: "q",
    },
    EngineDef {
        id: "duckduckgo",
        label: "DuckDuckGo",
        keyword: "ddg",
        hostname: "duckduckgo.com",
        domain_scope: "duckduckgo.com",
        base_url: "https://duckduckgo.com/",
        query_param: "q",
    },
    EngineDef {
        id: "wikipedia",
        label: "Wikipedia",
        keyword: "wiki",
        hostname: "en.wikipedia.org",
        domain_scope: "wikipedia.org",
        base_url: "https://en.wikipedia.org/w/index.php",
        query_param: "search",
    },
    EngineDef {
        id: "youtube",
        label: "YouTube",
        keyword: "yt",
        hostname: "www.youtube.com",
        domain_scope: "youtube.com",
        base_url: "https://www.youtube.com/results",
        query_param: "search_query",
    },
    EngineDef {
        id: "archwiki",
        label: "ArchWiki",
        keyword: "aw",
        hostname: "wiki.archlinux.org",
        domain_scope: "wiki.archlinux.org",
        base_url: "https://wiki.archlinux.org/index.php",
        query_param: "search",
    },
    EngineDef {
        id: "aur",
        label: "AUR",
        keyword: "aur",
        hostname: "aur.archlinux.org",
        domain_scope: "aur.archlinux.org",
        base_url: "https://aur.archlinux.org/packages",
        query_param: "K",
    },
    EngineDef {
        id: "archpackages",
        label: "Arch packages",
        keyword: "pkg",
        hostname: "archlinux.org",
        domain_scope: "archlinux.org",
        base_url: "https://archlinux.org/packages/",
        query_param: "q",
    },
];

const ALTERNATIVE_ENGINE_PRIORITY: &[&str] = &[
    "aur",
    "archpackages",
    "youtube",
    "archwiki",
    "github",
    "duckduckgo",
];

/// Returns whether `engine_id` is supported as the persistent default search engine.
pub fn is_supported_default_engine(engine_id: &str) -> bool {
    matches!(engine_id, "duckduckgo" | "google")
}

/// Returns the static list of supported search engines for `web.engines.list`.
pub fn engines() -> Value {
    let list: Vec<EngineDto> = ENGINES
        .iter()
        .map(|engine| EngineDto {
            engine_id: engine.id,
            label: engine.label,
            keyword: engine.keyword,
            hostname: engine.hostname,
        })
        .collect();
    serde_json::to_value(list).unwrap_or_else(|_| Value::Array(Vec::new()))
}

/// Builds a provider search URL for engine `id` and `query`.
pub fn engine_url(id: &str, query: &str) -> Option<Url> {
    let engine = find_engine_by_id(id)?;
    let mut url = Url::parse(engine.base_url).ok()?;
    url.query_pairs_mut().append_pair(engine.query_param, query);
    Some(url)
}

/// Validates and losslessly normalizes an `http`/`https` URL without credentials or controls.
pub fn validate_url(raw: &str) -> Option<Url> {
    if raw.is_empty() || raw.len() > MAX_URL_BYTES {
        return None;
    }
    if raw.chars().any(|ch| ch.is_control()) {
        return None;
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let parsed = Url::parse(trimmed).ok()?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return None,
    }
    if parsed.cannot_be_a_base() {
        return None;
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return None;
    }

    // Reject empty userinfo forms like `https://@example.com` that leave username empty.
    let scheme_prefix_len = parsed.scheme().len() + 3;
    if let Some(after_scheme) = trimmed.get(scheme_prefix_len..) {
        let authority_end = after_scheme
            .find(['/', '?', '#'])
            .unwrap_or(after_scheme.len());
        if after_scheme[..authority_end].contains('@') {
            return None;
        }
    }

    let host = parsed.host_str()?;
    if host.is_empty() {
        return None;
    }
    if parsed.as_str().len() > MAX_URL_BYTES {
        return None;
    }
    Some(parsed)
}

pub(super) struct ParsedInput {
    pub(super) kind: &'static str,
    pub(super) active_engine: Option<&'static EngineDef>,
    pub(super) site_domain: Option<String>,
    pub(super) search_text: String,
    pub(super) direct_target: Option<String>,
    pub(super) include_query_field: bool,
}

impl ParsedInput {
    pub(super) fn is_empty_non_engine(&self) -> bool {
        self.active_engine.is_none()
            && self.site_domain.is_none()
            && self.search_text.trim().is_empty()
    }

    pub(super) fn to_dto(&self, completion: Option<CompletionDto>) -> InputDto {
        InputDto {
            kind: self.kind,
            direct_navigation_target: self.direct_target.clone(),
            completion,
            engine_id: self.active_engine.map(|e| e.id),
            query: if self.include_query_field {
                Some(self.search_text.clone())
            } else {
                None
            },
        }
    }
}

pub(super) fn parse_input(raw_query: &str, explicit_engine_id: Option<&str>) -> ParsedInput {
    if let Some(eid) = explicit_engine_id
        && let Some(engine) = find_engine_by_id(eid)
    {
        let (site_domain, remainder) = extract_site_scope(raw_query.trim_start());
        return ParsedInput {
            kind: "engine",
            active_engine: Some(engine),
            site_domain,
            search_text: remainder,
            direct_target: None,
            include_query_field: true,
        };
    }

    let trimmed_start = raw_query.trim_start();
    if let Some((engine, literal_rest)) = parse_engine_keyword(trimmed_start) {
        let (site_domain, remainder) = extract_site_scope(literal_rest);
        return ParsedInput {
            kind: "engine",
            active_engine: Some(engine),
            site_domain,
            search_text: remainder,
            direct_target: None,
            include_query_field: true,
        };
    }

    let trimmed = trimmed_start.trim_end();
    let (site_domain, remainder) = extract_site_scope(trimmed);
    if site_domain.is_some() {
        return ParsedInput {
            kind: "search",
            active_engine: None,
            site_domain,
            search_text: remainder,
            direct_target: None,
            include_query_field: true,
        };
    }

    if let Some(direct) = classify_url_input(trimmed) {
        return ParsedInput {
            kind: "url",
            active_engine: None,
            site_domain: None,
            search_text: trimmed.to_owned(),
            direct_target: Some(direct),
            include_query_field: false,
        };
    }

    ParsedInput {
        kind: "search",
        active_engine: None,
        site_domain: None,
        search_text: trimmed.to_owned(),
        direct_target: None,
        include_query_field: false,
    }
}

fn extract_site_scope(raw: &str) -> (Option<String>, String) {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && raw[i..].chars().next().is_some_and(char::is_whitespace) {
            i += raw[i..].chars().next().map_or(1, char::len_utf8);
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && !raw[i..].chars().next().is_some_and(char::is_whitespace) {
            i += raw[i..].chars().next().map_or(1, char::len_utf8);
        }
        let token = &raw[start..i];
        if token.len() > 5
            && token
                .get(..5)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("site:"))
            && let Some(domain) = validate_site_domain(&token[5..])
        {
            let before = raw[..start].trim_end();
            let after = raw[i..].trim_start();
            let remainder = match (before.is_empty(), after.is_empty()) {
                (true, true) => String::new(),
                (true, false) => after.to_owned(),
                (false, true) => before.to_owned(),
                (false, false) => format!("{before} {after}"),
            };
            return (Some(domain), remainder);
        }
    }
    (None, raw.to_owned())
}

fn validate_site_domain(domain: &str) -> Option<String> {
    if domain.is_empty()
        || domain.len() > 253
        || domain.contains(['/', '?', '#', '@', ':', '\\'])
        || domain
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return None;
    }
    let parsed = Url::parse(&format!("https://{domain}")).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    if !is_valid_domain_host(&host) {
        return None;
    }
    let valid_labels = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    });
    valid_labels.then_some(host)
}

fn find_engine_by_id(id: &str) -> Option<&'static EngineDef> {
    let normalized = id.trim();
    ENGINES
        .iter()
        .find(|engine| engine.id.eq_ignore_ascii_case(normalized))
}

fn parse_engine_keyword(trimmed_start: &str) -> Option<(&'static EngineDef, &str)> {
    let ws_idx = trimmed_start.find(char::is_whitespace)?;
    let prefix = &trimmed_start[..ws_idx];
    let engine = ENGINES
        .iter()
        .find(|engine| engine.keyword.eq_ignore_ascii_case(prefix))?;
    let after_kw = &trimmed_start[ws_idx..];
    let mut chars = after_kw.char_indices();
    let (_, first_ws) = chars.next()?;
    let rest_start = first_ws.len_utf8();
    let remainder = after_kw[rest_start..].trim_start();
    Some((engine, remainder))
}

fn classify_url_input(trimmed: &str) -> Option<String> {
    if trimmed.is_empty() || trimmed.len() > MAX_URL_BYTES {
        return None;
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return None;
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return validate_url(trimmed).map(|u| u.as_str().to_owned());
    }
    if trimmed.contains("://") {
        return None;
    }

    let authority_end = trimmed.find(['/', '?', '#']).unwrap_or(trimmed.len());
    let authority = &trimmed[..authority_end];
    if authority.is_empty() || authority.contains('@') {
        return None;
    }

    let (host, has_port) = split_host_and_port(authority)?;
    let is_unambiguous_host = if (host.starts_with('[') && host.ends_with(']'))
        || host.parse::<std::net::Ipv4Addr>().is_ok()
    {
        true
    } else if host.eq_ignore_ascii_case("localhost") {
        has_port || authority_end < trimmed.len()
    } else {
        is_valid_domain_host(host)
    };

    if !is_unambiguous_host {
        return None;
    }

    let candidate = format!("https://{trimmed}");
    validate_url(&candidate).map(|u| u.as_str().to_owned())
}

fn split_host_and_port(authority: &str) -> Option<(&str, bool)> {
    if authority.starts_with('[') {
        let close = authority.find(']')?;
        let host = &authority[..=close];
        let rest = &authority[close + 1..];
        if rest.is_empty() {
            return Some((host, false));
        }
        let port_str = rest.strip_prefix(':')?;
        let port: u16 = port_str.parse().ok()?;
        if port == 0 {
            return None;
        }
        return Some((host, true));
    }

    match authority.split_once(':') {
        Some((host, port_str)) => {
            if host.is_empty() || port_str.is_empty() {
                return None;
            }
            let port: u16 = port_str.parse().ok()?;
            if port == 0 {
                return None;
            }
            Some((host, true))
        }
        None => Some((authority, false)),
    }
}

pub(super) fn is_valid_domain_host(host: &str) -> bool {
    if !host.contains('.') || host.starts_with('.') || host.ends_with('.') || host.contains("..") {
        return false;
    }
    let Some(tld) = host.rsplit('.').next() else {
        return false;
    };
    if tld.len() < 2 {
        return false;
    }
    tld.chars().all(|ch| ch.is_ascii_alphabetic()) || tld.to_ascii_lowercase().starts_with("xn--")
}

pub(super) fn host_matches_domain(host: &str, domain: &str) -> bool {
    let host_l = host.to_ascii_lowercase();
    let domain_l = domain.to_ascii_lowercase();
    if host_l == domain_l {
        return true;
    }
    host_l.len() > domain_l.len()
        && host_l.ends_with(&domain_l)
        && host_l.as_bytes()[host_l.len() - domain_l.len() - 1] == b'.'
}

pub(super) fn host_matches_url_target(candidate_host: &str, target_host: &str) -> bool {
    if is_valid_domain_host(target_host) {
        host_matches_domain(candidate_host, target_host)
    } else {
        candidate_host.eq_ignore_ascii_case(target_host)
    }
}

fn build_provider_query(site_domain: Option<&str>, search_text: &str) -> String {
    match site_domain {
        Some(domain) => {
            let trimmed = search_text.trim();
            if trimmed.is_empty() {
                format!("site:{domain}")
            } else {
                format!("site:{domain} {search_text}")
            }
        }
        None => search_text.to_owned(),
    }
}

pub(super) fn build_suggestions(
    active_engine: Option<&'static EngineDef>,
    site_domain: Option<&str>,
    search_text: &str,
    default_engine_id: &str,
) -> Vec<SearchSuggestionDto> {
    let provider_query = build_provider_query(site_domain, search_text);
    let engines_to_emit: Vec<&'static EngineDef> = match active_engine {
        Some(engine) => {
            let mut list = vec![engine];
            for alt_id in ALTERNATIVE_ENGINE_PRIORITY {
                if list.len() >= 4 {
                    break;
                }
                if *alt_id != engine.id
                    && let Some(alt) = find_engine_by_id(alt_id)
                {
                    list.push(alt);
                }
            }
            list
        }
        None => {
            if provider_query.trim().is_empty() {
                Vec::new()
            } else {
                let primary_id = if is_supported_default_engine(default_engine_id) {
                    default_engine_id
                } else {
                    DEFAULT_ENGINE_ID
                };
                let secondary_id = if site_domain.is_some() {
                    if primary_id == "google" {
                        "duckduckgo"
                    } else {
                        "google"
                    }
                } else {
                    "github"
                };
                [primary_id, secondary_id]
                    .into_iter()
                    .filter_map(find_engine_by_id)
                    .collect()
            }
        }
    };

    engines_to_emit
        .into_iter()
        .filter_map(|engine| {
            let url = engine_url(engine.id, &provider_query)?;
            let title = if provider_query.is_empty() {
                format!("Search {}", engine.label)
            } else {
                format!("Search {} for {}", engine.label, provider_query)
            };
            Some(SearchSuggestionDto {
                action: "search",
                engine_id: engine.id,
                title,
                url: url.into(),
                hostname: engine.hostname,
            })
        })
        .collect()
}
