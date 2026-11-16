use std::io::{self, BufRead};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(super) const MAX_REQUEST_BYTES: usize = 32 * 1024;
pub(super) const MAX_RESPONSE_BYTES: usize = 512 * 1024;
pub(super) const MAX_PROFILE_FILTER_LEN: usize = 64;
pub(super) const MAX_ID_BYTES: usize = 256;
pub(super) const MAX_REQ_ID_BYTES: usize = 128;
pub(super) const MAX_SEARCH_LIMIT: usize = 100;
pub(super) const DEFAULT_SEARCH_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub(super) struct ApiError {
    code: &'static str,
    message: String,
    retryable: bool,
}

impl ApiError {
    pub(super) fn new(code: &'static str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }

    pub(super) fn invalid_request(message: impl Into<String>) -> Self {
        Self::new("INVALID_REQUEST", message, false)
    }

    pub(super) fn invalid_query(message: impl Into<String>) -> Self {
        Self::new("INVALID_QUERY", message, false)
    }

    pub(super) fn invalid_url(message: impl Into<String>) -> Self {
        Self::new("INVALID_URL", message, false)
    }

    pub(super) fn profile_not_found() -> Self {
        Self::new("PROFILE_NOT_FOUND", "Browser profile was not found.", false)
    }

    pub(super) fn profile_disabled() -> Self {
        Self::new("PROFILE_DISABLED", "Browser profile is disabled.", false)
    }

    pub(super) fn profile_permission_denied() -> Self {
        Self::new(
            "PROFILE_PERMISSION_DENIED",
            "Browser profile is inaccessible.",
            false,
        )
    }

    pub(super) fn browser_not_installed() -> Self {
        Self::new(
            "BROWSER_NOT_INSTALLED",
            "Requested browser handler is not installed or runnable.",
            false,
        )
    }

    pub(super) fn operation_unsupported(message: impl Into<String>) -> Self {
        Self::new("OPERATION_UNSUPPORTED", message, false)
    }

    pub(super) fn extension_not_connected() -> Self {
        Self::new(
            "EXTENSION_NOT_CONNECTED",
            "Browser history access is not enabled for this profile.",
            false,
        )
    }

    pub(super) fn icon_not_found() -> Self {
        Self::new("ICON_NOT_FOUND", "Cached favicon was not found.", false)
    }

    pub(super) fn entry_not_found() -> Self {
        Self::new("ENTRY_NOT_FOUND", "Web index entry was not found.", false)
    }

    pub(super) fn internal(message: impl Into<String>) -> Self {
        Self::new("INTERNAL", message, false)
    }
}

pub(super) enum LineStatus {
    Ok,
    Eof,
    TooLarge,
    InvalidUtf8,
}

pub(super) fn read_bounded_line(
    reader: &mut impl BufRead,
    out: &mut String,
    max_bytes: usize,
) -> io::Result<LineStatus> {
    out.clear();
    let mut buf = Vec::new();
    let mut oversized = false;

    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            if oversized {
                return Ok(LineStatus::TooLarge);
            }
            if buf.is_empty() {
                return Ok(LineStatus::Eof);
            }
            return match String::from_utf8(buf) {
                Ok(s) => {
                    *out = s;
                    Ok(LineStatus::Ok)
                }
                Err(_) => Ok(LineStatus::InvalidUtf8),
            };
        }

        let newline = chunk.iter().position(|&b| b == b'\n');
        let take = newline.map_or(chunk.len(), |idx| idx + 1);

        if oversized {
            reader.consume(take);
            if newline.is_some() {
                return Ok(LineStatus::TooLarge);
            }
            continue;
        }

        if buf.len() + take > max_bytes {
            oversized = true;
            buf.clear();
            reader.consume(take);
            if newline.is_some() {
                return Ok(LineStatus::TooLarge);
            }
            continue;
        }

