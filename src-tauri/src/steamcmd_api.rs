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
    fetch_appinfo_uncached(appid)
}

/// Always hits the network, then refreshes the cache entry. Used where a
/// session-old answer would be wrong (the post-download build check), and so
/// every later lookup in the same job sees the fresh data too.
fn fetch_appinfo_uncached(appid: &str) -> Result<Value, String> {

    let url = format!("https://api.steamcmd.net/v1/info/{}", appid);
    debug_eprintln!("[STEAMCMD] Fetching appinfo from: {}", url);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;
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

/// One depot as shown in the advanced depot picker.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DepotListEntry {
    pub depot_id: String,
    /// Display name: the DLC's name for DLC depots, else the depot's own
    /// appinfo name, else empty (the UI shows the ID).
    pub name: String,
    pub oslist: String,
    pub osarch: String,
    pub language: String,
    pub realm: String,
    pub dlc_appid: String,
    /// Shared redistributable (owned by another app, e.g. 228980).
    pub shared: bool,
    /// Current public manifest, when the mirror exposes it.
    pub public_manifest: String,
}

/// Lists an app's depots for the advanced picker (best-effort, community
/// mirror). DepotDownloader still enforces ownership/branch rules at download.
#[tauri::command]
pub async fn list_app_depots(app_id: String) -> Result<Vec<DepotListEntry>, String> {
    let app_id = app_id.trim().to_string();
    if app_id.is_empty() || !app_id.chars().all(|c| c.is_ascii_digit()) {
        return Err("Enter a numeric AppID first.".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let data = fetch_appinfo(&app_id)?;
        let mut entries = parse_depot_list(&data);
        // Name DLC depots after their DLC app (one lookup per distinct DLC).
        let mut cache: HashMap<String, Option<String>> = HashMap::new();
        for entry in entries.iter_mut().filter(|e| e.name.is_empty() && !e.dlc_appid.is_empty()) {
            let name = cache
                .entry(entry.dlc_appid.clone())
                .or_insert_with(|| fetch_app_name(&entry.dlc_appid))
                .clone();
            if let Some(name) = name {
                entry.name = name;
            }
        }
        Ok(entries)
    })
    .await
    .map_err(|e| format!("Depot lookup failed: {e}"))?
}

fn parse_depot_list(app_data: &Value) -> Vec<DepotListEntry> {
    let Some(depots) = app_data.get("depots").and_then(|d| d.as_object()) else {
        return Vec::new();
    };
    let s = |v: Option<&Value>| v.and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut out: Vec<DepotListEntry> = depots
        .iter()
        .filter(|(id, _)| id.chars().all(|c| c.is_ascii_digit()))
        .map(|(id, d)| {
            let config = d.get("config");
            DepotListEntry {
                depot_id: id.clone(),
                name: s(d.get("name")),
                oslist: s(config.and_then(|c| c.get("oslist"))),
                osarch: s(config.and_then(|c| c.get("osarch"))),
                language: s(config.and_then(|c| c.get("language"))),
                realm: s(config.and_then(|c| c.get("realm"))),
                dlc_appid: s(d.get("dlcappid")),
                shared: d.get("sharedinstall").and_then(|v| v.as_str()) == Some("1")
                    || crate::shared_depots::is_shared_depot(id),
                public_manifest: s(d
                    .get("manifests")
                    .and_then(|m| m.get("public"))
                    .and_then(|p| p.get("gid"))),
            }
        })
        .collect();
    out.sort_by_key(|e| e.depot_id.parse::<u64>().unwrap_or(u64::MAX));
    out
}

/// What appinfo says about an app's DLC, for the post-download DLC report.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DlcCatalog {
    /// DLC appids from `extended.listofdlc`, in listed order.
    pub dlc_appids: Vec<String>,
    /// depot_id → dlcappid for every depot of the base app that belongs to a DLC.
    pub depot_dlc: HashMap<String, String>,
    /// depot_id → oslist (comma-separated) for those DLC depots, when declared.
    pub depot_oslist: HashMap<String, String>,
}

/// Fetches the DLC catalog for an app. `None` when the lookup fails.
pub fn fetch_dlc_catalog(appid: &str) -> Option<DlcCatalog> {
    match fetch_appinfo(appid) {
        Ok(data) => Some(parse_dlc_catalog(&data)),
        Err(err) => {
            debug_eprintln!("[STEAMCMD] DLC catalog unavailable for {}: {}", appid, err);
            None
        }
    }
}

