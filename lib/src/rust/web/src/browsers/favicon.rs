use std::io::Cursor;
use std::path::Path;

use image::imageops::FilterType;
use image::{ImageFormat, ImageReader, Limits};
use rusqlite::params;
use rusqlite::types::ValueRef;

use super::sqlite::{classify_sqlite_error, require_columns, with_readonly_sqlite};
use super::{
    MAX_FAVICON_BYTES, MAX_FAVICON_DECODE_ALLOC, MAX_FAVICON_PIXELS, MAX_FAVICON_SOURCE_DIM,
    TARGET_FAVICON_DIM,
};

pub(super) fn build_favicon_url_candidates(raw_url: &str, normalized_url: &str) -> Vec<String> {
    if raw_url == normalized_url {
        vec![normalized_url.to_string()]
    } else {
        vec![normalized_url.to_string(), raw_url.to_string()]
    }
}

pub(super) fn read_gecko_favicon(dir: &Path, candidate_urls: &[String]) -> Option<Vec<u8>> {
    with_readonly_sqlite(&dir.join("favicons.sqlite"), |tx| {
        require_columns(tx, "moz_pages_w_icons", &["id", "page_url"])?;
        require_columns(tx, "moz_icons_to_pages", &["page_id", "icon_id"])?;
        require_columns(tx, "moz_icons", &["id", "icon_url", "width", "data"])?;

        let mut stmt = tx
            .prepare(
                "SELECT i.data, COALESCE(i.icon_url, '') \
                 FROM moz_pages_w_icons p \
                 JOIN moz_icons_to_pages ip ON ip.page_id = p.id \
                 JOIN moz_icons i ON i.id = ip.icon_id \
                 WHERE p.page_url = ?1 \
                   AND TYPEOF(i.data) = 'blob' \
                   AND LENGTH(i.data) BETWEEN 1 AND ?2 \
                 ORDER BY \
                   CASE WHEN i.width = 64 THEN 0 WHEN i.width > 64 THEN 1 WHEN i.width > 0 THEN 2 ELSE 3 END ASC, \
                   ABS(COALESCE(i.width, 0) - 64) ASC \
                 LIMIT 8",
            )
            .map_err(|err| classify_sqlite_error(&err))?;

        Ok(query_first_favicon(&mut stmt, candidate_urls))
    })
    .ok()
    .flatten()
}

pub(super) fn read_chromium_favicon(dir: &Path, candidate_urls: &[String]) -> Option<Vec<u8>> {
    with_readonly_sqlite(&dir.join("Favicons"), |tx| {
        require_columns(tx, "icon_mapping", &["page_url", "icon_id"])?;
        require_columns(tx, "favicons", &["id", "url"])?;
        let bitmap_cols = require_columns(
            tx,
            "favicon_bitmaps",
            &["icon_id", "image_data", "width", "height"],
        )?;
        let order_tail = if bitmap_cols.contains("last_updated") {
            ", b.last_updated DESC"
        } else {
            ""
        };

        let sql = format!(
            "SELECT b.image_data, COALESCE(f.url, '') \
             FROM icon_mapping m \
             JOIN favicons f ON f.id = m.icon_id \
             JOIN favicon_bitmaps b ON b.icon_id = f.id \
             WHERE m.page_url = ?1 \
               AND TYPEOF(b.image_data) = 'blob' \
               AND LENGTH(b.image_data) BETWEEN 1 AND ?2 \
             ORDER BY ABS(COALESCE(b.width, 0) - 64) + ABS(COALESCE(b.height, 0) - 64) ASC {order_tail} \
             LIMIT 8"
        );
        let mut stmt = tx
            .prepare(&sql)
            .map_err(|err| classify_sqlite_error(&err))?;

        Ok(query_first_favicon(&mut stmt, candidate_urls))
    })
    .ok()
    .flatten()
}

fn query_first_favicon(
    stmt: &mut rusqlite::Statement<'_>,
    candidate_urls: &[String],
) -> Option<Vec<u8>> {
    let max_bytes = MAX_FAVICON_BYTES as i64;
    for page_url in candidate_urls {
        let Ok(mut rows) = stmt.query(params![page_url, max_bytes]) else {
            continue;
        };
        while let Ok(Some(row)) = rows.next() {
            if let Ok(ValueRef::Text(url_bytes)) = row.get_ref(1)
                && is_svg_icon_hint(&String::from_utf8_lossy(url_bytes))
            {
                continue;
            }
            let Ok(ValueRef::Blob(blob)) = row.get_ref(0) else {
                continue;
            };
            if blob.is_empty() || blob.len() > MAX_FAVICON_BYTES {
                continue;
            }
            if let Some(png) = normalize_raster_favicon(blob) {
                return Some(png);
            }
        }
    }
    None
}

fn is_svg_icon_hint(icon_url: &str) -> bool {
    let lower = icon_url.trim().to_ascii_lowercase();
    lower.starts_with("data:image/svg+xml") || lower.ends_with(".svg") || lower.contains(".svg?")
}

fn looks_like_svg_payload(data: &[u8]) -> bool {
    let prefix_len = data.len().min(256);
    let Ok(prefix) = std::str::from_utf8(&data[..prefix_len]) else {
        return false;
    };
    let trimmed = prefix.trim_start_matches('\u{feff}').trim_start();
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("<svg") || lower.starts_with("<?xml") || lower.starts_with("<!doctype svg")
}

pub(super) fn normalize_raster_favicon(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() || data.len() > MAX_FAVICON_BYTES || looks_like_svg_payload(data) {
        return None;
    }

    let format = image::guess_format(data).ok()?;
    if !matches!(
        format,
        ImageFormat::Png
            | ImageFormat::Ico
            | ImageFormat::Jpeg
            | ImageFormat::WebP
            | ImageFormat::Gif
    ) {
        return None;
    }

    let make_reader = || {
        let mut reader = ImageReader::with_format(Cursor::new(data), format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_FAVICON_SOURCE_DIM);
        limits.max_image_height = Some(MAX_FAVICON_SOURCE_DIM);
        limits.max_alloc = Some(MAX_FAVICON_DECODE_ALLOC as u64);
        reader.limits(limits);
        reader
    };

    let (width, height) = make_reader().into_dimensions().ok()?;
    if width == 0
        || height == 0
        || width > MAX_FAVICON_SOURCE_DIM
        || height > MAX_FAVICON_SOURCE_DIM
        || u64::from(width) * u64::from(height) > MAX_FAVICON_PIXELS
    {
        return None;
    }

    let decoded = make_reader().decode().ok()?;
    let normalized =
        if decoded.width() > TARGET_FAVICON_DIM || decoded.height() > TARGET_FAVICON_DIM {
            decoded.resize(TARGET_FAVICON_DIM, TARGET_FAVICON_DIM, FilterType::Lanczos3)
        } else {
            decoded
        };

    let mut out = Cursor::new(Vec::new());
    normalized.write_to(&mut out, ImageFormat::Png).ok()?;
    let png_bytes = out.into_inner();
    if png_bytes.is_empty() || png_bytes.len() > MAX_FAVICON_BYTES {
        return None;
    }
    Some(png_bytes)
}
