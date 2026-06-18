//! ACF (Steam App Manifest) file generator
//!
//! Generates `appmanifest_<appid>.acf` files that tell Steam/GreenLuma about
//! installed games. These files enable drop-in compatibility with Steam's
//! library detection.
//!
//! PRIVACY: The `LastOwner` field is ALWAYS set to "0" to prevent deanonymization.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::job_metadata::JobMetadataFile;
use crate::steam_api::{get_shared_depot_owner, is_shared_depot};

/// VDF (Valve Data Format) builder for generating properly formatted .acf files
struct VdfBuilder {
    content: String,
    indent_level: usize,
}

impl VdfBuilder {
    fn new() -> Self {
        Self {
            content: String::new(),
            indent_level: 0,
        }
    }

    /// Adds indentation tabs for the current level
    fn indent(&mut self) {
        for _ in 0..self.indent_level {
            self.content.push('\t');
        }
    }

    /// Writes a key-value pair: "key"		"value"
    fn key_value(&mut self, key: &str, value: &str) {
        self.indent();
        self.content.push_str(&format!("\"{}\"\t\t\"{}\"\n", key, value));
    }

    /// Opens a new section: "name"\n{\n
    fn open_section(&mut self, name: &str) {
        self.indent();
        self.content.push_str(&format!("\"{}\"\n", name));
        self.indent();
        self.content.push_str("{\n");
        self.indent_level += 1;
    }

    /// Closes the current section: }\n
    fn close_section(&mut self) {
        self.indent_level = self.indent_level.saturating_sub(1);
        self.indent();
        self.content.push_str("}\n");
    }

    /// Returns the built VDF content
    fn build(self) -> String {
        self.content
    }
}

/// Calculates the total size of all files in a directory recursively
pub(crate) fn calculate_size_on_disk(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    if path.is_file() {
        return fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    }

    let mut total_size = 0u64;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                total_size += calculate_size_on_disk(&entry_path);
            } else if entry_path.is_file() {
                total_size += fs::metadata(&entry_path).map(|m| m.len()).unwrap_or(0);
            }
        }
    }

    total_size
}

/// Generates the content for an appmanifest .acf file
///
/// # Arguments
/// * `metadata` - Job metadata containing app/depot/build information
/// * `common_dir` - Path to the steamapps/common directory (for size calculation)
/// * `install_dir_name` - Name of the installation directory (relative path component)
/// * `manifest_map` - Map of depot_id → actual manifest_id (extracted from .manifest filenames)
/// * `depot_sizes` - Pre-computed map of depot_id → size in bytes (computed from staging
///   before the merge, since after merge all non-shared depots share one folder)
///
/// # Returns
/// The complete .acf file content as a string
fn generate_acf_content(
    metadata: &JobMetadataFile,
    common_dir: &Path,
    install_dir_name: &str,
    manifest_map: &HashMap<String, String>,
    depot_sizes: &HashMap<String, u64>,
) -> String {
    let mut vdf = VdfBuilder::new();

    // Get Unix timestamp from build datetime, or use current time as fallback
    let last_updated = metadata
        .build_datetime_utc
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    // SizeOnDisk = only the game's installdir, not shared depot folders
    let size_on_disk = calculate_size_on_disk(&common_dir.join(install_dir_name));

    // Separate shared depots from regular depots
    let (shared_depots, regular_depots): (Vec<_>, Vec<_>) = metadata
        .depots
        .iter()
        .partition(|d| is_shared_depot(&d.depot_id));

    vdf.open_section("AppState");

    // Core app information
    vdf.key_value("appid", &metadata.appid);
    vdf.key_value("universe", "1");
    vdf.key_value("name", &metadata.game_name);
    vdf.key_value("StateFlags", "4");
    vdf.key_value("installdir", install_dir_name);
    vdf.key_value("LastUpdated", &last_updated.to_string());
    vdf.key_value("LastPlayed", "0");
    vdf.key_value("SizeOnDisk", &size_on_disk.to_string());
    vdf.key_value("StagingSize", "0");
    vdf.key_value("buildid", &metadata.build_id);

    // PRIVACY: LastOwner is ALWAYS "0" to prevent deanonymization
    vdf.key_value("LastOwner", "0");

    vdf.key_value("UpdateResult", "0");
    vdf.key_value("BytesToDownload", "0");
    vdf.key_value("BytesDownloaded", "0");

    vdf.key_value("AutoUpdateBehavior", "0");
    vdf.key_value("AllowOtherDownloadsWhileRunning", "0");
    vdf.key_value("ScheduledAutoUpdate", "0");

    // InstalledDepots section - only regular (non-shared) depots
    vdf.open_section("InstalledDepots");
    for depot in &regular_depots {
        let manifest = manifest_map
            .get(&depot.depot_id)
            .or(depot.manifest_id_used.as_ref())
            .unwrap_or(&depot.manifest_id);

        let depot_size = depot_sizes.get(&depot.depot_id).copied().unwrap_or(0);

        vdf.open_section(&depot.depot_id);
        vdf.key_value("manifest", manifest);
        vdf.key_value("size", &depot_size.to_string());
        vdf.close_section();
    }
    vdf.close_section();

    // SharedDepots section - shared depots with their owner appids
    if !shared_depots.is_empty() {
        vdf.open_section("SharedDepots");
        for depot in &shared_depots {
            let owner_appid = get_shared_depot_owner(&depot.depot_id);
            vdf.key_value(&depot.depot_id, owner_appid);
        }
        vdf.close_section();
    }

    // UserConfig — real Steam populates language here; we leave it empty like SSP
    vdf.open_section("UserConfig");
    vdf.close_section();

    // MountedConfig — present in real Steam .acf files (not MountedDepots)
    vdf.open_section("MountedConfig");
    vdf.close_section();

    vdf.close_section(); // Close AppState

    vdf.build()
}

