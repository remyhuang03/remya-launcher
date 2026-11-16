use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use tempfile::TempDir;

use super::discovery::{discover_with_env, is_gecko_profile_dir, profile_id};
use super::tests_source::{create_chromium_history_db, create_gecko_db};
use super::{Family, Profile, read};

#[test]
fn discovery_covers_xdg_legacy_flatpak_snap_dedup_and_exclusions() {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path();
    let xdg = home.join(".config");

    // Firefox legacy + modern XDG symlinked to same canonical target + Snap current symlink
    let ff_legacy = home.join(".mozilla/firefox");
    let ff_prof = ff_legacy.join("Profiles/abcd.default-release");
    fs::create_dir_all(&ff_prof).unwrap();
    create_gecko_db(&ff_prof.join("places.sqlite"), false);

    // Custom absolute profile directory configured in profiles.ini
    let ff_custom = home.join("custom-ff-work");
    fs::create_dir_all(&ff_custom).unwrap();
    create_gecko_db(&ff_custom.join("places.sqlite"), false);

    // Install-targeted profile directory in installs.ini
    let ff_install = ff_legacy.join("Profiles/install-only.prof");
    fs::create_dir_all(&ff_install).unwrap();
    create_gecko_db(&ff_install.join("places.sqlite"), false);

    // Excluded guest profile in profiles.ini
    let ff_guest = ff_legacy.join("Profiles/guest-dir");
    fs::create_dir_all(&ff_guest).unwrap();
    create_gecko_db(&ff_guest.join("places.sqlite"), false);

    fs::write(
            ff_legacy.join("profiles.ini"),
            format!(
                "[Profile0]\nName=default-release\nIsRelative=1\nPath=Profiles/abcd.default-release\nDefault=1\n\
                 [Profile1]\nName=work-custom\nIsRelative=0\nPath={}\n\
                 [Profile2]\nName=Guest\nIsRelative=1\nPath=Profiles/guest-dir\n\
                 [Install4F96]\nDefault=Profiles/abcd.default-release\n",
                ff_custom.display()
            ),
        )
        .unwrap();
    fs::write(
        ff_legacy.join("installs.ini"),
        "[4F96]\nDefault=Profiles/install-only.prof\n",
    )
    .unwrap();

    // Modern XDG symlink pointing to legacy firefox root -> must dedup cleanly
    fs::create_dir_all(xdg.join("mozilla")).unwrap();
    symlink(&ff_legacy, xdg.join("mozilla/firefox")).unwrap();

    // Snap Firefox rev + current symlink -> must dedup to one profile
    let snap_rev = home.join("snap/firefox/1234/.mozilla/firefox");
    let snap_prof = snap_rev.join("Profiles/snap.default");
    fs::create_dir_all(&snap_prof).unwrap();
    create_gecko_db(&snap_prof.join("places.sqlite"), false);
    fs::write(
        snap_rev.join("profiles.ini"),
        "[Profile0]\nName=snap-default\nIsRelative=1\nPath=Profiles/snap.default\n",
    )
    .unwrap();
    symlink(
        home.join("snap/firefox/1234"),
        home.join("snap/firefox/current"),
    )
    .unwrap();

    // Chromium with Local State + numbered fallback + Guest/System exclusion
    let chr_root = xdg.join("chromium");
    fs::create_dir_all(chr_root.join("Default")).unwrap();
    fs::create_dir_all(chr_root.join("Profile 1")).unwrap();
    fs::create_dir_all(chr_root.join("Profile 2")).unwrap();
    fs::create_dir_all(chr_root.join("Guest Profile")).unwrap();
    fs::create_dir_all(chr_root.join("System Profile")).unwrap();
    create_chromium_history_db(&chr_root.join("Default/History"), false);
    create_chromium_history_db(&chr_root.join("Profile 1/History"), false);
    create_chromium_history_db(&chr_root.join("Profile 2/History"), false);
    create_chromium_history_db(&chr_root.join("Guest Profile/History"), false);
    create_chromium_history_db(&chr_root.join("System Profile/History"), false);
    fs::write(
        chr_root.join("Local State"),
        r#"{
                "profile": {
                    "info_cache": {
                        "Default": { "name": "Personal" },
                        "Profile 1": { "name": "Ephemeral", "is_ephemeral": true },
                        "Guest Profile": { "name": "Guest", "is_guest": true }
                    }
                }
            }"#,
    )
    .unwrap();

    // Flatpak Opera flat-root layout
    let opera_flat = home.join(".var/app/com.opera.Opera/config/opera");
    fs::create_dir_all(&opera_flat).unwrap();
    create_chromium_history_db(&opera_flat.join("History"), false);

    let discovered = discover_with_env(Some(home), Some(&xdg), None, None);

    // Expect: 4 firefox (default-release, work-custom, install-only.prof, snap-default)
    // + 2 chromium (Default -> Personal, Profile 2) + 1 opera (Default)
    let ff_labels: Vec<&str> = discovered
        .iter()
        .filter(|p| p.browser_id == "firefox")
        .map(|p| p.label.as_str())
        .collect();
    assert_eq!(ff_labels.len(), 4, "firefox profiles: {ff_labels:?}");
    assert!(ff_labels.contains(&"default-release"));
    assert!(ff_labels.contains(&"work-custom"));
    assert!(ff_labels.contains(&"install-only.prof"));
    assert!(ff_labels.contains(&"snap-default"));

    let chr_labels: Vec<&str> = discovered
        .iter()
        .filter(|p| p.browser_id == "chromium")
        .map(|p| p.label.as_str())
        .collect();
    assert_eq!(chr_labels, vec!["Personal", "Profile 2"]);

    let opera_labels: Vec<&str> = discovered
        .iter()
        .filter(|p| p.browser_id == "opera")
        .map(|p| p.label.as_str())
        .collect();
    assert_eq!(opera_labels, vec!["Default"]);

    // IDs are stable 64-char sha256 hex and never leak filesystem paths
    for p in &discovered {
        assert_eq!(p.id.len(), 64);
        assert!(p.id.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(!p.id.contains('/'));
    }
    assert_eq!(
        profile_id("firefox", &fs::canonicalize(&ff_prof).unwrap()),
        discovered
            .iter()
            .find(|p| p.label == "default-release")
            .unwrap()
            .id
    );
}

