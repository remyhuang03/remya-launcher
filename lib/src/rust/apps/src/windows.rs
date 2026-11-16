use crate::desktop::IndexedApp;
use crate::hyprland::{HyprClient, match_windows};
use crate::protocol::WindowEntry;
use crate::search::subsequence_score;

/// Lists open windows matching `query` by title, class or app name.
/// Clients arrive in MRU order, which the stable sort keeps within a tier.
pub fn list_windows(apps: &[IndexedApp], clients: &[HyprClient], query: &str) -> Vec<WindowEntry> {
    let query = query.trim().to_lowercase();
    let mut ranked: Vec<(u8, WindowEntry)> = clients
        .iter()
        .filter_map(|client| {
            let app = apps.iter().find(|app| {
                !match_windows(
                    app.startup_wm_class.as_deref(),
                    &app.id,
                    std::slice::from_ref(client),
                )
                .is_empty()
            });
            let entry = WindowEntry {
                id: client.address.clone(),
                title: client.title.clone(),
                class: client.class.clone(),
                workspace: client.workspace.clone(),
                app_id: app.map(|app| app.id.clone()),
                app_name: app.map(|app| app.name.clone()),
                icon: app.and_then(|app| app.icon.clone()),
            };
            Some((match_tier(&entry, &query)?, entry))
        })
        .collect();
    ranked.sort_by_key(|(tier, _)| *tier);
    ranked.into_iter().map(|(_, entry)| entry).collect()
}

fn match_tier(entry: &WindowEntry, query: &str) -> Option<u8> {
    if query.is_empty() {
        return Some(0);
    }
    let fields = [
        entry.title.to_lowercase(),
        entry.class.to_lowercase(),
        entry.app_name.as_deref().unwrap_or_default().to_lowercase(),
    ];
    if fields.iter().any(|field| field.starts_with(query)) {
        return Some(0);
    }
    let haystack = fields.join(" ");
    if query.split_whitespace().all(|word| haystack.contains(word)) {
        return Some(1);
    }
    let compact: String = query.chars().filter(|c| !c.is_whitespace()).collect();
    subsequence_score(&haystack, &compact).map(|_| 2)
}