/// Writes the appmanifest .acf file to the steamapps directory
///
/// # Arguments
/// * `steamapps_dir` - Path to the steamapps directory
/// * `metadata` - Job metadata containing app/depot/build information
/// * `common_dir` - Path to the steamapps/common directory (for size calculation)
/// * `install_dir_name` - Name of the installation directory (the game folder name)
/// * `manifest_map` - Map of depot_id → actual manifest_id (extracted from .manifest filenames)
/// * `depot_sizes` - Pre-computed map of depot_id → size in bytes (must be computed from
///   staging before the merge, since after merge per-depot sizes are unrecoverable)
pub fn write_acf_file(
    steamapps_dir: &Path,
    metadata: &JobMetadataFile,
    common_dir: &Path,
    install_dir_name: &str,
    manifest_map: &HashMap<String, String>,
    depot_sizes: &HashMap<String, u64>,
) -> Result<(), String> {
    let acf_content = generate_acf_content(metadata, common_dir, install_dir_name, manifest_map, depot_sizes);

    // Write to appmanifest_<appid>.acf
    let acf_filename = format!("appmanifest_{}.acf", metadata.appid);
    let acf_path = steamapps_dir.join(&acf_filename);

    fs::write(&acf_path, acf_content)
        .map_err(|e| format!("Failed to write {}: {}", acf_filename, e))?;

    Ok(())
}

