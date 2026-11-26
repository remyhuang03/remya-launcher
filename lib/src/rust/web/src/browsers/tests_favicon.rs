use std::fs;
use std::io::Cursor;

use image::{DynamicImage, ImageFormat, ImageReader, Rgba, RgbaImage};
use rusqlite::{Connection, params};
use tempfile::TempDir;

use super::discovery::profile_id;
use super::favicon::normalize_raster_favicon;
use super::sqlite::{classify_sqlite_error, with_chromium_favicons, with_readonly_sqlite};
use super::{Family, MAX_FAVICON_BYTES, Profile, favicon};

pub(super) fn make_png(width: u32, height: u32) -> Vec<u8> {
    let mut img = RgbaImage::new(width, height);
    for pixel in img.pixels_mut() {
        *pixel = Rgba([20, 120, 220, 255]);
    }
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img)
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

#[test]
fn favicon_bounds_blob_rejects_svg_and_normalizes_to_64px_png() {
    let tmp = TempDir::new().unwrap();
    let ff_dir = tmp.path().join("ff-fav");
    fs::create_dir_all(&ff_dir).unwrap();
    let ff_conn = Connection::open(ff_dir.join("favicons.sqlite")).unwrap();
    ff_conn
            .execute_batch(
                "CREATE TABLE moz_pages_w_icons (id INTEGER PRIMARY KEY, page_url TEXT);
                 CREATE TABLE moz_icons_to_pages (page_id INTEGER, icon_id INTEGER);
                 CREATE TABLE moz_icons (id INTEGER PRIMARY KEY, icon_url TEXT, width INTEGER, data BLOB);",
            )
            .unwrap();

    let big_png = make_png(128, 128);
    let svg_bytes = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"></svg>"#;
    let oversized_blob = vec![0u8; MAX_FAVICON_BYTES + 128];

    ff_conn
        .execute(
            "INSERT INTO moz_pages_w_icons (id, page_url) VALUES
                 (1, 'https://example.com/'),
                 (2, 'https://svg-only.example/')",
            [],
        )
        .unwrap();
    ff_conn
        .execute(
            "INSERT INTO moz_icons (id, icon_url, width, data) VALUES
                 (1, 'https://example.com/icon.svg', 64, ?1),
                 (2, 'https://example.com/huge.png', 64, ?2),
                 (3, 'https://example.com/icon-128.png', 128, ?3),
                 (4, 'https://svg-only.example/logo.svg', 64, ?1)",
            params![svg_bytes.as_slice(), oversized_blob, big_png],
        )
        .unwrap();
    ff_conn
        .execute(
            "INSERT INTO moz_icons_to_pages (page_id, icon_id) VALUES
                 (1, 1), (1, 2), (1, 3), (2, 4)",
            [],
        )
        .unwrap();
    drop(ff_conn);

    let ff_profile = Profile {
        id: profile_id("firefox", &ff_dir),
        browser_id: "firefox".into(),
        browser_label: "Firefox".into(),
        label: "default".into(),
        path: ff_dir,
        family: Family::Gecko,
    };

    let png = favicon(&ff_profile, "https://example.com").expect("raster icon must decode");
    let (w, h) = ImageReader::with_format(Cursor::new(&png), ImageFormat::Png)
        .into_dimensions()
        .unwrap();
    assert_eq!((w, h), (64, 64));
    assert!(favicon(&ff_profile, "https://svg-only.example/").is_none());
    assert!(favicon(&ff_profile, "javascript:alert(1)").is_none());

    // Chromium Favicons DB test
    let chr_dir = tmp.path().join("chr-fav");
    fs::create_dir_all(&chr_dir).unwrap();
    let chr_conn = Connection::open(chr_dir.join("Favicons")).unwrap();
    chr_conn
            .execute_batch(
                "CREATE TABLE icon_mapping (id INTEGER PRIMARY KEY, page_url TEXT, icon_id INTEGER);
                 CREATE TABLE favicons (id INTEGER PRIMARY KEY, url TEXT);
                 CREATE TABLE favicon_bitmaps (id INTEGER PRIMARY KEY, icon_id INTEGER, image_data BLOB, width INTEGER, height INTEGER, last_updated INTEGER);",
            )
            .unwrap();
    let png_32 = make_png(32, 32);
    chr_conn
            .execute(
                "INSERT INTO icon_mapping (id, page_url, icon_id) VALUES (1, 'https://app.example/docs', 10)",
                [],
            )
            .unwrap();
    chr_conn
        .execute(
            "INSERT INTO favicons (id, url) VALUES (10, 'https://app.example/favicon.svg')",
            [],
        )
        .unwrap();
    chr_conn
            .execute(
                "INSERT INTO favicon_bitmaps (id, icon_id, image_data, width, height, last_updated) VALUES
                 (1, 10, ?1, 32, 32, 100),
                 (2, 10, ?2, 128, 128, 200)",
                params![png_32, big_png],
            )
            .unwrap();
    drop(chr_conn);

    let chr_profile = Profile {
        id: profile_id("chromium", &chr_dir),
        browser_id: "chromium".into(),
        browser_label: "Chromium".into(),
        label: "Default".into(),
        path: chr_dir,
        family: Family::Chromium,
    };
    let chr_png = favicon(&chr_profile, "https://app.example/docs").unwrap();
    let (cw, ch) = ImageReader::with_format(Cursor::new(&chr_png), ImageFormat::Png)
        .into_dimensions()
        .unwrap();
    assert!(cw <= 64 && ch <= 64);

    // Oversized PNG dimensions (4096x4096) in header must fail before decoding pixels.
    let mut oversized_dim_png = make_png(16, 16);
    // Patch IHDR width/height bytes (offsets 16..24) to 4096x4096; CRC mismatch doesn't matter
    // since dimensions are bounded right after reading IHDR.
    oversized_dim_png[16..20].copy_from_slice(&4096u32.to_be_bytes());
    oversized_dim_png[20..24].copy_from_slice(&4096u32.to_be_bytes());
    assert!(normalize_raster_favicon(&oversized_dim_png).is_none());
}

#[test]
fn chromium_favicons_read_committed_pages_under_exclusive_lock() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("Favicons");
    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch(
            "PRAGMA locking_mode = EXCLUSIVE;
             PRAGMA synchronous = FULL;
             PRAGMA cache_size = 3;
             CREATE TABLE icons (id INTEGER PRIMARY KEY, data BLOB);
             INSERT INTO icons VALUES (1, 'committed');
             BEGIN IMMEDIATE;
             UPDATE icons SET data = 'staged' WHERE id = 1;",
        )
        .unwrap();

    let read_icon = |tx: &rusqlite::Transaction<'_>| -> Result<String, &'static str> {
        tx.query_row("SELECT data FROM icons WHERE id = 1", [], |row| row.get(0))
            .map_err(|err| classify_sqlite_error(&err))
    };
    assert_eq!(with_readonly_sqlite(&path, read_icon), Err("SOURCE_LOCKED"));
    assert_eq!(
        with_chromium_favicons(&path, read_icon).unwrap(),
        "committed"
    );

    writer
        .execute("INSERT INTO icons VALUES (2, zeroblob(262144))", [])
        .unwrap();
    assert_eq!(
        with_chromium_favicons(&path, read_icon),
        Err("SOURCE_LOCKED")
    );
    writer.execute_batch("ROLLBACK").unwrap();
}
