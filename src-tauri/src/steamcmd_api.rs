//! SteamCMD appinfo client (api.steamcmd.net)
//!
//! Fetches Steam `appinfo` for an arbitrary appid from the public, no-auth
//! `api.steamcmd.net` mirror. This exposes depot metadata that DepotDownloader
//! does not surface in its output — notably per-depot `dlcappid` mappings and
//! the public-branch `buildid` for apps we are not directly downloading (e.g.
//! the Steamworks Common Redistributables app, 228980).
//!
//! This is a best-effort enrichment source. It is community-run and not part of
//! Valve's infrastructure, so every caller MUST treat a failure as non-fatal and
//! fall back to existing behavior. Nothing here is allowed to fail a job.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;

use crate::debug_console::debug_eprintln;

/// Cache of appid → parsed appinfo, to avoid repeated network calls within a session.
static APPINFO_CACHE: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);

/// Fetches and caches the raw appinfo JSON for an appid.
///
/// Returns the `data.<appid>` object on success. Returns `Err` (which callers
/// should treat as "enrichment unavailable") on any network/parse failure or if
/// the app is missing from the response.
fn fetch_appinfo(appid: &str) -> Result<Value, String> {
    if let Ok(guard) = APPINFO_CACHE.lock() {
        if let Some(cache) = guard.as_ref() {
            if let Some(cached) = cache.get(appid) {
                return Ok(cached.clone());
            }
        }
    }

    let url = format!("https://api.steamcmd.net/v1/info/{}", appid);
    debug_eprintln!("[STEAMCMD] Fetching appinfo from: {}", url);

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "OmniPacker/1.0")
        .send()
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("steamcmd API returned status {}", response.status()));
    }

    let body: Value = response
        .json()
        .map_err(|e| format!("Failed to parse steamcmd JSON: {}", e))?;

    // Expected shape: { "status": "success", "data": { "<appid>": { ... } } }
    let app_data = body
        .get("data")
        .and_then(|d| d.get(appid))
        .cloned()
        .ok_or_else(|| format!("appid {} not present in steamcmd response", appid))?;

    if let Ok(mut guard) = APPINFO_CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(appid.to_string(), app_data.clone());
    }

    Ok(app_data)
}

/// Returns a map of depot_id → dlcappid for every depot in `appid` that declares one.
///
/// Depots without a `dlcappid` (the base game depots, shared redistributables, etc.)
/// are simply absent from the map. On any failure this returns an empty map, so the
/// caller transparently degrades to "no dlcappid information".
pub fn fetch_depot_dlcappids(appid: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();

    let app_data = match fetch_appinfo(appid) {
        Ok(data) => data,
        Err(err) => {
            debug_eprintln!("[STEAMCMD] dlcappid lookup unavailable for {}: {}", appid, err);
            return result;
        }
    };

    let Some(depots) = app_data.get("depots").and_then(|d| d.as_object()) else {
        return result;
    };

    for (depot_id, depot) in depots {
        // Skip the non-depot keys that live alongside numeric depot entries
        // (e.g. "branches", "baselanguages", "overridescddb").
        if !depot_id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Some(dlcappid) = depot.get("dlcappid").and_then(|v| v.as_str()) {
            result.insert(depot_id.clone(), dlcappid.to_string());
        }
    }

    result
}

/// Returns a map of depot_id → owner appid for every depot in `appid` that is a
/// shared install (`sharedinstall` == "1").
///
/// Steam marks redistributables and other cross-app depots with `sharedinstall`
/// and a `depotfromapp` pointing at the app that actually owns the depot (e.g.
/// the VC++/DirectX redists point at 228980, "Steamworks Common
/// Redistributables"). This is the authoritative, data-driven way to recognize a
/// shared depot regardless of whether it appears in any hardcoded list. When
/// `depotfromapp` is absent, the entry is still returned with `appid` itself as a
/// best-effort owner. On any failure this returns an empty map, so callers
/// transparently degrade to the hardcoded `is_shared_depot` list.
pub fn fetch_shared_depots(appid: &str) -> HashMap<String, String> {
    let app_data = match fetch_appinfo(appid) {
        Ok(data) => data,
        Err(err) => {
            debug_eprintln!("[STEAMCMD] shared-depot lookup unavailable for {}: {}", appid, err);
            return HashMap::new();
        }
    };

    parse_shared_depots(&app_data, appid)
}

