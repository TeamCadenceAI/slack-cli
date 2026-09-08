//! Native browser/desktop token extraction for Slack CLI.
//!
//! This module implements the `--from-browser` path: it discovers locally
//! logged-in Slack sessions (in Chromium-family browsers and the Slack desktop
//! app), reads the per-team `xoxc` tokens from the browser's LevelDB local
//! storage, decrypts the shared `xoxd` cookie from the Cookies SQLite DB, and
//! pairs them into [`ExtractedWorkspace`] records.
//!
//! The heavy lifting lives in the submodules:
//! - [`profiles`] — discover browser data roots / profiles on disk.
//! - [`crypto`]   — derive the OS-specific "safe storage" key and decrypt
//!   cookie values (macOS is the priority target).
//! - [`cookies`]  — read + decrypt the shared `d` cookie from the Cookies DB.
//! - [`chromium`] — parse `xoxc` team tokens out of the LevelDB local storage.
//!
//! No network calls are made here — the caller is expected to `auth_test` any
//! returned workspaces to validate them.

pub mod chromium;
pub mod cookies;
pub mod crypto;
pub mod profiles;

use crate::auth::browser::BrowserTokens;
use crate::error::Result;

/// A single workspace's credentials discovered on the local machine.
pub struct ExtractedWorkspace {
    /// The paired browser tokens (`xoxc` token + `xoxd` cookie).
    pub tokens: BrowserTokens,
    /// The Slack team ID (e.g. `T012345ABCD`), if known.
    pub team_id: Option<String>,
    /// The workspace domain (the leading label of `<sub>.slack.com`), if known.
    pub team_domain: Option<String>,
    /// The human-readable workspace name, if known.
    pub team_name: Option<String>,
    /// Where this was found, formatted as `"<browser>/<profile>"`.
    pub source: String,
}

/// A workspace discovered locally, described by metadata only.
///
/// Unlike [`ExtractedWorkspace`], this carries **no** credentials: it is
/// produced by reading only the browser/app LevelDB local storage, so building
/// it never touches the OS Keychain, never decrypts the `xoxd` cookie, and
/// never makes a network call. It answers "which workspaces *could* I import?".
pub struct DiscoveredWorkspace {
    /// The Slack team ID (e.g. `T012345ABCD`), if known.
    pub team_id: Option<String>,
    /// The workspace domain (the leading label of `<sub>.slack.com`), if known.
    pub team_domain: Option<String>,
    /// The human-readable workspace name, if known.
    pub team_name: Option<String>,
    /// Where this was found, formatted as `"<browser>/<profile>"`.
    pub source: String,
}

/// Options controlling extraction.
pub struct ExtractOptions {
    /// Optional workspace URL to prefer, e.g. `myteam.slack.com` or
    /// `https://myteam.slack.com`. Only the leading domain label is used.
    pub url: Option<String>,
    /// Optional browser filter (e.g. `chrome`, `brave`). When `None`, all
    /// supported browsers/profiles are scanned.
    pub browser: Option<String>,
}

/// Discover and pair Slack credentials from locally logged-in browsers / the
/// Slack desktop app.
///
/// For each discovered profile this derives the safe-storage key, reads the
/// shared `xoxd` cookie, and extracts every per-team `xoxc` token, pairing them
/// into [`ExtractedWorkspace`] records. Profiles whose key cannot be derived are
/// skipped (with a diagnostic on stderr) so one broken profile does not abort
/// the whole scan.
///
/// If [`ExtractOptions::url`] is set, the result is filtered to workspaces whose
/// domain matches; if nothing matches, an empty list is returned so the caller
/// can report that the requested workspace was not found locally.
///
/// Results are deduplicated by team ID (falling back to the `xoxc` token).
/// Discover which Slack workspaces are signed into local browsers / the Slack
/// desktop app, **without** decrypting cookies or contacting the network.
///
/// This reads only the LevelDB local storage of each discovered profile (via
/// [`chromium::extract_tokens_from_leveldb`]) and reports team metadata. It is
/// the backing logic for `slack auth discover`. Results are deduplicated by
/// team ID (falling back to domain) and, if `browser` is set, limited to
/// matching browsers.
pub fn discover_workspaces(browser: Option<&str>) -> Vec<DiscoveredWorkspace> {
    let mut out: Vec<DiscoveredWorkspace> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for profile in profiles::discover_profiles(browser) {
        let teams = match chromium::extract_tokens_from_leveldb(&profile.local_storage_leveldb) {
            Ok(teams) => teams,
            Err(_) => continue,
        };
        let source = format!("{}/{}", profile.browser, profile.profile);
        for team in teams {
            let key = match (&team.team_id, &team.domain) {
                (Some(id), _) => format!("id:{id}"),
                (None, Some(dom)) => format!("dom:{dom}"),
                (None, None) => format!("xoxc:{}", team.xoxc),
            };
            if seen.insert(key) {
                out.push(DiscoveredWorkspace {
                    team_id: team.team_id,
                    team_domain: team.domain,
                    team_name: team.name,
                    source: source.clone(),
                });
            }
        }
    }

    out
}

