use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct RequestEnvelope {
    pub id: Value,
    #[serde(flatten)]
    pub op: Operation,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Operation {
    Search {
        #[serde(default)]
        query: String,
        #[serde(default = "default_true")]
        packages: bool,
    },
    Perform {
        action: String,
        #[serde(default, rename = "appId")]
        app_id: Option<String>,
        #[serde(default)]
        query: String,
        #[serde(default)]
        target: Option<String>,
        #[serde(default, rename = "packageId")]
        package_id: Option<String>,
        #[serde(default)]
        source: Option<String>,
    },
    Preferences {
        #[serde(default)]
        values: Option<PreferencesPatch>,
    },
    Hidden {},
    Windows {
        #[serde(default)]
        query: String,
    },
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    #[serde(default = "default_frequent_days")]
    pub frequent_days: u32,
    #[serde(default = "default_true")]
    pub pinyin: bool,
    #[serde(default)]
    pub terminal: String,
    #[serde(default = "default_true")]
    pub show_packages: bool,
    #[serde(default = "default_true")]
    pub official: bool,
    #[serde(default = "default_true")]
    pub flatpak: bool,
    #[serde(default = "default_true")]
    pub aur: bool,
}

const fn default_frequent_days() -> u32 {
    30
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            frequent_days: default_frequent_days(),
            pinyin: true,
            terminal: String::new(),
            show_packages: true,
            official: true,
            flatpak: true,
            aur: true,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreferencesPatch {
    pub frequent_days: Option<i64>,
    pub pinyin: Option<bool>,
    pub terminal: Option<String>,
    pub show_packages: Option<bool>,
    pub official: Option<bool>,
    pub flatpak: Option<bool>,
    pub aur: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppWindow {
    pub id: String,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowEntry {
    pub id: String,
    pub title: String,
    pub class: String,
    pub workspace: String,
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppAction {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: Option<String>,
    pub source: Option<String>,
    pub terminal: bool,
    pub windows: Vec<AppWindow>,
    pub actions: Vec<AppAction>,
    pub match_kind: Option<String>,
    pub match_text: Option<String>,
    pub action_id: Option<String>,
    pub pinned: bool,
    pub hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchData {
    pub total: usize,
    pub matched: usize,
    pub apps: Vec<AppEntry>,
    pub frequent: Vec<AppEntry>,
    pub pinned: Vec<AppEntry>,
    pub packages: Vec<PackageEntry>,
    pub hidden: bool,
}

#[derive(Debug, Serialize)]
pub struct SuccessResponse<T: Serialize> {
    pub id: Value,
    pub data: T,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub id: Value,
    pub error: String,
}

pub fn serialize_result<T: Serialize>(id: Value, result: Result<T, String>) -> String {
    match result {
        Ok(data) => serde_json::to_string(&SuccessResponse { id, data }).unwrap_or_else(|e| {
            serde_json::to_string(&ErrorResponse {
                id: Value::Null,
                error: format!("Serialization error: {e}"),
            })
            .expect("error response serializes")
        }),
        Err(error) => {
            serde_json::to_string(&ErrorResponse { id, error }).expect("error response serializes")
        }
    }
}
