use serde::Deserialize;
use std::collections::HashMap;

const STEAM_STORE_API_URL: &str = "https://store.steampowered.com/api/appdetails";

/// Response from Steam's appdetails API
#[derive(Debug, Deserialize)]
struct AppDetailsResponse {
    success: bool,
    data: Option<AppData>,
}

/// App data from Steam's appdetails API
#[derive(Debug, Deserialize)]
struct AppData {
    name: String,
    #[serde(default)]
    steam_appid: u64,
}

/// Information fetched from Steam's public API
#[derive(Debug, Clone)]
pub struct SteamAppInfo {
    /// Human-readable game name
    pub name: String,
    /// Steam App ID (as returned by API)
    #[allow(dead_code)] // Fetched from API but not currently used; kept for debugging/future use
    pub steam_appid: u64,
}

/// Depot IDs belonging to the Steamworks Common Redistributables app (228980).
///
/// These are the VC++ runtimes, DirectX, OpenAL and .NET redistributables that
/// Steam installs into the shared `Steamworks Shared` folder. The full set was
/// verified against a real `appmanifest_228980.acf` install.
const REDIST_DEPOTS: &[&str] = &[
    "228981", // VC++ 2005
    "228982", // VC++ 2008
    "228983", // VC++ 2010
    "228984", // VC++ 2012
    "228985", // VC++ 2013
    "228986", // VC++ 2015
    "228987", // OpenAL
    "228988", // VC++ 2019
    "228989", // VC++ 2022
    "228990", // DirectX (Jun 2010)
    "229006", // .NET 4.7
];

/// Checks if a depot ID is a known shared Steam depot (redistributables, runtimes, etc.)
pub fn is_shared_depot(depot_id: &str) -> bool {
    REDIST_DEPOTS.contains(&depot_id)
        || matches!(
            depot_id,
            // Steamworks Common Redistributables app depot itself
            "228980" |
            // Steam Linux Runtime
            "1391110" | "1628210" | "1826330"
        )
}

/// Gets a human-readable name for a shared Steam depot
fn get_shared_depot_name(depot_id: &str) -> Option<String> {
    // All redistributables live in the single "Steamworks Shared" folder,
    // matching real Steam's on-disk layout.
    if depot_id == "228980" || REDIST_DEPOTS.contains(&depot_id) {
        return Some("Steamworks Shared".to_string());
    }
    match depot_id {
        // Steam Linux Runtime
        "1391110" => Some("SteamLinuxRuntime".to_string()),
        "1628210" => Some("SteamLinuxRuntime_soldier".to_string()),
        "1826330" => Some("SteamLinuxRuntime_sniper".to_string()),
        _ => None,
    }
}

/// Returns the owner appid for a shared depot
///
/// In Steam's .acf format, shared depots are listed in a `SharedDepots` section
/// with the format: `"depot_id" "owner_appid"`
pub fn get_shared_depot_owner(depot_id: &str) -> &'static str {
    // All redistributables are owned by the Steamworks Common Redistributables app
    if depot_id == "228980" || REDIST_DEPOTS.contains(&depot_id) {
        return "228980";
    }
    match depot_id {
        // Steam Linux Runtime - these are their own owners
        "1391110" => "1391110",  // Steam Linux Runtime (base)
        "1628210" => "1628350",  // Steam Linux Runtime - Soldier (owned by app 1628350)
        "1826330" => "1826330",  // Steam Linux Runtime - Sniper
        // Default to Steamworks Common Redistributables app
        _ => "228980",
    }
}

/// Gets a human-readable name for a depot
///
/// Strategy:
/// 1. If it's the primary depot, use the game name
/// 2. If it's a common Steam shared depot, use the known name
/// 3. Otherwise, use depot_{id} fallback
pub fn get_depot_name(depot_id: &str, is_primary: bool, game_name: &str) -> String {
    // If it's the primary depot, use the game name
    if is_primary {
        return game_name.to_string();
    }

    // Check if it's a known shared depot
    if let Some(name) = get_shared_depot_name(depot_id) {
        return name;
    }

    // Otherwise, use fallback
    format!("depot_{}", depot_id)
}

/// Fetches app info from Steam's public store API
///
/// This uses the public endpoint which does NOT require authentication:
/// https://store.steampowered.com/api/appdetails?appids=<appid>
///
/// Rate limit: ~200 requests per 5 minutes
pub fn fetch_app_info(appid: &str) -> Result<SteamAppInfo, String> {
    let url = format!("{}?appids={}", STEAM_STORE_API_URL, appid);

    let response = reqwest::blocking::get(&url)
        .map_err(|e| format!("Failed to fetch Steam app info: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Steam API returned status {}: {}",
            response.status(),
            response.status().canonical_reason().unwrap_or("Unknown")
        ));
    }

    let body: HashMap<String, AppDetailsResponse> = response
        .json()
        .map_err(|e| format!("Failed to parse Steam API response: {}", e))?;

    let app_response = body
        .get(appid)
        .ok_or_else(|| format!("No data returned for appid {}", appid))?;

    if !app_response.success {
        return Err(format!(
            "Steam API returned success=false for appid {}. The app may not exist or be restricted.",
            appid
        ));
    }

    let data = app_response
        .data
        .as_ref()
        .ok_or_else(|| format!("No app data in response for appid {}", appid))?;

    Ok(SteamAppInfo {
        name: data.name.clone(),
        steam_appid: data.steam_appid,
    })
}