#[test]
fn discovery_reads_explicit_chromium_user_data_and_config_roots() {
    let tmp = TempDir::new().unwrap();
    let chrome_config = tmp.path().join("chrome-config");
    let configured = chrome_config.join("chromium/Default");
    let custom_root = tmp.path().join("custom-data");
    let custom = custom_root.join("Default");
    for profile in [&configured, &custom] {
        fs::create_dir_all(profile).unwrap();
        create_chromium_history_db(&profile.join("History"), false);
    }

    let discovered = discover_with_env(None, None, Some(&chrome_config), Some(&custom_root));
    assert!(discovered.iter().any(|profile| profile.path == configured));
    assert!(discovered.iter().any(|profile| profile.path == custom));
}

#[test]
fn rejects_symlinks_hardlinks_and_world_writable_profile_dirs() {
    let tmp = TempDir::new().unwrap();
    let target_db = tmp.path().join("real-places.sqlite");
    let conn = create_gecko_db(&target_db, false);
    drop(conn);

    // Symlinked places.sqlite -> PERMISSION_DENIED and not discovered
    let sym_dir = tmp.path().join("sym-prof");
    fs::create_dir_all(&sym_dir).unwrap();
    symlink(&target_db, sym_dir.join("places.sqlite")).unwrap();
    assert!(!is_gecko_profile_dir(&sym_dir));
    let sym_profile = Profile {
        id: profile_id("firefox", &sym_dir),
        browser_id: "firefox".into(),
        browser_label: "Firefox".into(),
        label: "sym".into(),
        path: sym_dir,
        family: Family::Gecko,
    };
    assert_eq!(read(&sym_profile), Err("PERMISSION_DENIED"));

    // Hardlinked places.sqlite (nlink == 2) -> PERMISSION_DENIED and not discovered
    let hl_dir = tmp.path().join("hl-prof");
    fs::create_dir_all(&hl_dir).unwrap();
    fs::hard_link(&target_db, hl_dir.join("places.sqlite")).unwrap();
    assert!(!is_gecko_profile_dir(&hl_dir));
    let hl_profile = Profile {
        id: profile_id("firefox", &hl_dir),
        browser_id: "firefox".into(),
        browser_label: "Firefox".into(),
        label: "hl".into(),
        path: hl_dir,
        family: Family::Gecko,
    };
    assert_eq!(read(&hl_profile), Err("PERMISSION_DENIED"));

    // World-writable profile directory -> PERMISSION_DENIED
    let ww_dir = tmp.path().join("ww-prof");
    fs::create_dir_all(&ww_dir).unwrap();
    let ww_conn = create_gecko_db(&ww_dir.join("places.sqlite"), false);
    drop(ww_conn);
    fs::set_permissions(&ww_dir, fs::Permissions::from_mode(0o777)).unwrap();
    let ww_profile = Profile {
        id: profile_id("firefox", &ww_dir),
        browser_id: "firefox".into(),
        browser_label: "Firefox".into(),
        label: "ww".into(),
        path: ww_dir.clone(),
        family: Family::Gecko,
    };
    assert_eq!(read(&ww_profile), Err("PERMISSION_DENIED"));
    fs::set_permissions(&ww_dir, fs::Permissions::from_mode(0o700)).unwrap();
}
