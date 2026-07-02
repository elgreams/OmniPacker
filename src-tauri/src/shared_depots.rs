//! Single source of truth for Steam's shared/redistributable depots.
//!
//! Three concerns used to live in three different files and could silently
//! disagree: which depots are shared (naming, steam_api), which app owns them
//! (.acf SharedDepots, acf_generator), and which folder they install into
//! (finalization layout, job_finalization). They are unified here so a depot
//! can never be *named* like a shared depot but *laid out* like a regular one.
//!
//! Note: `steamcmd_api::fetch_shared_depots` also detects shared depots
//! data-drivenly via appinfo `sharedinstall`. That path is used for depot
//! *naming* only, with this module as its offline fallback; folder layout and
//! .acf sections key off this table alone.

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

/// Non-redist shared depots: (depot_id, owner_appid).
///
/// The Steam Linux Runtime depots are shipped by their own apps; note that the
/// Soldier runtime's depot (1628210) is owned by a *different* appid (1628350).
const OTHER_SHARED_DEPOTS: &[(&str, &str)] = &[
    // Steamworks Common Redistributables app depot itself
    ("228980", "228980"),
    // Steam Linux Runtime (base)
    ("1391110", "1391110"),
    // Steam Linux Runtime - Soldier (owned by app 1628350)
    ("1628210", "1628350"),
    // Steam Linux Runtime - Sniper
    ("1826330", "1826330"),
];

/// Owner appid → the installdir folder name under `steamapps/common/` that
/// Steam uses for that shared app. Every owner reachable from the tables above
/// MUST have an entry here (enforced by test).
const OWNER_INSTALL_DIRS: &[(&str, &str)] = &[
    ("228980", "Steamworks Shared"),
    ("1391110", "SteamLinuxRuntime"),
    ("1628350", "SteamLinuxRuntime_soldier"),
    ("1826330", "SteamLinuxRuntime_sniper"),
];

/// Checks if a depot ID is a known shared Steam depot (redistributables, runtimes, etc.)
pub fn is_shared_depot(depot_id: &str) -> bool {
    REDIST_DEPOTS.contains(&depot_id)
        || OTHER_SHARED_DEPOTS.iter().any(|(id, _)| *id == depot_id)
}

/// Returns the owner appid for a shared depot
///
/// In Steam's .acf format, shared depots are listed in a `SharedDepots` section
/// with the format: `"depot_id" "owner_appid"`
pub fn get_shared_depot_owner(depot_id: &str) -> &'static str {
    // All redistributables are owned by the Steamworks Common Redistributables app
    if REDIST_DEPOTS.contains(&depot_id) {
        return "228980";
    }
    OTHER_SHARED_DEPOTS
        .iter()
        .find(|(id, _)| *id == depot_id)
        .map(|(_, owner)| *owner)
        // Default to Steamworks Common Redistributables app
        .unwrap_or("228980")
}

/// Returns the `steamapps/common/` folder name a shared depot installs into,
/// based on its owner app (e.g. all 228980-owned depots → "Steamworks Shared",
/// matching real Steam).
pub fn get_shared_depot_install_dir(depot_id: &str) -> &'static str {
    let owner = get_shared_depot_owner(depot_id);
    OWNER_INSTALL_DIRS
        .iter()
        .find(|(o, _)| *o == owner)
        .map(|(_, dir)| *dir)
        .unwrap_or("Steamworks Shared")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shared depot's owner must have an install-dir entry; this is the
    /// cross-file disagreement BE-6 flagged (naming said "shared", layout
    /// didn't know the owner and silently fell back).
    #[test]
    fn every_shared_depot_owner_has_an_install_dir() {
        let all_ids = REDIST_DEPOTS
            .iter()
            .copied()
            .chain(OTHER_SHARED_DEPOTS.iter().map(|(id, _)| *id));
        for depot_id in all_ids {
            assert!(is_shared_depot(depot_id), "{depot_id} not shared?");
            let owner = get_shared_depot_owner(depot_id);
            assert!(
                OWNER_INSTALL_DIRS.iter().any(|(o, _)| *o == owner),
                "owner {owner} of depot {depot_id} has no install-dir mapping"
            );
        }
    }

    #[test]
    fn redists_are_owned_by_steamworks_shared() {
        for depot_id in REDIST_DEPOTS {
            assert_eq!(get_shared_depot_owner(depot_id), "228980");
            assert_eq!(get_shared_depot_install_dir(depot_id), "Steamworks Shared");
        }
    }

    #[test]
    fn soldier_runtime_depot_maps_through_its_owner_app() {
        // Depot 1628210 belongs to app 1628350 (Soldier), not itself.
        assert!(is_shared_depot("1628210"));
        assert_eq!(get_shared_depot_owner("1628210"), "1628350");
        assert_eq!(
            get_shared_depot_install_dir("1628210"),
            "SteamLinuxRuntime_soldier"
        );
    }

    #[test]
    fn linux_runtimes_map_to_their_own_dirs() {
        assert_eq!(get_shared_depot_install_dir("1391110"), "SteamLinuxRuntime");
        assert_eq!(
            get_shared_depot_install_dir("1826330"),
            "SteamLinuxRuntime_sniper"
        );
    }

    #[test]
    fn regular_depots_are_not_shared() {
        assert!(!is_shared_depot("1000"));
        assert!(!is_shared_depot("1465471"));
    }
}
