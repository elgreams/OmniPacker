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
    eprintln!("[STEAMCMD] Fetching appinfo from: {}", url);

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
            eprintln!("[STEAMCMD] dlcappid lookup unavailable for {}: {}", appid, err);
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

/// Returns the public-branch buildid for an app, if available.
///
/// Used to populate the redistributables manifest (228980) with the same buildid
/// real Steam records. Returns `None` on any failure; callers fall back to "0".
pub fn fetch_public_buildid(appid: &str) -> Option<String> {
    let app_data = match fetch_appinfo(appid) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("[STEAMCMD] buildid lookup unavailable for {}: {}", appid, err);
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
}
