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

/// Checks if a depot ID is a known shared Steam depot (redistributables, runtimes, etc.)
pub fn is_shared_depot(depot_id: &str) -> bool {
    matches!(
        depot_id,
        // Steamworks Common Redistributables
        "228980" | "228989" | "228990" |
        // DirectX redistributables
        "228983" | "228984" | "228986" |
        // Visual C++ redistributables
        "228985" |
        // OpenAL
        "228987" |
        // Steam Linux Runtime
        "1391110" | "1628210" | "1826330"
    )
}

/// Gets a human-readable name for a shared Steam depot
fn get_shared_depot_name(depot_id: &str) -> Option<String> {
    match depot_id {
        // Steamworks Common Redistributables
        "228980" | "228989" | "228990" => Some("Steamworks Shared".to_string()),
        // DirectX redistributables
        "228983" | "228984" | "228986" => Some("DirectX".to_string()),
        // Visual C++ redistributables
        "228985" => Some("VC Redist".to_string()),
        // OpenAL
        "228987" => Some("OpenAL".to_string()),
        // Steam Linux Runtime
        "1391110" => Some("SteamLinuxRuntime".to_string()),
        "1628210" => Some("SteamLinuxRuntime_soldier".to_string()),
        "1826330" => Some("SteamLinuxRuntime_sniper".to_string()),
        _ => None,
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

/// Sanitizes a game name for use in output folder names
///
/// Rules (from ROADMAP.md):
/// - Preserve casing from metadata
/// - Replace spaces with `.`
/// - Remove: apostrophes, colons, slashes (`/` and `\`), non-ASCII characters
/// - Keep numeric characters
pub fn sanitize_game_name(name: &str) -> String {
    name.chars()
        .filter_map(|c| {
            if c == ' ' {
                Some('.')
            } else if c == '\'' || c == ':' || c == '/' || c == '\\' {
                None
            } else if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                Some(c)
            } else if !c.is_ascii() {
                None
            } else {
                Some(c)
            }
        })
        .collect()
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