fn parse_dlc_catalog(app_data: &Value) -> DlcCatalog {
    let mut catalog = DlcCatalog::default();
    if let Some(list) = app_data
        .get("extended")
        .and_then(|e| e.get("listofdlc"))
        .and_then(|v| v.as_str())
    {
        catalog.dlc_appids = list
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .collect();
    }
    if let Some(depots) = app_data.get("depots").and_then(|d| d.as_object()) {
        for (depot_id, depot) in depots {
            if !depot_id.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if let Some(dlc) = depot.get("dlcappid").and_then(|v| v.as_str()) {
                catalog.depot_dlc.insert(depot_id.clone(), dlc.to_string());
                if let Some(os) = depot
                    .get("config")
                    .and_then(|c| c.get("oslist"))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    catalog.depot_oslist.insert(depot_id.clone(), os.to_string());
                }
            }
        }
    }
    catalog
}

/// Returns the current buildid of `branch` for an app, fetched fresh (never
/// from the session cache). `None` when the lookup fails or the branch isn't
/// listed (e.g. a password-protected beta the mirror can't see).
pub fn fetch_branch_buildid_fresh(appid: &str, branch: &str) -> Option<String> {
    match fetch_appinfo_uncached(appid) {
        Ok(data) => parse_branch_buildid(&data, branch),
        Err(err) => {
            debug_eprintln!("[STEAMCMD] branch buildid lookup unavailable for {}: {}", appid, err);
            None
        }
    }
}

/// Pure parser behind [`fetch_branch_buildid_fresh`]. Branch names are matched
/// case-insensitively (Steam keys are lowercase; users may type "Beta").
fn parse_branch_buildid(app_data: &Value, branch: &str) -> Option<String> {
    let branch = if branch.trim().is_empty() { "public" } else { branch.trim() };
    app_data
        .get("depots")
        .and_then(|d| d.get("branches"))
        .and_then(|b| b.as_object())?
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(branch))
        .and_then(|(_, info)| info.get("buildid"))
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
    fn test_parse_depot_list() {
        let data = serde_json::json!({ "depots": {
            "3321465": { "config": { "language": "french", "oslist": "windows", "osarch": "64" },
                         "manifests": { "public": { "gid": "111" } } },
            "228989": { "config": { "oslist": "windows" }, "depotfromapp": "228980", "sharedinstall": "1" },
            "5001841": { "config": { "oslist": "windows" }, "dlcappid": "5001840" },
            "2593343": { "config": { "oslist": "windows", "realm": "steamchina" } },
            "branches": { "public": { "buildid": "1" } }
        }});
        let list = parse_depot_list(&data);
        let ids: Vec<&str> = list.iter().map(|e| e.depot_id.as_str()).collect();
        assert_eq!(ids, vec!["228989", "2593343", "3321465", "5001841"]);
        assert!(list[0].shared);
        assert_eq!(list[1].realm, "steamchina");
        assert_eq!(list[2].language, "french");
        assert_eq!(list[2].public_manifest, "111");
        assert_eq!(list[3].dlc_appid, "5001840");
    }

    #[test]
    fn test_parse_dlc_catalog() {
        // Crimson Desert (3321460) shape, trimmed.
        let data = serde_json::json!({
            "extended": { "listofdlc": "4024620,4024630,4193060,5001840" },
            "depots": {
                "3321461": { "config": { "oslist": "windows" } },
                "4783050": { "config": { "oslist": "windows" }, "dlcappid": "4783050" },
                "5001841": { "config": { "oslist": "windows" }, "dlcappid": "5001840" },
                "5001842": { "config": { "oslist": "macos" }, "dlcappid": "5001840" },
                "branches": { "public": { "buildid": "1" } }
            }
        });
        let c = parse_dlc_catalog(&data);
        assert_eq!(c.dlc_appids, vec!["4024620", "4024630", "4193060", "5001840"]);
        assert_eq!(c.depot_dlc.get("5001841").map(String::as_str), Some("5001840"));
        assert_eq!(c.depot_dlc.get("4783050").map(String::as_str), Some("4783050"));
        assert!(!c.depot_dlc.contains_key("3321461"));
        assert_eq!(c.depot_oslist.get("5001842").map(String::as_str), Some("macos"));
    }

    #[test]
    fn test_parse_branch_buildid() {
        let data = serde_json::json!({
            "depots": { "branches": {
                "public": { "buildid": "25446194" },
                "beta": { "buildid": "25500000", "pwdrequired": "1" }
            }}
        });
        assert_eq!(parse_branch_buildid(&data, "public").as_deref(), Some("25446194"));
        assert_eq!(parse_branch_buildid(&data, "").as_deref(), Some("25446194"));
        assert_eq!(parse_branch_buildid(&data, "Beta").as_deref(), Some("25500000"));
        assert_eq!(parse_branch_buildid(&data, "missing"), None);
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
