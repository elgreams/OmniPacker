//! Lightweight "check and warn" update detector.
//!
//! Queries the GitHub Releases API for the latest published OmniPacker release
//! and compares it against the running version. This is intentionally NOT a
//! self-updater (no signing, no latest.json, no downloaded artifacts) — it only
//! surfaces a banner pointing the user at the release page.
//!
//! Network and parse failures are returned as `Err` and swallowed silently by
//! the frontend; a failed check must never block launch or show an error.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/elgreams/OmniPacker/releases/latest";

/// Subset of the GitHub release payload we care about.
#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
}

/// Result handed to the frontend.
#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    /// Running app version (e.g. "1.2.0").
    pub current: String,
    /// Latest released version, leading "v" stripped (e.g. "1.3.0").
    pub latest: String,
    /// Release page URL to open in the browser.
    pub url: String,
    /// True only when `latest` is strictly newer than `current`.
    pub update_available: bool,
}

/// Parses a "major.minor.patch" version into a comparable tuple. A leading "v"
/// is stripped. Missing/non-numeric components are treated as 0, so "1.2" and
/// "v1.2.0" both parse. Extra components beyond patch are ignored.
fn parse_version(raw: &str) -> (u64, u64, u64) {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    // Drop any pre-release/build suffix (e.g. "1.2.0-rc1" or "1.2.0+op4").
    let core = trimmed
        .split(['-', '+'])
        .next()
        .unwrap_or(trimmed);
    let mut parts = core.split('.');
    let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    (major, minor, patch)
}

/// Returns true if `latest` is strictly newer than `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    parse_version(latest) > parse_version(current)
}

#[tauri::command]
pub fn check_for_update(app_handle: AppHandle) -> Result<UpdateInfo, String> {
    let current = app_handle.package_info().version.to_string();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;

    let response = client
        .get(LATEST_RELEASE_URL)
        // GitHub's API rejects requests without a User-Agent.
        .header("User-Agent", "OmniPacker")
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|e| format!("Update check request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Update check returned HTTP {}",
            response.status().as_u16()
        ));
    }

    let release: GithubRelease = response
        .json()
        .map_err(|e| format!("Failed to parse release info: {e}"))?;

    let latest = release
        .tag_name
        .trim()
        .trim_start_matches(['v', 'V'])
        .to_string();

    let update_available = is_newer(&latest, &current);

    Ok(UpdateInfo {
        current,
        latest,
        url: release.html_url,
        update_available,
    })
}

/// Opens an external URL (the release page) in the user's default browser.
#[tauri::command]
pub fn open_external_url(app_handle: AppHandle, url: String) -> Result<(), String> {
    // Only allow http(s) URLs so this can't be coaxed into opening files/programs.
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Refusing to open non-http URL.".to_string());
    }
    app_handle
        .opener()
        .open_url(url, None::<String>)
        .map_err(|e| format!("Failed to open URL: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_with_and_without_prefix() {
        assert_eq!(parse_version("1.2.0"), (1, 2, 0));
        assert_eq!(parse_version("v1.2.0"), (1, 2, 0));
        assert_eq!(parse_version("V1.3"), (1, 3, 0));
        assert_eq!(parse_version("1.2.0+op4"), (1, 2, 0));
        assert_eq!(parse_version("1.2.0-rc1"), (1, 2, 0));
    }

    #[test]
    fn detects_newer_versions() {
        assert!(is_newer("1.3.0", "1.2.0"));
        assert!(is_newer("1.2.1", "1.2.0"));
        assert!(is_newer("2.0.0", "1.9.9"));
        assert!(is_newer("v1.3.0", "1.2.0"));
    }

    #[test]
    fn ignores_equal_or_older_versions() {
        assert!(!is_newer("1.2.0", "1.2.0"));
        assert!(!is_newer("1.1.9", "1.2.0"));
        assert!(!is_newer("1.2.0", "1.2.1"));
    }
}
