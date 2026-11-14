use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::programmer::ProgrammerSettings;

pub const VERSION: u8 = 1;
pub const FRAME_LIMIT: usize = 64 * 1024;

#[derive(Deserialize)]
#[serde(
    tag = "op",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum Operation {
    Hello {},
    Status {},
    Preview {
        query: String,
        revision: u64,
        scope: Scope,
        #[serde(default)]
        settings: Option<ProgrammerSettings>,
        #[serde(default)]
        cursor_utf16: Option<usize>,
    },
    Cancel {
        revision: u64,
    },
    Commit {
        candidate_id: String,
        revision: u64,
        idempotency_key: String,
        #[serde(default)]
        format_id: Option<String>,
        #[serde(default)]
        remember_choice: bool,
    },
    CatalogSearch {
        query: String,
        limit: usize,
        revision: u64,
    },
    Complete {
        query: String,
        cursor_utf16: usize,
        revision: u64,
    },
    HistoryList {
        #[serde(default)]
        limit: Option<u32>,
        #[serde(default)]
        before: Option<u64>,
    },
    HistoryDelete {
        history_id: String,
        idempotency_key: String,
    },
    HistoryPin {
        history_id: String,
        pinned: bool,
        idempotency_key: String,
    },
    HistoryClear {
        #[serde(default)]
        include_pinned: bool,
        idempotency_key: String,
    },
    SaveVariable {
        name: String,
        expression: String,
        idempotency_key: String,
    },
    VariablesList {},
    VariableDelete {
        name: String,
        idempotency_key: String,
    },
    PreferencesSet {
        key: String,
        value: Value,
        idempotency_key: String,
    },
    FxRefresh {
        idempotency_key: String,
    },
    Shutdown {},
}

impl Operation {
    pub(crate) fn revision(&self) -> Option<u64> {
        match self {
            Self::Preview { revision, .. }
            | Self::Cancel { revision }
            | Self::Commit { revision, .. }
            | Self::CatalogSearch { revision, .. }
            | Self::Complete { revision, .. } => Some(*revision),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    All,
    Calc,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub kind: String,
    pub normalized: String,
    pub interpretation: String,
    pub assumptions: Vec<String>,
    pub value: Value,
    pub formats: Vec<Format>,
    pub diagnostics: Vec<Diagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Value>,
}

impl Candidate {
    pub(crate) fn new(
        id: impl Into<String>,
        kind: &str,
        normalized: &str,
        interpretation: &str,
        value: Value,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            normalized: normalized.into(),
            interpretation: interpretation.into(),
            assumptions: Vec::new(),
            value,
            formats: Vec::new(),
            diagnostics: Vec::new(),
            provenance: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Format {
    pub id: String,
    pub label: String,
    pub copy_text: String,
}

impl Format {
    pub(crate) fn new(id: &str, label: impl Into<String>, copy_text: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            copy_text: copy_text.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Diagnostic(Box<DiagnosticDetails>);

impl std::ops::Deref for Diagnostic {
    type Target = DiagnosticDetails;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Diagnostic {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticDetails {
    pub code: String,
    pub severity: String,
    pub message_key: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub parameters: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Value>,
    pub suggestions: Vec<Value>,
}

impl Diagnostic {
    fn new(code: &str, severity: &str) -> Self {
        Self(Box::new(DiagnosticDetails {
            code: code.into(),
            severity: severity.into(),
            message_key: format!("calc.error.{code}"),
            parameters: Value::Null,
            span: None,
            suggestions: Vec::new(),
        }))
    }

    pub(crate) fn error(code: &str) -> Self {
        Self::new(code, "error")
    }

    pub(crate) fn warning(code: &str) -> Self {
        Self::new(code, "warning")
    }
}

pub fn response(
    id: Option<&str>,
    revision: Option<u64>,
    result: Result<Value, Diagnostic>,
) -> Value {
    let mut output = match result {
        Ok(data) => json!({"v": VERSION, "id": id, "ok": true, "data": data}),
        Err(error) => json!({"v": VERSION, "id": id, "ok": false, "error": error}),
    };
    if let Some(revision) = revision {
        output["revision"] = json!(revision);
    }
    output
}
