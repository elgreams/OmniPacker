use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use regex::Regex;

use crate::depot_runner::JobMetadata;

/// Information about a depot extracted from DepotDownloader output
#[derive(Debug, Clone)]
pub struct PreflightDepotInfo {
    pub depot_id: String,
    #[allow(dead_code)] // Captured but not currently used; kept for debugging/future use
    pub manifest_id: String,
    pub depot_name: Option<String>,
}

/// Result of the manifest-only preflight operation
#[derive(Debug)]
pub struct PreflightResult {
    /// List of depots discovered
    #[allow(dead_code)] // Captured but not currently used; kept for debugging/future use
    pub depots: Vec<PreflightDepotInfo>,
    /// Primary depot ID (from installdir detection)
    #[allow(dead_code)] // Captured but not currently used; kept for debugging/future use
    pub primary_depot_id: Option<String>,
    /// Build ID if found in output
    #[allow(dead_code)] // Captured but not currently used; kept for debugging/future use
    pub build_id: Option<String>,
    /// Build release datetime if found in output
    pub build_datetime_utc: Option<DateTime<Utc>>,
    /// Raw output lines for debugging
    #[allow(dead_code)] // Captured but not currently used; kept for debugging/future use
    pub raw_output: Vec<String>,
}

/// Builds command-line arguments for preflight (similar to regular run but without download-specific options)
pub fn build_preflight_args(job: &JobMetadata) -> Result<Vec<String>, String> {
    let mut args = Vec::new();

    if !job.app_id.is_empty() && job.app_id != "unknown" {
        args.push("-app".to_string());
        args.push(job.app_id.clone());
    }

    if !job.branch.is_empty() {
        args.push("-branch".to_string());
        args.push(job.branch.clone());
    }

    // OS/arch for depot selection (though we don't filter, DD might need it)
    let (os, arch) = map_os_selection(&job.os);
    args.push("-os".to_string());
    args.push(os.to_string());
    args.push("-osarch".to_string());
    args.push(arch.to_string());

    // Authentication
    if job.qr_enabled {
        args.push("-qr".to_string());
    } else if !job.username.is_empty() {
        args.push("-username".to_string());
        args.push(job.username.clone());

        if !job.password.is_empty() {
            args.push("-password".to_string());
            args.push(job.password.clone());
        }
        args.push("-remember-password".to_string());
    }
    // If both username and password are empty, attempt anonymous download (no auth args)

    Ok(args)
}

/// Maps OS selection string to DepotDownloader -os and -osarch arguments
fn map_os_selection(os: &str) -> (&'static str, &'static str) {
    match os {
        "Windows x64" => ("windows", "64"),
        "Windows x86" => ("windows", "32"),
        "Linux" => ("linux", "64"),
        "macOS x64" => ("macos", "64"),
        "macOS arm64" => ("macos", "arm64"),
        "macOS" => ("macos", "64"),
        _ => ("windows", "64"),
    }
}