/// Generates the `appmanifest_228980.acf` file for Steamworks Common Redistributables.
///
/// Steam expects a separate manifest for the shared redistributable app (228980) whenever
/// shared depots (DirectX, VCRedist, etc.) are present. Without it, Steam may attempt to
/// re-download the redistributables or fail to run install scripts on first launch.
///
/// The `InstallScripts` section is populated by scanning the redistributable directories
/// for `installscript.vdf` files, using Windows-style paths as Steam expects.
pub fn write_shared_depots_acf(
    steamapps_dir: &Path,
    metadata: &JobMetadataFile,
    common_dir: &Path,
    manifest_map: &HashMap<String, String>,
) -> Result<(), String> {
    // Collect only the shared depots present in this job
    let shared_depots: Vec<_> = metadata
        .depots
        .iter()
        .filter(|d| is_shared_depot(&d.depot_id))
        // Only include depots owned by 228980 (redistributables) — not Linux Runtime etc.
        .filter(|d| get_shared_depot_owner(&d.depot_id) == "228980")
        .collect();

    if shared_depots.is_empty() {
        return Ok(());
    }

    let size_on_disk = shared_depots
        .iter()
        .map(|d| calculate_size_on_disk(&common_dir.join(&d.depot_name)))
        .sum::<u64>();

    let last_updated = metadata
        .build_datetime_utc
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());

    let mut vdf = VdfBuilder::new();
    vdf.open_section("AppState");

    vdf.key_value("appid", "228980");
    vdf.key_value("universe", "1");
    vdf.key_value("LauncherPath", "0");
    vdf.key_value("name", "Steamworks Common Redistributables");
    vdf.key_value("StateFlags", "4");
    vdf.key_value("installdir", "Steamworks Shared");
    vdf.key_value("LastUpdated", &last_updated.to_string());
    vdf.key_value("LastPlayed", "0");
    vdf.key_value("SizeOnDisk", &size_on_disk.to_string());
    vdf.key_value("StagingSize", "0");
    // buildid is not meaningful for redistributables; use 0 as a neutral value
    vdf.key_value("buildid", "0");
    vdf.key_value("LastOwner", "0");
    vdf.key_value("DownloadType", "0");
    vdf.key_value("AutoUpdateBehavior", "0");
    vdf.key_value("AllowOtherDownloadsWhileRunning", "0");
    vdf.key_value("ScheduledAutoUpdate", "0");

    vdf.open_section("InstalledDepots");
    for depot in &shared_depots {
        let manifest = manifest_map
            .get(&depot.depot_id)
            .or(depot.manifest_id_used.as_ref())
            .unwrap_or(&depot.manifest_id);
        let size = calculate_size_on_disk(&common_dir.join(&depot.depot_name));

        vdf.open_section(&depot.depot_id);
        vdf.key_value("manifest", manifest);
        vdf.key_value("size", &size.to_string());
        vdf.close_section();
    }
    vdf.close_section();

    // Scan for installscript.vdf files in the redistributable directories
    // Steam uses these to run installers (VCRedist, DirectX) on first launch.
    // Paths use Windows-style backslashes as Steam expects them.
    let install_scripts = collect_install_scripts(common_dir);
    if !install_scripts.is_empty() {
        vdf.open_section("InstallScripts");
        for (depot_id, script_path) in &install_scripts {
            vdf.key_value(depot_id, script_path);
        }
        vdf.close_section();
    }

    vdf.open_section("UserConfig");
    vdf.close_section();
    vdf.open_section("MountedConfig");
    vdf.close_section();

    vdf.close_section(); // AppState

    let acf_path = steamapps_dir.join("appmanifest_228980.acf");
    fs::write(&acf_path, vdf.build())
        .map_err(|e| format!("Failed to write appmanifest_228980.acf: {}", e))?;

    Ok(())
}

