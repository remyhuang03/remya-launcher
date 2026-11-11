use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use image::{ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::Path;
use url::Url;
use zeroize::Zeroize;

pub const TEXT_LIMIT: usize = 1024 * 1024;
pub const ORIGINAL_IMAGE_LIMIT: usize = 20_000_000;
pub const OFFER_LIMIT: usize = 64 * 1024 * 1024;
pub const SECRET_LIMIT: usize = 256 * 1024;
pub const PLAIN: &str = "text/plain;charset=utf-8";
pub const PLAIN_TYPES: &[&str] = &[PLAIN, "text/plain", "UTF8_STRING", "TEXT", "STRING"];
pub const HTML: &str = "text/html";
pub const URI_LIST: &str = "text/uri-list";
pub const GNOME_FILES: &str = "x-special/gnome-copied-files";
pub const KDE_CUT: &str = "application/x-kde-cutselection";
pub const IMAGE_TYPES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/gif",
    "image/bmp",
    "image/x-bmp",
];

pub struct Format {
    pub mime: String,
    pub data: Vec<u8>,
}

impl Drop for Format {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Code,
    Command,
    Image,
    Link,
    Color,
    Files,
    Hidden,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub kind: Kind,
    pub title: String,
    pub summary: String,
    pub mime_types: Vec<String>,
    pub thumbnail_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    #[serde(flatten)]
    pub description: Summary,
    pub copied_at: i64,
    pub times_copied: i64,
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

#[derive(Default, Deserialize, Serialize)]
pub struct Detail {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<String>>,
}

pub struct Content {
    pub id: String,
    pub description: Summary,
    pub detail: Detail,
    pub thumbnail: Option<Vec<u8>>,
    pub formats: Vec<Format>,
}

pub fn plain(formats: &[Format]) -> Option<&str> {
    PLAIN_TYPES.iter().find_map(|mime| {
        formats
            .iter()
            .find(|format| format.mime == *mime)
            .and_then(|format| std::str::from_utf8(&format.data).ok())
    })
}

pub fn plain_format(text: String) -> Format {
    Format {
        mime: PLAIN.into(),
        data: text.into_bytes(),
    }
}

pub fn file_paths(formats: &[Format]) -> Option<Vec<String>> {
    let list = [URI_LIST, GNOME_FILES].into_iter().find_map(|mime| {
        formats
            .iter()
            .find(|format| format.mime == mime)
            .and_then(|format| std::str::from_utf8(&format.data).ok())
    })?;
    let paths: Vec<_> = list
        .lines()
        .filter_map(|line| Url::parse(line.trim()).ok())
        .filter_map(|url| url.to_file_path().ok())
        .filter_map(|path| path.into_os_string().into_string().ok())
        .filter(|path| !path.chars().any(char::is_control))
        .collect();
    (!paths.is_empty()).then_some(paths)
}

pub fn content_id(formats: &mut [Format]) -> String {
    formats.sort_unstable_by(|a, b| a.mime.cmp(&b.mime));
    let mut digest = Sha256::new();
    for format in formats {
        digest.update((format.mime.len() as u64).to_le_bytes());
        digest.update(format.mime.as_bytes());
        digest.update((format.data.len() as u64).to_le_bytes());
        digest.update(&format.data);
    }
    URL_SAFE_NO_PAD.encode(digest.finalize())
}

pub fn classify(id: String, mut formats: Vec<Format>) -> Option<Content> {
    let mut detail = Detail::default();
    let mut description = Summary {
        kind: Kind::Text,
        title: String::new(),
        summary: String::new(),
        mime_types: Vec::new(),
        thumbnail_only: false,
        color: None,
    };
    let mut thumbnail = None;

    if let Some((format, preview, width, height)) = IMAGE_TYPES.iter().find_map(|mime| {
        let format = formats.iter().find(|format| format.mime == *mime)?;
        let (preview, width, height) = image_thumbnail(&format.data, image_format(mime)?)?;
        Some((format, preview, width, height))
    }) {
        description.kind = Kind::Image;
        description.title = format!("Image {width} × {height}");
        let label = format.mime.split('/').nth(1)?.to_uppercase();
        let size = format.data.len();
        description.thumbnail_only = size > ORIGINAL_IMAGE_LIMIT;
        description.summary = if description.thumbnail_only {
            format!(
                "{label} · {:.1} MB · thumbnail only",
                size as f64 / 1_000_000.0
            )
        } else {
            format!("{label} · {:.1} MB", size as f64 / 1_000_000.0)
        };
        detail.width = Some(width);
        detail.height = Some(height);
        detail.size = Some(size);
        thumbnail = Some(preview);
    } else if let Some(paths) = file_paths(&formats) {
        description.kind = Kind::Files;
        description.summary = format!("{} {}", paths.len(), plural(paths.len(), "file", "files"));
        description.title = paths
            .first()
            .and_then(|path| Path::new(path).file_name())
            .map(|name| name.to_string_lossy().chars().take(160).collect())
            .unwrap_or_else(|| description.summary.clone());
        // Keep the preview bounded while publishing the original file list.
        detail.files = Some(
            paths
                .into_iter()
                .take(32)
                .filter(|path| path.len() <= 4096)
                .collect(),
        );
    } else {
        let raw = plain(&formats)?;
        if raw.is_empty() {
            return None;
        }
        let text = raw.trim();
        let line_count = raw.lines().count().max(1);
        let rich = formats.iter().any(|format| format.mime == HTML);
        description.title = raw
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Whitespace")
            .trim()
            .chars()
            .take(160)
            .collect();
        description.summary = format!(
            "{line_count} {}{}",
            plural(line_count, "line", "lines"),
            if rich { " · HTML also available" } else { "" }
        );
        if !rich {
            if let Some(host) = link_host(text) {
                description.kind = Kind::Link;
                description.summary = host;
            } else if let Some(color) = parse_color(text) {
                description.kind = Kind::Color;
                description.summary = "Color value".into();
                description.color = Some(color);
            } else if line_count == 1 && looks_like_command(text) {
                description.kind = Kind::Command;
            } else if line_count > 1
                && let Some(language) = detect_code(text)
            {
                description.kind = Kind::Code;
                description.summary = format!("{language} · {}", description.summary);
                detail.language = Some(language.into());
            }
        }
        detail.text = Some(
            raw.chars()
                .take(4096)
                .collect::<String>()
                .lines()
                .take(8)
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }

    if description.thumbnail_only {
        description.mime_types = formats.iter().map(|format| format.mime.clone()).collect();
        formats.clear();
    } else {
        formats.retain(|format| {
            !format.mime.starts_with("image/")
                || description.kind == Kind::Image
                    && format.data.len() <= ORIGINAL_IMAGE_LIMIT
                    && image_format(&format.mime)
                        .is_some_and(|image| image::guess_format(&format.data).ok() == Some(image))
        });
        description.mime_types = formats.iter().map(|format| format.mime.clone()).collect();
    }
    Some(Content {
        id,
        description,
        detail,
        thumbnail,
        formats,
    })
}

fn link_host(text: &str) -> Option<String> {
    let (scheme, _) = text.split_once("://")?;
    if text.len() > 8192
        || text.chars().any(char::is_whitespace)
        || !(scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http"))
    {
        return None;
    }
    Some(Url::parse(text).ok()?.host_str()?.into())
}

fn parse_color(text: &str) -> Option<String> {
    if text.len() > 160 {
        return None;
    }
    let prefix = text.split('(').next()?.to_ascii_lowercase();
    if !text.starts_with('#')
        && !matches!(prefix.as_str(), "rgb" | "rgba" | "hsl" | "hsla" | "oklch")
    {
        return None;
    }
    Some(csscolorparser::parse(text).ok()?.to_css_hex())
}

fn looks_like_command(text: &str) -> bool {
    let first = text.split_ascii_whitespace().next().unwrap_or_default();
    first.starts_with("./")
        || matches!(
            first,
            "sudo"
                | "git"
                | "cargo"
                | "rustc"
                | "flutter"
                | "dart"
                | "fvm"
                | "pnpm"
                | "npm"
                | "npx"
                | "yarn"
                | "bun"
                | "node"
                | "python"
                | "python3"
                | "pip"
                | "pipx"
                | "uv"
                | "make"
                | "cmake"
                | "ninja"
                | "curl"
                | "wget"
                | "ssh"
                | "scp"
                | "rsync"
                | "docker"
                | "podman"
                | "kubectl"
                | "systemctl"
                | "journalctl"
                | "hyprctl"
                | "pacman"
                | "yay"
                | "paru"
                | "bash"
                | "fish"
                | "zsh"
                | "sh"
                | "ls"
                | "cd"
                | "grep"
                | "rg"
                | "cat"
                | "find"
                | "sed"
                | "awk"
                | "chmod"
                | "echo"
                | "cp"
                | "mv"
                | "mkdir"
                | "rm"
                | "env"
        )
}

fn detect_code(text: &str) -> Option<&'static str> {
    let mut language = None;
    for line in text.lines().take(40).map(str::trim_start) {
        if line.starts_with("fn ") || line.starts_with("pub fn ") || line.starts_with("use std::") {
            return Some("Rust");
        }
        if (line.starts_with("def ") || line.starts_with("async def "))
            && line.contains('(')
            && line.ends_with(':')
        {
            return Some("Python");
        }
        if line.starts_with("#include <") || line.starts_with("#include \"") {
            return Some("C / C++");
        }
        if line.starts_with("function ") || line.starts_with("export function ") {
            return Some("JavaScript");
        }
        if line.starts_with("import 'package:") {
            return Some("Dart");
        }
        if line.starts_with("<!DOCTYPE html") || line.starts_with("<html") {
            return Some("HTML");
        }
        if (line.starts_with("const ") || line.starts_with("let ")) && line.contains(" = ") {
            language = Some("Code");
        }
    }
    language
}

fn plural<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 { singular } else { plural }
}

fn image_format(mime: &str) -> Option<ImageFormat> {
    Some(match mime {
        "image/png" => ImageFormat::Png,
        "image/jpeg" => ImageFormat::Jpeg,
        "image/webp" => ImageFormat::WebP,
        "image/gif" => ImageFormat::Gif,
        "image/bmp" | "image/x-bmp" => ImageFormat::Bmp,
        _ => return None,
    })
}

fn image_thumbnail(data: &[u8], format: ImageFormat) -> Option<(Vec<u8>, u32, u32)> {
    if image::guess_format(data).ok()? != format {
        return None;
    }
    let make_reader = || {
        let mut reader = ImageReader::with_format(Cursor::new(data), format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        reader
    };
    let (width, height) = make_reader().into_dimensions().ok()?;
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 24_000_000
    {
        return None;
    }
    let thumbnail = make_reader().decode().ok()?.thumbnail(320, 320);
    let mut output = Cursor::new(Vec::new());
    thumbnail.write_to(&mut output, ImageFormat::Png).ok()?;
    Some((output.into_inner(), width, height))
}

#[cfg(test)]
pub fn fixture(mut formats: Vec<Format>) -> Content {
    classify(content_id(&mut formats), formats).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(value: &str) -> Content {
        fixture(vec![plain_format(value.into())])
    }

    #[test]
    fn classifies_and_preserves_text() {
        assert_eq!(
            text("https://example.com/path").description.kind,
            Kind::Link
        );
        assert_eq!(text("pnpm add --save lib").description.kind, Kind::Command);
        assert_eq!(
            text("def hello():\n    print('hello')").description.kind,
            Kind::Code
        );
        assert_eq!(
            text("  ordinary words  \n").formats[0].data,
            b"  ordinary words  \n"
        );
        assert_eq!(text("oklch(0.49 0.13 297)").description.kind, Kind::Color);
        assert_eq!(
            text("#f0f8").description.color.as_deref(),
            Some("#ff00ff88")
        );
        let rich = fixture(vec![
            plain_format("https://example.com".into()),
            Format {
                mime: HTML.into(),
                data: b"<b>example</b>".to_vec(),
            },
        ]);
        assert_eq!(rich.description.kind, Kind::Text);
        assert_eq!(rich.formats.len(), 2);
    }

    #[test]
    fn parses_local_file_uris_and_preserves_file_manager_mime() {
        let content = fixture(vec![Format {
            mime: GNOME_FILES.into(),
            data: b"cut\nfile:///tmp/a%20b\nhttps://example.com/unsafe\n".to_vec(),
        }]);
        assert_eq!(content.description.kind, Kind::Files);
        assert_eq!(
            content.detail.files.as_deref(),
            Some(&["/tmp/a b".into()][..])
        );
        assert_eq!(content.formats[0].mime, GNOME_FILES);
    }

    #[test]
    fn oversized_image_keeps_only_a_thumbnail() {
        let mut bmp = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(3000, 3000)
            .write_to(&mut bmp, ImageFormat::Bmp)
            .unwrap();
        let content = fixture(vec![Format {
            mime: "image/bmp".into(),
            data: bmp.into_inner(),
        }]);
        assert!(content.description.thumbnail_only);
        assert!(content.formats.is_empty());
        assert_eq!(content.detail.width, Some(3000));
        assert!(content.thumbnail.unwrap().len() < 500_000);
    }
}
