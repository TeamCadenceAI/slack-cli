//! Browser / desktop-app profile discovery for local Slack credential extraction.
//!
//! Slack stores its per-team `xoxc` tokens in a Chromium-style LevelDB
//! (`Local Storage/leveldb`) and its shared `xoxd` cookie in a `Cookies`
//! SQLite database. Both Chromium-based browsers and the Slack desktop app use
//! this layout. This module enumerates the on-disk locations of those artifacts
//! so sibling modules can read and decrypt them.
//!
//! Discovery is currently implemented for macOS. On other operating systems
//! [`discover_profiles`] compiles but returns an empty list until platform
//! support lands.

use std::path::PathBuf;

/// A single discovered browser (or desktop-app) profile that may contain
/// extractable Slack credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProfile {
    /// Human-readable browser name (e.g. `"Chrome"`, `"Brave"`, `"Slack"`).
    pub browser: String,
    /// Profile name within the browser (e.g. `"Default"`, `"Profile 1"`).
    /// The Slack desktop app uses `"Default"` as a synthetic profile name.
    pub profile: String,
    /// Absolute path to this profile's `Local Storage/leveldb` directory.
    pub local_storage_leveldb: PathBuf,
    /// Absolute path to this profile's `Cookies` SQLite database.
    pub cookies_db: PathBuf,
    /// macOS Keychain generic-password service name that holds the AES key
    /// used to decrypt cookie values (e.g. `"Chrome Safe Storage"`).
    pub safe_storage_service: String,
}

/// Static description of a supported Chromium-based application.
struct BrowserSpec {
    /// Display name emitted in [`BrowserProfile::browser`].
    name: &'static str,
    /// Path (relative to `~/Library/Application Support`) of the app's data root.
    data_root: &'static str,
    /// Keychain "Safe Storage" service name for cookie decryption.
    safe_storage_service: &'static str,
    /// Whether this app uses per-profile subdirectories (`Default`, `Profile N`).
    /// The Slack desktop app stores the artifacts at its data root instead.
    has_profiles: bool,
}

/// The set of macOS applications we know how to enumerate.
const BROWSER_SPECS: &[BrowserSpec] = &[
    BrowserSpec {
        name: "Chrome",
        data_root: "Google/Chrome",
        safe_storage_service: "Chrome Safe Storage",
        has_profiles: true,
    },
    BrowserSpec {
        name: "Brave",
        data_root: "BraveSoftware/Brave-Browser",
        safe_storage_service: "Brave Safe Storage",
        has_profiles: true,
    },
    BrowserSpec {
        name: "Edge",
        data_root: "Microsoft Edge",
        safe_storage_service: "Microsoft Edge Safe Storage",
        has_profiles: true,
    },
    BrowserSpec {
        name: "Chromium",
        data_root: "Chromium",
        safe_storage_service: "Chromium Safe Storage",
        has_profiles: true,
    },
    BrowserSpec {
        name: "Arc",
        data_root: "Arc",
        safe_storage_service: "Arc Safe Storage",
        has_profiles: true,
    },
    BrowserSpec {
        name: "Slack",
        data_root: "Slack",
        safe_storage_service: "Slack Safe Storage",
        has_profiles: false,
    },
];

/// Returns `true` when `name` matches the caller-supplied filter.
///
/// The filter is a case-insensitive substring match on the browser name. A
/// `None` filter matches everything.
fn browser_matches_filter(name: &str, filter: Option<&str>) -> bool {
    match filter {
        None => true,
        Some(f) => name.to_lowercase().contains(&f.to_lowercase()),
    }
}

/// Discover Chromium-style profiles that may hold Slack credentials.
///
/// `browser_filter` narrows the results by a case-insensitive substring match
/// on the browser name (e.g. `Some("chrome")`). Only profiles whose
/// `Local Storage/leveldb` directory actually exists on disk are returned.
///
/// On non-macOS platforms this currently returns an empty vector.
pub fn discover_profiles(browser_filter: Option<&str>) -> Vec<BrowserProfile> {
    #[cfg(target_os = "macos")]
    {
        discover_profiles_macos(browser_filter)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = browser_filter;
        Vec::new()
    }
}

/// macOS implementation of [`discover_profiles`].
#[cfg(target_os = "macos")]
fn discover_profiles_macos(browser_filter: Option<&str>) -> Vec<BrowserProfile> {
    let home = match std::env::var("HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h),
        _ => return Vec::new(),
    };
    let app_support = home.join("Library").join("Application Support");

    let mut profiles = Vec::new();

    for spec in BROWSER_SPECS {
        if !browser_matches_filter(spec.name, browser_filter) {
            continue;
        }

        let root = app_support.join(spec.data_root);
        if !root.is_dir() {
            continue;
        }

        if spec.has_profiles {
            for profile_name in enumerate_profile_dirs(&root) {
                push_if_leveldb_exists(
                    &mut profiles,
                    spec,
                    &root.join(&profile_name),
                    &profile_name,
                );
            }
        } else {
            // Slack desktop app: artifacts live at the data root itself.
            push_if_leveldb_exists(&mut profiles, spec, &root, "Default");
        }
    }

    profiles
}