/// Sanitizes a game name for use as a filesystem path segment (the top-level
/// output folder and the `steamapps/common/<installdir>` folder).
///
/// Rules (from ROADMAP.md):
/// - Preserve casing from metadata
/// - Replace spaces with `.`
/// - Remove apostrophes and non-ASCII characters
/// - Keep numeric characters
///
/// In addition to the ROADMAP rules, every character that is illegal in a
/// Windows path segment is removed: `< > : " / \ | ? *` and control characters.
/// The ROADMAP only called out colons and slashes, but the others fail the same
/// way on Windows (`os error 267`), e.g. the `?` in "Who's Your Daddy?!", and a
/// `"` would additionally break the quoted `installdir` value in the `.acf`.
pub fn sanitize_game_name(name: &str) -> String {
    /// Characters forbidden in a Windows path segment (besides control chars).
    const WINDOWS_ILLEGAL: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

    let sanitized: String = name
        .chars()
        .filter_map(|c| {
            if c == ' ' {
                Some('.')
            } else if c == '\''
                || !c.is_ascii()
                || c.is_control()
                || WINDOWS_ILLEGAL.contains(&c)
            {
                None
            } else {
                Some(c)
            }
        })
        .collect();

    // Windows silently strips trailing dots and spaces from path segments, which
    // would make the created folder name diverge from the `.acf` installdir value
    // (e.g. "S.T.A.L.K.E.R." -> folder "S.T.A.L.K.E.R" but installdir keeps the
    // dot). Trim them so both sides stay consistent. Spaces are already mapped to
    // dots above, so trimming dots is sufficient.
    sanitized.trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_game_name_basic() {
        assert_eq!(sanitize_game_name("Balatro"), "Balatro");
        assert_eq!(sanitize_game_name("Half-Life 2"), "Half-Life.2");
        assert_eq!(sanitize_game_name("Portal 2"), "Portal.2");
    }

    #[test]
    fn test_sanitize_game_name_special_chars() {
        // Apostrophes removed
        assert_eq!(sanitize_game_name("Assassin's Creed"), "Assassins.Creed");
        // Colons removed
        assert_eq!(sanitize_game_name("Fallout: New Vegas"), "Fallout.New.Vegas");
        // Slashes removed
        assert_eq!(sanitize_game_name("Game/Name\\Test"), "GameNameTest");
    }

    #[test]
    fn test_sanitize_game_name_windows_illegal_chars() {
        // Every character illegal in a Windows path segment must be removed,
        // not just the colon/slashes the ROADMAP originally called out.
        assert_eq!(sanitize_game_name("Who's Your Daddy?!"), "Whos.Your.Daddy!");
        assert_eq!(sanitize_game_name("Quake II <RTX>"), "Quake.II.RTX");
        assert_eq!(sanitize_game_name("A|B*C?D"), "ABCD");
        assert_eq!(sanitize_game_name("Say \"Hello\""), "Say.Hello");
        // Control characters (e.g. tab) removed
        assert_eq!(sanitize_game_name("Tab\tName"), "TabName");
    }

    #[test]
    fn test_sanitize_game_name_trailing_dots_and_spaces() {
        // Windows strips trailing dots/spaces from folder names; trim them so the
        // created folder matches the .acf installdir value.
        assert_eq!(sanitize_game_name("S.T.A.L.K.E.R."), "S.T.A.L.K.E.R");
        assert_eq!(sanitize_game_name("Portal 2 "), "Portal.2");
        assert_eq!(sanitize_game_name("Trailing..."), "Trailing");
    }

    #[test]
    fn test_sanitize_game_name_non_ascii() {
        // Non-ASCII removed
        assert_eq!(sanitize_game_name("Café Game™"), "Caf.Game");
        assert_eq!(sanitize_game_name("日本語ゲーム"), "");
    }

    #[test]
    fn test_sanitize_game_name_preserves_case() {
        assert_eq!(sanitize_game_name("CamelCaseGame"), "CamelCaseGame");
        assert_eq!(sanitize_game_name("ALLCAPS"), "ALLCAPS");
        assert_eq!(sanitize_game_name("lowercase"), "lowercase");
    }

    #[test]
    fn test_sanitize_game_name_complex() {
        assert_eq!(
            sanitize_game_name("The Witcher 3: Wild Hunt - Game of the Year Edition"),
            "The.Witcher.3.Wild.Hunt.-.Game.of.the.Year.Edition"
        );
    }
}