        buf.extend_from_slice(&chunk[..take]);
        reader.consume(take);
        if newline.is_some() {
            return match String::from_utf8(buf) {
                Ok(s) => {
                    *out = s;
                    Ok(LineStatus::Ok)
                }
                Err(_) => Ok(LineStatus::InvalidUtf8),
            };
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct EmptyParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetEnabledParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
    pub(super) profile_id: String,
    pub(super) enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SyncRequestParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
    #[serde(default)]
    pub(super) profile_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(super) mode: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
    #[serde(default)]
    pub(super) query: String,
    #[serde(default)]
    pub(super) engine_id: Option<String>,
    #[serde(default)]
    pub(super) profile_ids: Option<Vec<String>>,
    #[serde(default)]
    pub(super) limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ActionsAvailableParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
    pub(super) entry_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PerformTarget {
    #[serde(default)]
    pub(super) entry_id: Option<String>,
    #[serde(default)]
    pub(super) url: Option<String>,
    #[serde(default)]
    pub(super) engine_id: Option<String>,
    #[serde(default)]
    pub(super) query: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PerformParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
    pub(super) action: String,
    #[serde(default)]
    pub(super) target: Option<PerformTarget>,
    #[serde(default)]
    pub(super) browser_id: Option<String>,
    #[serde(default)]
    pub(super) profile_id: Option<String>,
    #[serde(default)]
    pub(super) private: Option<bool>,
    #[serde(default)]
    pub(super) event_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FaviconResolveParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
    pub(super) url: String,
    #[serde(default)]
    pub(super) profile_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FaviconReadParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
    pub(super) icon_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PrefsSetParams {
    #[allow(dead_code)]
    pub(super) id: Value,
    #[allow(dead_code)]
    pub(super) op: String,
    pub(super) values: Value,
}

pub(super) fn parse_params<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, ApiError> {
    serde_json::from_value(value)
        .map_err(|_| ApiError::invalid_request("Invalid request parameters."))
}

pub(super) fn is_valid_request_id(val: &Value) -> bool {
    match val {
        Value::Number(num) => num.is_i64() || num.is_u64(),
        Value::String(s) => {
            !s.is_empty()
                && s.len() <= MAX_REQ_ID_BYTES
                && !s.chars().any(|c| c.is_control() || c.is_whitespace())
        }
        _ => false,
    }
}

pub(super) fn is_valid_op_name(op: &str) -> bool {
    !op.is_empty()
        && op.len() <= 64
        && op
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

pub(super) fn validate_entity_id(id: &str, max_len: usize, field: &str) -> Result<(), ApiError> {
    if id.is_empty()
        || id.len() > max_len
        || id.contains('/')
        || id.contains('\\')
        || id.contains("..")
        || id.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(ApiError::invalid_request(format!("Malformed {field}.")));
    }
    Ok(())
}

pub(super) fn validate_slug_id(id: &str, max_len: usize, field: &str) -> Result<(), ApiError> {
    if id.is_empty()
        || id.len() > max_len
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(ApiError::invalid_request(format!("Malformed {field}.")));
    }
    Ok(())
}

pub(super) fn validate_icon_key(key: &str) -> Result<(), ApiError> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b':')
    {
        return Err(ApiError::invalid_request("Malformed iconKey."));
    }
    Ok(())
}

pub(super) fn serialize_success(id: &Value, data: Value) -> String {
    serde_json::to_string(&json!({ "id": id, "data": data })).unwrap_or_else(|_| {
        serialize_error(
            id.clone(),
            ApiError::internal("Failed to serialize response."),
        )
    })
}

pub(super) fn serialize_error(id: Value, error: ApiError) -> String {
    serde_json::to_string(&json!({ "id": id, "error": error }))
        .unwrap_or_else(|_| {
            r#"{"id":null,"error":{"code":"INTERNAL","message":"Serialization failed.","retryable":false}}"#.to_owned()
        })
}