/// Parses DepotDownloader output to extract depot and manifest information
pub fn parse_preflight_output(lines: &[String]) -> PreflightResult {
    let mut depots: Vec<PreflightDepotInfo> = Vec::new();
    let mut primary_depot_id: Option<String> = None;
    let mut build_id: Option<String> = None;
    let mut build_datetime_utc: Option<DateTime<Utc>> = None;

    // Patterns to match depot/manifest info from DD output
    // Example patterns:
    // "Depot <depot_id> - Manifest <manifest_id>"
    // "Got depot key for <depot_id>"
    // "Downloading depot <depot_id>"

    // Pattern: "Depot 123456 - Manifest 9876543210987654321"
    let depot_manifest_pattern =
        Regex::new(r"[Dd]epot\s+(\d+)\s*[-–]\s*[Mm]anifest\s+(\d+)").unwrap();

    // Pattern: Depot ID with quoted name: Depot 12345 "Depot Name"
    let depot_name_pattern = Regex::new(r#"[Dd]epot\s+(\d+)\s+"([^"]+)""#).unwrap();

    // Pattern: name field in appinfo dump: "name"    "Depot Name"
    let appinfo_name_pattern = Regex::new(r#""name"\s+"([^"]+)""#).unwrap();

    // Pattern: "Got manifest request code for..." or similar
    let manifest_pattern = Regex::new(r"[Mm]anifest\s+(\d+)").unwrap();
    let depot_pattern = Regex::new(r"[Dd]epot\s+(\d+)").unwrap();

    // Pattern for installdir (primary depot detection)
    // Example: "installdir = Common\GameName"
    let installdir_pattern = Regex::new(r#"installdir\s*[=:]\s*"?([^"\n]+)"?"#).unwrap();

    // Pattern for build ID
    // Example: "buildid = 12345678" or "BuildID: 12345678"
    let buildid_pattern = Regex::new(r"[Bb]uild[Ii][Dd]\s*[=:]\s*(\d+)").unwrap();

    // Pattern for Unix epoch timestamps
    let timeupdated_pattern =
        Regex::new(r"(?i)timeupdated[^0-9]*(\d{9,})").unwrap();
    let lastupdated_pattern =
        Regex::new(r"(?i)last\s*updated[^0-9]*(\d{9,})").unwrap();
    let builddate_pattern =
        Regex::new(r"(?i)build(?:_|\s)*date[^0-9]*(\d{9,})").unwrap();

    // Pattern for ISO datetime format
    let iso_datetime_pattern = Regex::new(
        r"(?i)(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})(?:\s*UTC|Z)?",
    )
    .unwrap();

    // Pattern for manifest creation time from DepotDownloader output WITH depot ID
    // Example: "Manifest 12345678 (1/15/2024 10:30:45 AM)" or "Manifest 12345678 (2024-01-15 10:30:45)"
    let manifest_creationtime_pattern = Regex::new(
        r"(?i)Manifest\s+(\d+)\s+\((.+?)\)",
    )
    .unwrap();

    // Pattern for .NET DateTime formats commonly used by DepotDownloader
    // Matches: "1/15/2024 10:30:45 AM", "12/5/2024 3:45:12 PM", etc.
    let dotnet_datetime_pattern = Regex::new(
        r"(\d{1,2})/(\d{1,2})/(\d{4})\s+(\d{1,2}):(\d{2}):(\d{2})(?:\s*([AP]M))?",
    )
    .unwrap();

    // Track depot IDs we've seen with their manifest IDs (depot_id -> manifest_id)
    let mut depot_manifest_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Track manifest IDs to depot IDs (manifest_id -> depot_id) for reverse lookup
    let mut manifest_depot_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Track depot names (depot_id -> depot_name)
    let mut depot_name_map: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    // Track depot-specific timestamps (depot_id -> timestamp)
    let mut depot_timestamp_map: std::collections::HashMap<String, DateTime<Utc>> =
        std::collections::HashMap::new();

    // Track the last depot mentioned (for installdir detection)
    let mut last_depot_mentioned: Option<String> = None;

    for line in lines {
        // Try to match depot names: Depot 12345 "Depot Name"
        if let Some(caps) = depot_name_pattern.captures(line) {
            let depot_id = caps.get(1).map(|m| m.as_str().to_string()).unwrap();
            let depot_name = caps.get(2).map(|m| m.as_str().to_string()).unwrap();
            eprintln!("[PREFLIGHT] Found depot name: {} -> {}", depot_id, depot_name);
            depot_name_map.insert(depot_id.clone(), depot_name);
            last_depot_mentioned = Some(depot_id);
            continue;
        }

        // Try to match depot-manifest pairs
        if let Some(caps) = depot_manifest_pattern.captures(line) {
            let depot_id = caps.get(1).map(|m| m.as_str().to_string()).unwrap();
            let manifest_id = caps.get(2).map(|m| m.as_str().to_string()).unwrap();
            depot_manifest_map.insert(depot_id.clone(), manifest_id.clone());
            manifest_depot_map.insert(manifest_id, depot_id.clone());
            last_depot_mentioned = Some(depot_id);
            continue;
        }

        // Try to match appinfo name field (when in depot context)
        if let Some(caps) = appinfo_name_pattern.captures(line) {
            if let Some(ref depot_id) = last_depot_mentioned {
                let depot_name = caps.get(1).map(|m| m.as_str().to_string()).unwrap();
                eprintln!("[PREFLIGHT] Found depot name (appinfo): {} -> {}", depot_id, depot_name);
                depot_name_map.insert(depot_id.clone(), depot_name);
            }
        }

        // Also track depot mentions from standalone depot patterns
        if let Some(caps) = depot_pattern.captures(line) {
            last_depot_mentioned = caps.get(1).map(|m| m.as_str().to_string());
        }

        // Check for build ID
        if let Some(caps) = buildid_pattern.captures(line) {
            if build_id.is_none() {
                build_id = caps.get(1).map(|m| m.as_str().to_string());
            }
        }

        // FIRST: Capture build release timestamp (timeupdated) - PRIMARY SOURCE
        // This is the actual build/patch release date shown in SteamDB's Patches section
        if build_datetime_utc.is_none() {
            // Try Unix epoch timestamp patterns first (most reliable for build dates)
            if let Some(caps) = timeupdated_pattern.captures(line) {
                if let Some(epoch_str) = caps.get(1).map(|m| m.as_str()) {
                    if let Some(parsed) = parse_epoch_timestamp(Some(epoch_str)) {
                        build_datetime_utc = Some(parsed);
                    }
                }
            } else if let Some(caps) = lastupdated_pattern.captures(line) {
                if let Some(parsed) = parse_epoch_timestamp(caps.get(1).map(|m| m.as_str())) {
                    build_datetime_utc = Some(parsed);
                }
            } else if let Some(caps) = builddate_pattern.captures(line) {
                if let Some(parsed) = parse_epoch_timestamp(caps.get(1).map(|m| m.as_str())) {
                    build_datetime_utc = Some(parsed);
                }
            }
        }

        // SECOND: Capture per-depot timestamps from manifest creation time (for fallback only)
        // Example: "Manifest 7206221393165260579 (1/15/2024 10:30:45 AM)"
        // Note: These are stored for per-depot tracking but NOT used for build_datetime_utc
        // if timeupdated was already found above
        if let Some(caps) = manifest_creationtime_pattern.captures(line) {
            if let Some(manifest_id) = caps.get(1).map(|m| m.as_str().to_string()) {
                if let Some(datetime_str) = caps.get(2).map(|m| m.as_str()) {
                    // Try to parse as .NET DateTime format first
                    let parsed = if let Some(p) = parse_dotnet_datetime(datetime_str, &dotnet_datetime_pattern) {
                        Some(p)
                    }
                    // Fall back to ISO format
                    else if let Some(iso_caps) = iso_datetime_pattern.captures(datetime_str) {
                        parse_iso_datetime(
                            iso_caps.get(1).map(|m| m.as_str()),
                            iso_caps.get(2).map(|m| m.as_str()),
                        )
                    } else {
                        None
                    };

                    // Map manifest ID back to depot ID and store timestamp
                    if let Some(timestamp) = parsed {
                        if let Some(depot_id) = manifest_depot_map.get(&manifest_id) {
                            depot_timestamp_map.insert(depot_id.clone(), timestamp);
                        }
                    }
                }
            }
        }

        // Check for installdir (primary depot detection)
        // The installdir field appears in the config for the primary/main game depot
        if installdir_pattern.is_match(line) && primary_depot_id.is_none() {
            if let Some(ref depot_id) = last_depot_mentioned {
                primary_depot_id = Some(depot_id.clone());
            }
        }
    }

    // If we didn't find explicit depot-manifest pairs, try to find them separately
    if depot_manifest_map.is_empty() {
        let mut last_depot: Option<String> = None;

        for line in lines {
            if let Some(caps) = depot_pattern.captures(line) {
                last_depot = caps.get(1).map(|m| m.as_str().to_string());
            }

            if let Some(caps) = manifest_pattern.captures(line) {
                if let Some(ref depot) = last_depot {
                    let manifest_id = caps.get(1).map(|m| m.as_str().to_string()).unwrap();
                    depot_manifest_map.insert(depot.clone(), manifest_id);
                }
            }
        }
    }

    // Convert to Vec with depot names if available
    for (depot_id, manifest_id) in depot_manifest_map {
        let depot_name = depot_name_map.get(&depot_id).cloned();
        depots.push(PreflightDepotInfo {
            depot_id,
            manifest_id,
            depot_name,
        });
    }

    // Sort by depot ID for consistency
    depots.sort_by(|a, b| {
        a.depot_id
            .parse::<u64>()
            .unwrap_or(0)
            .cmp(&b.depot_id.parse::<u64>().unwrap_or(0))
    });

    // Fallback: if no primary depot was detected via installdir, use first non-shared depot
    // Priority: 1) installdir detection, 2) first non-shared depot, 3) first depot
    if !depots.is_empty() && primary_depot_id.is_none() {
        use crate::steam_api::is_shared_depot;
        // Find first non-shared depot as fallback
        primary_depot_id = depots
            .iter()
            .find(|d| !is_shared_depot(&d.depot_id))
            .map(|d| d.depot_id.clone())
            .or_else(|| Some(depots[0].depot_id.clone()));
    }

    // FALLBACK: If timeupdated wasn't found, use primary depot's manifest timestamp
    // Only do this if build_datetime_utc is still None (meaning timeupdated wasn't captured)
    if build_datetime_utc.is_none() {
        if let Some(ref primary_id) = primary_depot_id {
            if let Some(primary_timestamp) = depot_timestamp_map.get(primary_id) {
                build_datetime_utc = Some(*primary_timestamp);
            }
        }
    }

    PreflightResult {
        depots,
        primary_depot_id,
        build_id,
        build_datetime_utc,
        raw_output: lines.to_vec(),
    }
}

fn parse_epoch_timestamp(value: Option<&str>) -> Option<DateTime<Utc>> {
    let seconds: i64 = value?.trim().parse().ok()?;
    Utc.timestamp_opt(seconds, 0).single()
}

fn parse_iso_datetime(date: Option<&str>, time: Option<&str>) -> Option<DateTime<Utc>> {
    let date = date?;
    let time = time?;
    let combined = format!("{} {}", date.trim(), time.trim());
    let parsed = NaiveDateTime::parse_from_str(&combined, "%Y-%m-%d %H:%M:%S").ok()?;
    Some(DateTime::from_naive_utc_and_offset(parsed, Utc))
}

fn parse_dotnet_datetime(text: &str, pattern: &Regex) -> Option<DateTime<Utc>> {
    let caps = pattern.captures(text)?;

    let month: u32 = caps.get(1)?.as_str().parse().ok()?;
    let day: u32 = caps.get(2)?.as_str().parse().ok()?;
    let year: i32 = caps.get(3)?.as_str().parse().ok()?;
    let mut hour: u32 = caps.get(4)?.as_str().parse().ok()?;
    let minute: u32 = caps.get(5)?.as_str().parse().ok()?;
    let second: u32 = caps.get(6)?.as_str().parse().ok()?;

    // Handle AM/PM if present
    if let Some(ampm) = caps.get(7) {
        let ampm_str = ampm.as_str();
        if ampm_str.eq_ignore_ascii_case("PM") && hour < 12 {
            hour += 12;
        } else if ampm_str.eq_ignore_ascii_case("AM") && hour == 12 {
            hour = 0;
        }
    }

    // Create NaiveDateTime
    let naive = NaiveDateTime::new(
        chrono::NaiveDate::from_ymd_opt(year, month, day)?,
        chrono::NaiveTime::from_hms_opt(hour, minute, second)?,
    );

    Some(DateTime::from_naive_utc_and_offset(naive, Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn test_parse_depot_manifest_line() {
        let lines = vec![
            "Depot 123456 - Manifest 9876543210987654321".to_string(),
            "Depot 123457 - Manifest 1234567890123456789".to_string(),
        ];

        let result = parse_preflight_output(&lines);
        assert_eq!(result.depots.len(), 2);
        assert_eq!(result.depots[0].depot_id, "123456");
        assert_eq!(result.depots[0].manifest_id, "9876543210987654321");
    }

    #[test]
    fn test_parse_buildid() {
        let lines = vec![
            "Some info line".to_string(),
            "buildid = 18674832".to_string(),
            "More info".to_string(),
        ];

        let result = parse_preflight_output(&lines);
        assert_eq!(result.build_id, Some("18674832".to_string()));
    }

    #[test]
    fn test_primary_depot_detection() {
        let lines = vec![
            "Depot 123456 - Manifest 111".to_string(),
            "Depot 123457 - Manifest 222".to_string(),
        ];

        let result = parse_preflight_output(&lines);
        // Primary should be the first/lowest depot ID
        assert_eq!(result.primary_depot_id, Some("123456".to_string()));
    }

    #[test]
    fn test_primary_depot_timestamp_used() {
        let lines = vec![
            "Depot 123456 - Manifest 111".to_string(),
            "Manifest 222 (1/10/2024 1:00:00 PM)".to_string(),  // Non-primary depot 123457's timestamp
            "Depot 123457 - Manifest 222".to_string(),
            "Manifest 111 (1/15/2024 10:30:45 AM)".to_string(), // Primary depot 123456's timestamp
        ];

        let result = parse_preflight_output(&lines);

        // Primary depot should be 123456 (lowest ID)
        assert_eq!(result.primary_depot_id, Some("123456".to_string()));

        // Should use timestamp from primary depot (123456), not from 123457
        assert!(result.build_datetime_utc.is_some());
        let timestamp = result.build_datetime_utc.unwrap();

        // Verify it's the primary depot's timestamp (1/15/2024 10:30:45 AM)
        assert_eq!(timestamp.month(), 1);
        assert_eq!(timestamp.day(), 15);
        assert_eq!(timestamp.year(), 2024);
        assert_eq!(timestamp.hour(), 10);
        assert_eq!(timestamp.minute(), 30);
        assert_eq!(timestamp.second(), 45);
    }

    #[test]
    fn test_balatro_case_realistic() {
        // Realistic Balatro output with Steamworks Shared (228989) and Balatro (2379781)
        let lines = vec![
            "Depot 228989 - Manifest 7206221393165260579".to_string(),
            "Manifest 7206221393165260579 (7/14/2025 11:02:36 PM)".to_string(),  // Steamworks (shared, non-primary)
            "Depot 2379781 - Manifest 4851806656204679952".to_string(),
            "Manifest 4851806656204679952 (2/24/2025 10:02:36 PM)".to_string(), // Balatro (primary)
        ];

        let result = parse_preflight_output(&lines);

        // Primary depot should be 2379781 (Balatro - first non-shared depot)
        assert_eq!(result.primary_depot_id, Some("2379781".to_string()));

        // Should use timestamp from primary depot (2379781 - Balatro)
        assert!(result.build_datetime_utc.is_some());
        let timestamp = result.build_datetime_utc.unwrap();

        // Verify it's using Balatro timestamp (2/24/2025), NOT Steamworks timestamp
        assert_eq!(timestamp.month(), 2);
        assert_eq!(timestamp.day(), 24);
        assert_eq!(timestamp.year(), 2025);
        assert_eq!(timestamp.hour(), 22); // 10 PM = 22:00
        assert_eq!(timestamp.minute(), 2);
        assert_eq!(timestamp.second(), 36);
    }

    #[test]
    fn test_dotnet_datetime_parsing() {
        // Test AM/PM parsing
        let lines = vec![
            "Depot 123456 - Manifest 111".to_string(),
            "Manifest 111 (12/5/2024 3:45:12 PM)".to_string(),
        ];

        let result = parse_preflight_output(&lines);
        assert!(result.build_datetime_utc.is_some());
        let timestamp = result.build_datetime_utc.unwrap();

        assert_eq!(timestamp.month(), 12);
        assert_eq!(timestamp.day(), 5);
        assert_eq!(timestamp.year(), 2024);
        assert_eq!(timestamp.hour(), 15); // 3 PM = 15:00
        assert_eq!(timestamp.minute(), 45);
        assert_eq!(timestamp.second(), 12);
    }

    #[test]
    fn test_installdir_determines_primary_depot() {
        // Test that installdir detection overrides "first non-shared" logic
        let lines = vec![
            "Depot 123456 - Manifest 111".to_string(),
            "Depot 123457 - Manifest 222".to_string(),
            "installdir = Common\\GameName".to_string(), // This makes 123457 the primary depot
            "Manifest 111 (1/10/2024 1:00:00 PM)".to_string(),
            "Manifest 222 (1/15/2024 10:30:45 AM)".to_string(),
        ];

        let result = parse_preflight_output(&lines);

        // Primary depot should be 123457 because installdir appeared after its mention
        assert_eq!(result.primary_depot_id, Some("123457".to_string()));

        // Should use timestamp from depot 123457
        assert!(result.build_datetime_utc.is_some());
        let timestamp = result.build_datetime_utc.unwrap();

        assert_eq!(timestamp.month(), 1);
        assert_eq!(timestamp.day(), 15);
        assert_eq!(timestamp.year(), 2024);
    }
}