/// Enumerate the profile subdirectory names (`Default`, `Profile N`) present
/// under a Chromium data root, sorted for deterministic output.
#[cfg(target_os = "macos")]
fn enumerate_profile_dirs(root: &std::path::Path) -> Vec<String> {
    let mut names = Vec::new();

    // "Default" is the canonical primary profile.
    if root.join("Default").is_dir() {
        names.push("Default".to_string());
    }

    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with("Profile ") {
                names.push(name);
            }
        }
    }

    names.sort();
    names.dedup();
    names
}

/// Build a [`BrowserProfile`] for `profile_dir` and push it onto `out` when the
/// expected `Local Storage/leveldb` directory exists.
#[cfg(target_os = "macos")]
fn push_if_leveldb_exists(
    out: &mut Vec<BrowserProfile>,
    spec: &BrowserSpec,
    profile_dir: &std::path::Path,
    profile_name: &str,
) {
    let leveldb = profile_dir.join("Local Storage").join("leveldb");
    if !leveldb.is_dir() {
        return;
    }
    out.push(BrowserProfile {
        browser: spec.name.to_string(),
        profile: profile_name.to_string(),
        local_storage_leveldb: leveldb,
        cookies_db: profile_dir.join("Cookies"),
        safe_storage_service: spec.safe_storage_service.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_none_matches_all() {
        assert!(browser_matches_filter("Chrome", None));
        assert!(browser_matches_filter("Slack", None));
    }

    #[test]
    fn filter_is_case_insensitive_substring() {
        assert!(browser_matches_filter("Chrome", Some("chrome")));
        assert!(browser_matches_filter("Chrome", Some("CHROME")));
        assert!(browser_matches_filter("Chrome", Some("hrom")));
        assert!(browser_matches_filter("Microsoft Edge", Some("edge")));
        assert!(browser_matches_filter("Brave", Some("BR")));
    }

    #[test]
    fn filter_rejects_non_matches() {
        assert!(!browser_matches_filter("Chrome", Some("firefox")));
        assert!(!browser_matches_filter("Arc", Some("chrome")));
    }

    #[test]
    fn every_spec_has_expected_service_name() {
        let lookup = |name: &str| {
            BROWSER_SPECS
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.safe_storage_service)
        };
        assert_eq!(lookup("Chrome"), Some("Chrome Safe Storage"));
        assert_eq!(lookup("Brave"), Some("Brave Safe Storage"));
        assert_eq!(lookup("Edge"), Some("Microsoft Edge Safe Storage"));
        assert_eq!(lookup("Chromium"), Some("Chromium Safe Storage"));
        assert_eq!(lookup("Arc"), Some("Arc Safe Storage"));
        assert_eq!(lookup("Slack"), Some("Slack Safe Storage"));
    }

    #[test]
    fn slack_app_has_no_profile_subdirs() {
        let slack = BROWSER_SPECS.iter().find(|s| s.name == "Slack").unwrap();
        assert!(!slack.has_profiles);
        // All Chromium browsers do use profile subdirs.
        for spec in BROWSER_SPECS.iter().filter(|s| s.name != "Slack") {
            assert!(spec.has_profiles, "{} should use profiles", spec.name);
        }
    }

    #[test]
    fn spec_names_are_unique() {
        let mut names: Vec<&str> = BROWSER_SPECS.iter().map(|s| s.name).collect();
        names.sort();
        let count = names.len();
        names.dedup();
        assert_eq!(count, names.len(), "duplicate browser spec names");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn discover_finds_synthetic_profiles() {
        use std::fs;

        let tmp =
            std::env::temp_dir().join(format!("slack-cli-profiles-test-{}", std::process::id()));
        let app_support = tmp.join("Library").join("Application Support");

        // Chrome with a Default and a Profile 1 that has a leveldb dir.
        let chrome = app_support.join("Google/Chrome");
        for p in ["Default", "Profile 1"] {
            fs::create_dir_all(chrome.join(p).join("Local Storage").join("leveldb")).unwrap();
        }
        // A profile without leveldb should be skipped.
        fs::create_dir_all(chrome.join("Profile 2")).unwrap();

        // Slack desktop app: artifacts at the top level.
        let slack = app_support.join("Slack");
        fs::create_dir_all(slack.join("Local Storage").join("leveldb")).unwrap();

        // Point HOME at our sandbox for the duration of this test.
        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &tmp);

        let all = discover_profiles(None);
        let chrome_only = discover_profiles(Some("chrome"));

        // Restore HOME before assertions so a panic can't leak state.
        match prev_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }

        let chrome_profiles: Vec<_> = all.iter().filter(|p| p.browser == "Chrome").collect();
        assert_eq!(chrome_profiles.len(), 2, "expected Default + Profile 1");
        assert!(all
            .iter()
            .any(|p| p.browser == "Slack" && p.profile == "Default"));

        assert!(chrome_only.iter().all(|p| p.browser == "Chrome"));
        assert_eq!(chrome_only.len(), 2);

        // Verify the derived paths for one entry.
        let def = chrome_profiles
            .iter()
            .find(|p| p.profile == "Default")
            .unwrap();
        assert!(def.local_storage_leveldb.ends_with("Local Storage/leveldb"));
        assert!(def.cookies_db.ends_with("Cookies"));
        assert_eq!(def.safe_storage_service, "Chrome Safe Storage");

        let _ = fs::remove_dir_all(&tmp);
    }
}