/// Pure parser behind [`fetch_shared_depots`]: extracts depot_id → owner appid for
/// every `sharedinstall == "1"` depot, defaulting the owner to `appid` when
/// `depotfromapp` is absent. Split out so it can be unit-tested without the network.
fn parse_shared_depots(app_data: &Value, appid: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();

    let Some(depots) = app_data.get("depots").and_then(|d| d.as_object()) else {
        return result;
    };

    for (depot_id, depot) in depots {
        if !depot_id.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let is_shared = depot
            .get("sharedinstall")
            .and_then(|v| v.as_str())
            .map(|s| s == "1")
            .unwrap_or(false);
        if !is_shared {
            continue;
        }
        let owner = depot
            .get("depotfromapp")
            .and_then(|v| v.as_str())
            .unwrap_or(appid)
            .to_string();
        result.insert(depot_id.clone(), owner);
    }

    result
}

/// Returns the human-readable `common.name` for an app, if available.
///
/// Used to name shared/redistributable and DLC depots after the app that owns
/// them (e.g. 228980 → "Steamworks Common Redistributables"). Returns `None` on
/// any failure; callers fall back to other naming strategies.
pub fn fetch_app_name(appid: &str) -> Option<String> {
    let app_data = match fetch_appinfo(appid) {
        Ok(data) => data,
        Err(err) => {
            debug_eprintln!("[STEAMCMD] app-name lookup unavailable for {}: {}", appid, err);
            return None;
        }
    };

    app_data
        .get("common")
        .and_then(|c| c.get("name"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Returns the public-branch buildid for an app, if available.
///
/// Used to populate the redistributables manifest (228980) with the same buildid
/// real Steam records. Returns `None` on any failure; callers fall back to "0".
pub fn fetch_public_buildid(appid: &str) -> Option<String> {
    let app_data = match fetch_appinfo(appid) {
        Ok(data) => data,
        Err(err) => {
            debug_eprintln!("[STEAMCMD] buildid lookup unavailable for {}: {}", appid, err);
            return None;
        }
    };

    app_data
        .get("depots")
        .and_then(|d| d.get("branches"))
        .and_then(|b| b.get("public"))
        .and_then(|p| p.get("buildid"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Returns Steam's canonical install-folder name for an app, if available.
///
/// This is the `config.installdir` value (e.g. "ProjectZomboid"), which is the
/// exact `steamapps/common/<folder>` name real Steam uses. It differs from both
/// the store `common.name` ("Project Zomboid") and the per-depot names
/// ("Project Zomboid - windows"), so it is the only reliable source for the
/// merged depot folder name. Returns `None` on any failure; callers must fall
/// back to deriving a name from the depot/game name.
pub fn fetch_install_dir(appid: &str) -> Option<String> {
    let app_data = match fetch_appinfo(appid) {
        Ok(data) => data,
        Err(err) => {
            debug_eprintln!("[STEAMCMD] installdir lookup unavailable for {}: {}", appid, err);
            return None;
        }
    };

    app_data
        .get("config")
        .and_then(|c| c.get("installdir"))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Clears the appinfo cache (useful for testing or forcing refresh).
#[allow(dead_code)]
pub fn clear_cache() {
    if let Ok(mut guard) = APPINFO_CACHE.lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an appinfo Value matching the real api.steamcmd.net shape.
    fn sample_appinfo() -> Value {
        serde_json::json!({
            "config": { "installdir": "ProjectZomboid" },
            "depots": {
                "72840": { "dlcappid": "72840", "manifests": { "public": { "gid": "1" } } },
                "22475": { "dlcappid": "22475" },
                "22381": { "manifests": { "public": { "gid": "2" } } },
                "branches": {
                    "public": { "buildid": "1510068", "timeupdated": "123" }
                }
            }
        })
    }

    /// Extracts dlcappids from a pre-parsed appinfo Value (mirrors the parsing in
    /// fetch_depot_dlcappids, without the network layer).
    fn extract_dlcappids(app_data: &Value) -> HashMap<String, String> {
        let mut result = HashMap::new();
        let depots = app_data.get("depots").and_then(|d| d.as_object()).unwrap();
        for (depot_id, depot) in depots {
            if !depot_id.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if let Some(dlcappid) = depot.get("dlcappid").and_then(|v| v.as_str()) {
                result.insert(depot_id.clone(), dlcappid.to_string());
            }
        }
        result
    }

    #[test]
    fn test_parse_dlcappids_only_dlc_depots() {
        let data = sample_appinfo();
        let map = extract_dlcappids(&data);

        assert_eq!(map.get("72840"), Some(&"72840".to_string()));
        assert_eq!(map.get("22475"), Some(&"22475".to_string()));
        // Base-game depot without dlcappid is absent
        assert!(!map.contains_key("22381"));
        // "branches" is not a depot and must be skipped
        assert!(!map.contains_key("branches"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_parse_install_dir() {
        let data = sample_appinfo();
        let installdir = data
            .get("config")
            .and_then(|c| c.get("installdir"))
            .and_then(|v| v.as_str());
        assert_eq!(installdir, Some("ProjectZomboid"));
    }

    #[test]
    fn test_parse_public_buildid() {
        let data = sample_appinfo();
        let buildid = data
            .get("depots")
            .and_then(|d| d.get("branches"))
            .and_then(|b| b.get("public"))
            .and_then(|p| p.get("buildid"))
            .and_then(|v| v.as_str());
        assert_eq!(buildid, Some("1510068"));
    }

    /// Appinfo matching The Crust (appid 1465470): one real game depot plus two
    /// redist depots flagged `sharedinstall` and owned by 228980.
    fn the_crust_appinfo() -> Value {
        serde_json::json!({
            "common": { "name": "The Crust" },
            "config": { "installdir": "The Crust" },
            "depots": {
                "1465471": { "config": { "oslist": "windows" } },
                "228989": {
                    "config": { "oslist": "windows" },
                    "depotfromapp": "228980",
                    "sharedinstall": "1"
                },
                "228990": {
                    "config": { "oslist": "windows" },
                    "depotfromapp": "228980",
                    "sharedinstall": "1"
                },
                "branches": { "public": { "buildid": "23867425" } }
            }
        })
    }

    #[test]
    fn test_parse_shared_depots_flags_redists() {
        let data = the_crust_appinfo();
        let map = parse_shared_depots(&data, "1465470");

        // Both redist depots are recognized as shared and owned by 228980.
        assert_eq!(map.get("228989"), Some(&"228980".to_string()));
        assert_eq!(map.get("228990"), Some(&"228980".to_string()));
        // The real game depot is NOT shared.
        assert!(!map.contains_key("1465471"));
        // "branches" is not a depot.
        assert!(!map.contains_key("branches"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn test_parse_shared_depots_defaults_owner_to_self() {
        // A shared depot with no explicit depotfromapp falls back to the app itself.
        let data = serde_json::json!({
            "depots": {
                "999": { "sharedinstall": "1" }
            }
        });
        let map = parse_shared_depots(&data, "555");
        assert_eq!(map.get("999"), Some(&"555".to_string()));
    }

    #[test]
    fn test_parse_app_name() {
        let data = the_crust_appinfo();
        let name = data
            .get("common")
            .and_then(|c| c.get("name"))
            .and_then(|v| v.as_str());
        assert_eq!(name, Some("The Crust"));
    }
}