/// Scans common/ subdirectories for installscript.vdf files and returns
/// a map of depot_id → Windows-style relative path (for the InstallScripts section).
///
/// Steam's InstallScripts section uses Windows backslash paths even on non-Windows.
/// The path is relative to the steamapps/common/<depot_name>/ directory.
fn collect_install_scripts(common_dir: &Path) -> Vec<(String, String)> {
    // Known install script locations: depot_id → (folder_name, relative_path)
    let known_scripts: &[(&str, &str, &str)] = &[
        ("228984", "Steamworks Shared", r"_CommonRedist\vcredist\2012\installscript.vdf"),
        ("228990", "Steamworks Shared", r"_CommonRedist\DirectX\Jun2010\installscript.vdf"),
        ("228983", "Steamworks Shared", r"_CommonRedist\DirectX\Jun2010\installscript.vdf"),
    ];

    let mut result = Vec::new();
    for (depot_id, folder, rel_path) in known_scripts {
        // Convert backslash path to forward slash for filesystem check
        let fs_rel = rel_path.replace('\\', "/");
        if common_dir.join(folder).join(&fs_rel).exists() {
            result.push((depot_id.to_string(), rel_path.to_string()));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_metadata::{BuildIdSource, DepotInfo};
    use chrono::TimeZone;

    fn create_test_metadata() -> JobMetadataFile {
        JobMetadataFile::new(
            "test-job-id".to_string(),
            "47410".to_string(),
            "public".to_string(),
            "Linux64".to_string(),
            "47411".to_string(),
            "Test Game".to_string(),
            "3354190".to_string(),
            BuildIdSource::AppBuildid,
            Some(chrono::Utc.with_ymd_and_hms(2024, 1, 15, 10, 30, 45).unwrap()),
            vec![DepotInfo {
                depot_id: "47411".to_string(),
                depot_name: "Test Game Content".to_string(),
                manifest_id: "6777399203159127119".to_string(),
                manifest_id_used: None,
            }],
        )
    }

    #[test]
    fn test_vdf_builder_key_value() {
        let mut vdf = VdfBuilder::new();
        vdf.key_value("appid", "12345");
        assert_eq!(vdf.build(), "\"appid\"\t\t\"12345\"\n");
    }

    #[test]
    fn test_vdf_builder_section() {
        let mut vdf = VdfBuilder::new();
        vdf.open_section("AppState");
        vdf.key_value("appid", "12345");
        vdf.close_section();

        let result = vdf.build();
        assert!(result.contains("\"AppState\"\n"));
        assert!(result.contains("{\n"));
        assert!(result.contains("\t\"appid\"\t\t\"12345\"\n"));
        assert!(result.contains("}\n"));
    }

    fn empty_depot_sizes() -> HashMap<String, u64> {
        HashMap::new()
    }

    #[test]
    fn test_generate_acf_content_has_required_fields() {
        let metadata = create_test_metadata();
        let manifest_map = HashMap::new();
        let depot_sizes = empty_depot_sizes();
        let content = generate_acf_content(&metadata, Path::new("/tmp/test"), "Test Game", &manifest_map, &depot_sizes);

        assert!(content.contains("\"appid\"\t\t\"47410\""));
        assert!(content.contains("\"universe\"\t\t\"1\""));
        assert!(content.contains("\"name\"\t\t\"Test Game\""));
        assert!(content.contains("\"StateFlags\"\t\t\"4\""));
        assert!(content.contains("\"buildid\"\t\t\"3354190\""));
        assert!(content.contains("\"LastOwner\"\t\t\"0\""));

        // Verify correct sections present (MountedConfig, not MountedDepots)
        assert!(content.contains("\"UserConfig\""));
        assert!(content.contains("\"MountedConfig\""));
        assert!(!content.contains("\"MountedDepots\""));
    }

    #[test]
    fn test_generate_acf_content_has_depots() {
        let metadata = create_test_metadata();
        let manifest_map = HashMap::new();
        let depot_sizes = empty_depot_sizes();
        let content = generate_acf_content(&metadata, Path::new("/tmp/test"), "Test Game", &manifest_map, &depot_sizes);

        assert!(content.contains("\"InstalledDepots\""));
        assert!(content.contains("\"47411\""));
        assert!(content.contains("\"manifest\"\t\t\"6777399203159127119\""));
    }

    #[test]
    fn test_last_owner_always_zero() {
        let metadata = create_test_metadata();
        let manifest_map = HashMap::new();
        let depot_sizes = empty_depot_sizes();
        let content = generate_acf_content(&metadata, Path::new("/tmp/test"), "Test Game", &manifest_map, &depot_sizes);

        assert!(
            content.contains("\"LastOwner\"\t\t\"0\""),
            "PRIVACY VIOLATION: LastOwner must be \"0\" to prevent deanonymization"
        );
    }

    #[test]
    fn test_manifest_map_overrides_metadata() {
        let metadata = create_test_metadata();
        let mut manifest_map = HashMap::new();
        manifest_map.insert("47411".to_string(), "1234567890123456789".to_string());
        let depot_sizes = empty_depot_sizes();

        let content = generate_acf_content(&metadata, Path::new("/tmp/test"), "Test Game", &manifest_map, &depot_sizes);

        assert!(content.contains("\"manifest\"\t\t\"1234567890123456789\""));
        assert!(!content.contains("\"manifest\"\t\t\"6777399203159127119\""));
    }
}