pub fn extract_workspaces(opts: &ExtractOptions) -> Result<Vec<ExtractedWorkspace>> {
    let mut workspaces: Vec<ExtractedWorkspace> = Vec::new();

    for profile in profiles::discover_profiles(opts.browser.as_deref()) {
        // Derive the OS-specific decryption key candidates. If this fails (e.g.
        // no keychain entry, or unsupported OS), skip this profile but keep
        // going. A single service can expose several keys (e.g. the Slack app's
        // "Slack Key" vs "Slack App Store Key"); we try them all below.
        let keys = match crypto::safe_storage_keys(&profile.safe_storage_service) {
            Ok(keys) => keys,
            Err(err) => {
                eprintln!(
                    "skipping {}/{}: could not derive safe storage key: {}",
                    profile.browser, profile.profile, err
                );
                continue;
            }
        };

        // The `xoxd` cookie is shared across all workspaces in a profile.
        let xoxd = match cookies::read_slack_d_cookie(&profile.cookies_db, &keys) {
            Ok(Some(xoxd)) => xoxd,
            Ok(None) => {
                eprintln!(
                    "skipping {}/{}: no Slack 'd' cookie found",
                    profile.browser, profile.profile
                );
                continue;
            }
            Err(err) => {
                eprintln!(
                    "skipping {}/{}: could not read Slack cookie: {}",
                    profile.browser, profile.profile, err
                );
                continue;
            }
        };

        // Extract every per-team xoxc token from local storage.
        let teams = match chromium::extract_tokens_from_leveldb(&profile.local_storage_leveldb) {
            Ok(teams) => teams,
            Err(err) => {
                eprintln!(
                    "skipping {}/{}: could not read local storage: {}",
                    profile.browser, profile.profile, err
                );
                continue;
            }
        };

        let source = format!("{}/{}", profile.browser, profile.profile);
        for team in teams {
            workspaces.push(ExtractedWorkspace {
                tokens: BrowserTokens::new(team.xoxc, xoxd.clone()),
                team_id: team.team_id,
                team_domain: team.domain,
                team_name: team.name,
                source: source.clone(),
            });
        }
    }

    let workspaces = dedup_workspaces(workspaces);
    let workspaces = filter_by_url(workspaces, opts.url.as_deref());
    Ok(workspaces)
}

/// Extract the leading domain label from a workspace URL.
///
/// Accepts values like `myteam.slack.com`, `https://myteam.slack.com/`, or a
/// bare `myteam`, and returns the lowercased leading label (`myteam`). Returns
/// `None` if no meaningful label can be determined.
fn domain_label_from_url(url: &str) -> Option<String> {
    let mut s = url.trim();

    // Strip scheme.
    if let Some(rest) = s.split_once("://") {
        s = rest.1;
    }
    // Strip any path / query / port.
    s = s.split(['/', '?', '#', ':']).next().unwrap_or(s);
    let s = s.trim().trim_end_matches('.');

    // Take the leading label (before the first dot), which for
    // `myteam.slack.com` is `myteam`.
    let label = s.split('.').next().unwrap_or(s).trim();
    if label.is_empty() {
        None
    } else {
        Some(label.to_ascii_lowercase())
    }
}

