use serde_json::Value;

use super::{make_entry, make_profile, open_test_index};
use crate::index::{Search, engine_url, engines, validate_url};

#[test]
fn url_security_and_lossless_normalization() {
    assert!(validate_url("javascript:alert(1)").is_none());
    assert!(validate_url("data:text/html,<script>").is_none());
    assert!(validate_url("file:///etc/passwd").is_none());
    assert!(validate_url("chrome://settings").is_none());
    assert!(validate_url("about:config").is_none());
    assert!(validate_url("ftp://example.com/file").is_none());
    assert!(validate_url("https://user:pass@example.com/").is_none());
    assert!(validate_url("https://user@example.com/").is_none());
    assert!(validate_url("https://:pass@example.com/").is_none());
    assert!(validate_url("https://@example.com/").is_none());
    assert!(validate_url("https://example.com/\nfoo").is_none());
    assert!(validate_url("https://example.com/\0foo").is_none());
    assert!(validate_url("https://example.com/\u{0085}foo").is_none());

    let too_long = format!("https://example.com/{}", "a".repeat(8200));
    assert!(validate_url(&too_long).is_none());

    // Lossless: preserves query params including tracking params and fragments.
    let parsed = validate_url("HTTPS://EXAMPLE.COM:443/docs?utm_source=x&id=2#frag")
        .expect("valid https url");
    assert_eq!(
        parsed.as_str(),
        "https://example.com/docs?utm_source=x&id=2#frag"
    );

    let default_port = validate_url("http://Example.com:80").expect("valid http");
    assert_eq!(default_port.as_str(), "http://example.com/");

    // Direct URL & completion classification
    let (_dir, mut idx) = open_test_index();
    let p = make_profile("fx-1", "firefox", "Firefox", "Default");
    idx.set_enabled(&p.id, true).unwrap();
    idx.replace(
        &p,
        vec![
            make_entry(
                "h1",
                false,
                "https://github.com/hyprwm/Hyprland",
                "Hyprland Repo",
                10,
                Some(1_700_000_000_000),
                &[],
            ),
            make_entry(
                "h2",
                false,
                "https://github.com/hyprwm/Hyprland/wiki?utm_source=keep",
                "Hyprland Wiki",
                1,
                Some(1_699_000_000_000),
                &[],
            ),
        ],
    )
    .unwrap();

    let res = idx
        .search_at(
            Search {
                query: "github.com/hyprw",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            1_700_000_100_000,
        )
        .unwrap();
    assert_eq!(res["input"]["kind"], "url");
    assert_eq!(
        res["input"]["directNavigationTarget"],
        "https://github.com/hyprw"
    );
    assert_eq!(
        res["input"]["completion"]["suggestedUrl"],
        "https://github.com/hyprwm/Hyprland"
    );
    assert!(res["input"]["completion"]["sourceEntryId"].is_string());

    // Unsafe scheme is rejected from direct navigation
    let unsafe_res = idx
        .search_at(
            Search {
                query: "javascript:alert(1)",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            1_700_000_100_000,
        )
        .unwrap();
    assert_eq!(unsafe_res["input"]["kind"], "search");
    assert_eq!(unsafe_res["input"]["directNavigationTarget"], Value::Null);
}

#[test]
fn provider_encoding_and_domain_boundary_filtering() {
    let engine_list = engines();
    let arr = engine_list.as_array().unwrap();
    assert_eq!(arr.len(), 8);
    assert_eq!(arr[0]["engineId"], "github");
    assert_eq!(arr[0]["keyword"], "gh");
    assert_eq!(arr[0]["hostname"], "github.com");

    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-1", "firefox", "Firefox", "Default");
    idx.set_enabled(&fx.id, true).unwrap();
    idx.replace(
        &fx,
        vec![
            make_entry(
                "1",
                false,
                "https://github.com/rust-lang/rust",
                "Rust Repo",
                10,
                Some(10_000),
                &[],
            ),
            make_entry(
                "2",
                false,
                "https://gist.github.com/rust-example",
                "Rust Gist",
                5,
                Some(9_000),
                &[],
            ),
            make_entry(
                "3",
                false,
                "https://github.com.evil.example/rust",
                "Evil Rust Clone",
                99,
                Some(11_000),
                &[],
            ),
        ],
    )
    .unwrap();

    // "gh" without trailing space MUST NOT hijack into engine mode
    let bare_kw = idx
        .search_at(
            Search {
                query: "gh",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            12_000,
        )
        .unwrap();
    assert_eq!(bare_kw["input"]["kind"], "search");

    // "gh " with literal text including special query chars
    let scoped = idx
        .search_at(
            Search {
                query: "gh rust&a=b #1",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            12_000,
        )
        .unwrap();
    assert_eq!(scoped["input"]["kind"], "engine");
    assert_eq!(scoped["input"]["engineId"], "github");
    assert_eq!(scoped["input"]["query"], "rust&a=b #1");
    assert_eq!(scoped["input"]["directNavigationTarget"], Value::Null);

    let suggestions = scoped["groups"][2]["items"].as_array().unwrap();
    assert_eq!(suggestions.len(), 4);
    assert_eq!(suggestions[0]["engineId"], "github");
    assert_eq!(
        suggestions[0]["url"],
        "https://github.com/search?q=rust%26a%3Db+%231"
    );
    assert_eq!(suggestions[1]["engineId"], "aur");
    assert_eq!(suggestions[2]["engineId"], "archpackages");
    assert_eq!(suggestions[3]["engineId"], "youtube");

    // Domain boundary check for engine scope
    let scoped_rust = idx
        .search_at(
            Search {
                query: "gh rust",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            12_000,
        )
        .unwrap();
    let hist = scoped_rust["groups"][1]["items"].as_array().unwrap();
    assert_eq!(hist.len(), 2);
    assert!(
        hist.iter()
            .all(|item| item["hostname"] != "github.com.evil.example")
    );
}

#[test]
fn direct_navigation_preserves_long_url() {
    let (_dir, idx) = open_test_index();
    let input = format!("https://example.com/docs?id={}#section", "a".repeat(800));
    let result = idx
        .search(
            Search {
                query: &input,
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
        )
        .unwrap();
    assert_eq!(result["input"]["directNavigationTarget"], input);
}

#[test]
fn figma_engines_url_alternatives_and_ordinary_defaults() {
    assert_eq!(
        engine_url("archwiki", "hypr land&x=1").unwrap().as_str(),
        "https://wiki.archlinux.org/index.php?search=hypr+land%26x%3D1"
    );
    assert_eq!(
        engine_url("aur", "yay-bin").unwrap().as_str(),
        "https://aur.archlinux.org/packages?K=yay-bin"
    );
    assert_eq!(
        engine_url("archpackages", "hyprland").unwrap().as_str(),
        "https://archlinux.org/packages/?q=hyprland"
    );
    assert!(engine_url("unknown-engine", "test").is_none());

    let (_dir, idx) = open_test_index();

    // Explicit aw engine: primary ArchWiki + AUR, pkg, YouTube
    let aw = idx
        .search_at(
            Search {
                query: "aw hyprland",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            10_000,
        )
        .unwrap();
    assert_eq!(aw["input"]["engineId"], "archwiki");
    assert_eq!(aw["input"]["query"], "hyprland");
    let aw_ids: Vec<&str> = aw["groups"][2]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["engineId"].as_str().unwrap())
        .collect();
    assert_eq!(aw_ids, vec!["archwiki", "aur", "archpackages", "youtube"]);

    // Explicit aur engine skips self and pulls ArchWiki as 4th
    let aur = idx
        .search_at(
            Search {
                query: "aur hyprland-git",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            10_000,
        )
        .unwrap();
    let aur_ids: Vec<&str> = aur["groups"][2]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["engineId"].as_str().unwrap())
        .collect();
    assert_eq!(aur_ids, vec!["aur", "archpackages", "youtube", "archwiki"]);

    // Explicit pkg and yt engines
    let pkg = idx
        .search_at(
            Search {
                query: "pkg mesa",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            10_000,
        )
        .unwrap();
    let pkg_ids: Vec<&str> = pkg["groups"][2]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["engineId"].as_str().unwrap())
        .collect();
    assert_eq!(pkg_ids, vec!["archpackages", "aur", "youtube", "archwiki"]);

    let yt = idx
        .search_at(
            Search {
                query: "yt wayland",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            10_000,
        )
        .unwrap();
    let yt_ids: Vec<&str> = yt["groups"][2]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["engineId"].as_str().unwrap())
        .collect();
    assert_eq!(yt_ids, vec!["youtube", "aur", "archpackages", "archwiki"]);

    // Ordinary query defaults to DuckDuckGo + GitHub
    let ordinary = idx
        .search_at(
            Search {
                query: "wayland compositor",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            10_000,
        )
        .unwrap();
    let ord_ids: Vec<&str> = ordinary["groups"][2]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["engineId"].as_str().unwrap())
        .collect();
    assert_eq!(ord_ids, vec!["duckduckgo", "github"]);
}

#[test]
fn site_scope_parsing_domain_boundary_and_provider_query() {
    let (_dir, mut idx) = open_test_index();
    let fx = make_profile("fx-1", "firefox", "Firefox", "Default");
    idx.set_enabled(&fx.id, true).unwrap();
    idx.replace(
        &fx,
        vec![
            make_entry(
                "1",
                false,
                "https://docs.rs/tokio/latest/tokio/",
                "tokio - Rust",
                10,
                Some(10_000),
                &[],
            ),
            make_entry(
                "2",
                false,
                "https://sub.docs.rs/tokio",
                "Sub Tokio",
                5,
                Some(9_000),
                &[],
            ),
            make_entry(
                "3",
                false,
                "https://docs.rs.evil.org/tokio",
                "Evil Tokio",
                99,
                Some(12_000),
                &[],
            ),
            make_entry(
                "4",
                false,
                "https://crates.io/crates/tokio",
                "crates.io tokio",
                50,
                Some(11_000),
                &[],
            ),
        ],
    )
    .unwrap();

    // site:<valid-domain> with remainder
    let res = idx
        .search_at(
            Search {
                query: "site:Docs.RS tokio",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            15_000,
        )
        .unwrap();
    assert_eq!(res["input"]["kind"], "search");
    assert_eq!(res["input"]["query"], "tokio");
    assert_eq!(res["matchedCount"], 2);
    let hist_urls: Vec<&str> = res["groups"][1]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["url"].as_str().unwrap())
        .collect();
    assert_eq!(
        hist_urls,
        vec![
            "https://docs.rs/tokio/latest/tokio/",
            "https://sub.docs.rs/tokio",
        ]
    );
    let suggestions = res["groups"][2]["items"].as_array().unwrap();
    assert_eq!(suggestions.len(), 2);
    assert_eq!(
        suggestions[0]["url"],
        "https://duckduckgo.com/?q=site%3Adocs.rs+tokio"
    );
    assert_eq!(
        suggestions[1]["url"],
        "https://www.google.com/search?q=site%3Adocs.rs+tokio"
    );

    // site:<valid-domain> with empty remainder (followed by end or space)
    let empty_rem = idx
        .search_at(
            Search {
                query: "site:docs.rs",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            15_000,
        )
        .unwrap();
    assert_eq!(empty_rem["input"]["kind"], "search");
    assert_eq!(empty_rem["input"]["query"], "");
    assert_eq!(empty_rem["matchedCount"], 2);
    let empty_sugg = empty_rem["groups"][2]["items"].as_array().unwrap();
    assert_eq!(
        empty_sugg[0]["url"],
        "https://duckduckgo.com/?q=site%3Adocs.rs"
    );

    // Invalid domain / not followed by space or end must not activate site scope
    let bad_site = idx
        .search_at(
            Search {
                query: "site:docs.rs/tokio",
                engine_id: None,
                profile_ids: None,
                limit: 10,
            },
            false,
            15_000,
        )
        .unwrap();
    assert_eq!(bad_site["input"]["query"], Value::Null);
    assert_eq!(
        bad_site["groups"][2]["items"][0]["url"],
        "https://duckduckgo.com/?q=site%3Adocs.rs%2Ftokio"
    );
}
