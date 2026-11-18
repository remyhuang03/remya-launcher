use crate::model::{Entry, expand_path};
use gio::glib::{DateTime, TimeZone};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Query {
    pub text: String,
    pub words: Vec<String>,
    pub contents: bool,
    pub scope: Option<PathBuf>,
    pub hidden: Option<bool>,
    pub changed: bool,
    pub filtered: bool,
    pub diagnostic: Option<String>,
    ancestor_term: bool,
    ext: Vec<String>,
    kind: Option<&'static str>,
    size: Option<(char, u64)>,
    modified: Option<(i64, Option<i64>)>,
}

fn size_filter(value: &str) -> Option<(char, u64)> {
    let op = value.chars().next()?;
    if !matches!(op, '>' | '<') {
        return None;
    }
    let rest = &value[1..];
    let i = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let factor = match rest[i..].to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1024,
        "m" | "mb" | "mib" => 1024 * 1024,
        "g" | "gb" | "gib" => 1024 * 1024 * 1024,
        _ => return None,
    };
    Some((op, rest[..i].parse::<u64>().ok()?.checked_mul(factor)?))
}

fn modified_filter(value: &str) -> Option<(i64, Option<i64>)> {
    let now = DateTime::now_local().ok()?;
    let midnight =
        |year, month, day| DateTime::new(&TimeZone::local(), year, month, day, 0, 0, 0.0).ok();
    if value == "today" {
        let start = midnight(now.year(), now.month(), now.day_of_month())?;
        return Some((start.to_unix(), Some(start.add_days(1).ok()?.to_unix())));
    }
    if let Some(days) = value.strip_suffix('d') {
        let seconds = days.parse::<u32>().ok()? as i64 * 86400;
        return Some((now.to_unix() - seconds, None));
    }
    let parts: Vec<_> = value.strip_prefix('>')?.split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return None;
    };
    Some((
        midnight(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)?.to_unix(),
        None,
    ))
}

fn words(raw: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if quoted && chars.peek().is_some_and(|c| matches!(c, '\\' | '"')) => {
                word.push(chars.next().unwrap())
            }
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            _ => word.push(ch),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

impl Query {
    pub fn parse(
        raw: &str,
        mode: &str,
        anchor: &Path,
        home: &Path,
        filters: Option<&[serde_json::Value]>,
    ) -> Result<Self, String> {
        let (contents, raw) = if let Some(rest) = raw.strip_prefix("rg ") {
            (true, rest)
        } else {
            (mode == "contents", raw)
        };
        let mut result = Self {
            contents,
            ..Default::default()
        };
        let tokens = if let Some(filters) = filters {
            result.words = words(raw);
            result.text = raw.to_owned();
            filters
                .iter()
                .filter_map(serde_json::Value::as_str)
                .flat_map(words)
                .collect()
        } else {
            words(raw)
        };
        let mut invalid = Vec::new();
        for word in tokens {
            let (key, value) = word.split_once(':').unwrap_or(("", ""));
            match key {
                "in" if !value.is_empty() => result.scope = Some(expand_path(value, anchor, home)?),
                "hidden" if matches!(value, "yes" | "no") => result.hidden = Some(value == "yes"),
                "type" if matches!(value, "file" | "dir" | "directory" | "link" | "symlink") => {
                    result.kind = Some(match value {
                        "file" => "file",
                        "dir" | "directory" => "directory",
                        _ => "symlink",
                    });
                }
                "size" if size_filter(value).is_some() => {
                    result.size = size_filter(value);
                }
                "mod" if modified_filter(value).is_some() => {
                    result.modified = modified_filter(value);
                }
                "ext" if !value.is_empty() => {
                    result.ext = value
                        .split(',')
                        .map(|s| s.trim_start_matches('.').to_lowercase())
                        .collect()
                }
                "git" if value == "changed" => result.changed = true,
                _ => {
                    if matches!(
                        key,
                        "type" | "size" | "in" | "mod" | "hidden" | "ext" | "git"
                    ) && !value.is_empty()
                    {
                        invalid.push(key.to_owned());
                    }
                    result.words.push(word);
                    continue;
                }
            }
            result.filtered = true;
        }
        if !invalid.is_empty() {
            result.diagnostic = Some(format!("Unknown value for {} filter.", invalid.join(", ")));
        }
        if filters.is_none() {
            result.text = if !result.filtered && !raw.contains('"') {
                raw.to_owned()
            } else {
                result.words.join(" ")
            };
        }
        result.words = result.words.into_iter().map(|w| w.to_lowercase()).collect();
        result.ancestor_term = result.words.iter().any(|w| w.contains('/'));
        Ok(result)
    }

    pub fn accepts(&self, entry: &Entry) -> bool {
        if self.kind.is_some_and(|k| entry.kind != k) {
            return false;
        }
        if !self.ext.is_empty() {
            let name = entry.name.to_lowercase();
            if entry.kind == "directory"
                || !self.ext.iter().any(|ext| {
                    name.strip_suffix(ext)
                        .and_then(|stem| stem.strip_suffix('.'))
                        .is_some_and(|stem| !stem.is_empty())
                })
            {
                return false;
            }
        }
        if let Some((op, size)) = self.size
            && !entry
                .size
                .is_some_and(|n| if op == '>' { n > size } else { n < size })
        {
            return false;
        }
        if let Some((start, end)) = self.modified
            && (entry.modified < start || end.is_some_and(|e| entry.modified >= e))
        {
            return false;
        }
        true
    }

    pub fn rank(&self, name: &str, parent: &str) -> Option<i32> {
        let lower = name.to_lowercase();
        let parent = parent.to_lowercase();
        let mut min = 6;
        let mut total = 0;
        for word in &self.words {
            let word = word.as_str();
            let score = if word.contains('/') {
                if !parent.contains(word) {
                    return None;
                }
                6
            } else if lower == word {
                6
            } else if lower.starts_with(word) {
                5
            } else if lower
                .split(|c: char| !c.is_alphanumeric())
                .any(|w| w.starts_with(word))
                || camel_initials(name, word)
            {
                4
            } else if lower.contains(word) {
                3
            } else if !self.ancestor_term && parent.contains(word) {
                2
            } else {
                let mut letters = lower.chars();
                if !word.chars().all(|needed| letters.any(|ch| ch == needed)) {
                    return None;
                }
                1
            };
            min = min.min(score);
            total += score;
        }
        Some(min * 1024 + total)
    }
}

fn camel_initials(name: &str, word: &str) -> bool {
    let mut boundary = true;
    let mut initials = name
        .chars()
        .filter(|ch| {
            let initial = ch.is_alphanumeric() && (boundary || ch.is_uppercase());
            boundary = !ch.is_alphanumeric();
            initial
        })
        .flat_map(char::to_lowercase);
    word.chars().all(|c| initials.next() == Some(c))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ancestor_terms_and_quoted_scope() {
        let home = Path::new("/home/example");
        let q = Query::parse(
            "bar/docs spec in:\"~/My Projects\" ext:tar.gz type:file",
            "",
            home,
            home,
            None,
        )
        .unwrap();
        assert_eq!(
            q.scope.as_deref(),
            Some(Path::new("/home/example/My Projects"))
        );
        assert!(q.rank("specification.tar.gz", "bar/docs").is_some());
        assert!(q.rank("specification.tar.gz", "baz/docs").is_none());
        assert_eq!(size_filter(">4K"), Some(('>', 4096)));
    }
}
