use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone)]
pub(super) struct IndexedRow {
    pub(super) entry_id: String,
    pub(super) profile_id: String,
    pub(super) browser_id: String,
    pub(super) browser_label: String,
    pub(super) kind: EntryKind,
    pub(super) normalized_url: String,
    pub(super) host: String,
    pub(super) origin: String,
    pub(super) title: String,
    pub(super) folder_path: Vec<String>,
    pub(super) visited_count: i64,
    pub(super) last_visited_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryKind {
    Bookmark,
    History,
}

impl EntryKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Bookmark => "bookmark",
            Self::History => "history",
        }
    }

    pub(super) fn from_db(raw: &str) -> Option<Self> {
        match raw {
            "bookmark" => Some(Self::Bookmark),
            "history" => Some(Self::History),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MatchTier {
    ExactUrl = 0,
    HostOrUrlPrefix = 1,
    TitlePrefix = 2,
    WordBoundary = 3,
    Substring = 4,
}

#[derive(Debug, Clone, Default)]
pub(super) struct OpenStats {
    pub(super) recent_count: i64,
    pub(super) last_opened_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub(super) struct ScoredRow {
    pub(super) row: IndexedRow,
    pub(super) tier: MatchTier,
    pub(super) score: f64,
    pub(super) effective_last_ms: Option<i64>,
}

pub(super) struct RankedGroupEntry {
    pub(super) row: RowDto,
    pub(super) browser_labels: Vec<(String, String)>,
    pub(super) tier: MatchTier,
    pub(super) score: f64,
    pub(super) effective_last_ms: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct EngineDto {
    pub(super) engine_id: &'static str,
    pub(super) label: &'static str,
    pub(super) keyword: &'static str,
    pub(super) hostname: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceSummaryDto {
    pub(super) browser_id: String,
    pub(super) label: String,
    pub(super) count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SourceRefDto {
    pub(super) browser_id: String,
    pub(super) profile_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FaviconDto {
    pub(super) status: &'static str,
    pub(super) fallback_text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RowDto {
    pub(super) entry_id: String,
    pub(super) title: String,
    pub(super) url: String,
    pub(super) hostname: String,
    pub(super) folder_path: Vec<String>,
    pub(super) sources: Vec<SourceRefDto>,
    pub(super) visited_count: i64,
    pub(super) last_visited_ms: Option<i64>,
    pub(super) favicon: FaviconDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FrequentSiteDto {
    pub(super) entry_id: String,
    pub(super) title: String,
    pub(super) url: String,
    pub(super) representative_url: String,
    pub(super) origin: String,
    pub(super) hostname: String,
    pub(super) folder_path: Vec<String>,
    pub(super) sources: Vec<SourceRefDto>,
    pub(super) visited_count: i64,
    pub(super) last_visited_ms: Option<i64>,
    pub(super) favicon: FaviconDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchSuggestionDto {
    pub(super) action: &'static str,
    pub(super) engine_id: &'static str,
    pub(super) title: String,
    pub(super) url: String,
    pub(super) hostname: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CompletionDto {
    pub(super) suggested_url: String,
    pub(super) source_entry_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InputDto {
    pub(super) kind: &'static str,
    pub(super) direct_navigation_target: Option<String>,
    pub(super) completion: Option<CompletionDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) engine_id: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) query: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SyncDto {
    pub(super) stale: bool,
    pub(super) enabled_profile_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SearchResponseDto {
    pub(super) matched_count: usize,
    pub(super) source_summary: Vec<SourceSummaryDto>,
    pub(super) groups: Vec<Value>,
    pub(super) frequent_sites: Vec<FrequentSiteDto>,
    pub(super) recent: Vec<RowDto>,
    pub(super) input: InputDto,
    pub(super) sync: SyncDto,
}