/// Filter workspaces to those whose domain matches the URL's leading label.
///
/// If `url` is `None`, all workspaces are returned unchanged. If a URL is given,
/// only workspaces whose domain matches its leading label are returned — and if
/// none match, an empty list is returned (the caller reports "not found" rather
/// than importing unrelated workspaces the user did not ask for).
fn filter_by_url(
    workspaces: Vec<ExtractedWorkspace>,
    url: Option<&str>,
) -> Vec<ExtractedWorkspace> {
    let Some(target) = url.and_then(domain_label_from_url) else {
        return workspaces;
    };

    workspaces
        .into_iter()
        .filter(|w| {
            w.team_domain
                .as_deref()
                .is_some_and(|d| d.eq_ignore_ascii_case(&target))
        })
        .collect()
}

/// Deduplicate workspaces, keeping the first occurrence of each.
///
/// The dedup key is the team ID when present, otherwise the `xoxc` token, so
/// the same workspace discovered in multiple profiles is only reported once.
fn dedup_workspaces(workspaces: Vec<ExtractedWorkspace>) -> Vec<ExtractedWorkspace> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(workspaces.len());
    for w in workspaces {
        let key = match &w.team_id {
            Some(id) => format!("id:{id}"),
            None => format!("xoxc:{}", w.tokens.xoxc),
        };
        if seen.insert(key) {
            out.push(w);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(team_id: Option<&str>, domain: Option<&str>, xoxc: &str) -> ExtractedWorkspace {
        ExtractedWorkspace {
            tokens: BrowserTokens::new(xoxc.to_string(), "xoxd-shared".to_string()),
            team_id: team_id.map(str::to_string),
            team_domain: domain.map(str::to_string),
            team_name: None,
            source: "chrome/Default".to_string(),
        }
    }

    #[test]
    fn domain_label_parses_variants() {
        assert_eq!(
            domain_label_from_url("myteam.slack.com").as_deref(),
            Some("myteam")
        );
        assert_eq!(
            domain_label_from_url("https://myteam.slack.com/").as_deref(),
            Some("myteam")
        );
        assert_eq!(
            domain_label_from_url("http://MyTeam.slack.com/messages").as_deref(),
            Some("myteam")
        );
        assert_eq!(domain_label_from_url("myteam").as_deref(), Some("myteam"));
        assert_eq!(
            domain_label_from_url("myteam.slack.com:443").as_deref(),
            Some("myteam")
        );
        assert_eq!(domain_label_from_url("").as_deref(), None);
        assert_eq!(domain_label_from_url("   ").as_deref(), None);
    }

    #[test]
    fn filter_by_url_none_returns_all() {
        let all = vec![
            ws(Some("T1"), Some("alpha"), "xoxc-1"),
            ws(Some("T2"), Some("beta"), "xoxc-2"),
        ];
        let out = filter_by_url(all, None);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn filter_by_url_keeps_only_matching_domain() {
        let all = vec![
            ws(Some("T1"), Some("alpha"), "xoxc-1"),
            ws(Some("T2"), Some("beta"), "xoxc-2"),
        ];
        let out = filter_by_url(all, Some("beta.slack.com"));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].team_id.as_deref(), Some("T2"));
    }

    #[test]
    fn filter_by_url_returns_empty_when_no_match() {
        let all = vec![
            ws(Some("T1"), Some("alpha"), "xoxc-1"),
            ws(Some("T2"), Some("beta"), "xoxc-2"),
        ];
        let out = filter_by_url(all, Some("gamma.slack.com"));
        // A URL was explicitly requested but nothing matches -> empty, so the
        // caller reports "not found" instead of importing unrelated workspaces.
        assert!(out.is_empty());
    }

    #[test]
    fn filter_by_url_is_case_insensitive() {
        let all = vec![ws(Some("T1"), Some("Alpha"), "xoxc-1")];
        let out = filter_by_url(all, Some("ALPHA.slack.com"));
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn dedup_prefers_team_id() {
        let all = vec![
            ws(Some("T1"), Some("alpha"), "xoxc-a"),
            ws(Some("T1"), Some("alpha"), "xoxc-b"), // same team id, different token
            ws(Some("T2"), Some("beta"), "xoxc-c"),
        ];
        let out = dedup_workspaces(all);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].tokens.xoxc, "xoxc-a"); // first occurrence wins
    }

    #[test]
    fn dedup_falls_back_to_xoxc_when_no_team_id() {
        let all = vec![
            ws(None, Some("alpha"), "xoxc-dup"),
            ws(None, Some("alpha"), "xoxc-dup"),
            ws(None, Some("beta"), "xoxc-other"),
        ];
        let out = dedup_workspaces(all);
        assert_eq!(out.len(), 2);
    }
}
