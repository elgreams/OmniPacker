use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::acf_generator;
use crate::job_metadata::JobMetadataFile;
use crate::job_staging::resolve_staging_dir;
use crate::output_conflict::{request_output_conflict_resolution, OutputConflictChoice};
use crate::output_dir::resolve_downloads_dir;
use crate::steam_api::sanitize_game_name;

/// Finalizes a job by moving staging output to final output directory
///
/// This is the main entry point called after DepotDownloader exits successfully.
///
/// # Arguments
/// * `app_handle` - Tauri application handle
/// * `job_id` - Unique job identifier
/// * `compression_enabled` - Whether compression runs after finalization
///
/// # Returns
/// * `Ok(PathBuf)` - Path to the final output directory
/// * `Err(String)` - Human-readable error message
///
/// # Guarantees
/// - Atomic-ish finalization (no partial outputs)
/// - Staging cleanup on success or failure
/// - Temp cleanup on error
/// - Prompts if output already exists (overwrite/copy/cancel)
pub fn finalize_job(
    app_handle: &AppHandle,
    job_id: &str,
    compression_enabled: bool,
) -> Result<PathBuf, String> {
    // Step 1: Load job.json from staging
    let staging_dir = resolve_staging_dir(app_handle, job_id)?;
    let job_metadata = load_and_validate_metadata(&staging_dir)?;

    // Step 2: Validate staging contents
    validate_staging_contents(&staging_dir)?;

    // Step 3: Compute final output path
    let mut final_output_path = compute_final_output_path(app_handle, &job_metadata)?;

    // Step 4: Resolve output conflicts (overwrite/copy/cancel)
    let mut overwrite_existing = false;
    let mut archive_path = if compression_enabled {
        Some(resolve_archive_path(&final_output_path))
    } else {
        None
    };
    let output_exists = final_output_path.exists();
    let archive_exists = archive_path
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false);

    if output_exists || archive_exists {
        let conflict_path = if output_exists {
            final_output_path.clone()
        } else {
            archive_path
                .clone()
                .unwrap_or_else(|| final_output_path.clone())
        };
        match request_output_conflict_resolution(app_handle, job_id, &conflict_path)? {
            OutputConflictChoice::Overwrite => overwrite_existing = true,
            OutputConflictChoice::Copy => {
                final_output_path =
                    resolve_copy_output_path(&final_output_path, compression_enabled)?;
                if compression_enabled {
                    archive_path = Some(resolve_archive_path(&final_output_path));
                }
            }
            OutputConflictChoice::Cancel => {
                return Err(format!(
                    "Output already exists: {}. Job cancelled by user.",
                    conflict_path.display()
                ));
            }
        }
    }

    // Step 5: Build output in temp directory
    let temp_output_path = build_temp_output(app_handle, job_id, &staging_dir, &job_metadata)?;

    // Step 6: Remove existing output if overwrite was selected
    if overwrite_existing {
        remove_existing_output(&final_output_path)?;
        if let Some(path) = archive_path.as_ref() {
            remove_existing_archive(path)?;
        }
    }

    // Step 7: Atomic rename: temp → final
    match atomic_finalize(&temp_output_path, &final_output_path) {
        Ok(()) => Ok(final_output_path),
        Err(e) => {
            // Cleanup temp directory on failure
            let _ = fs::remove_dir_all(&temp_output_path);
            Err(e)
        }
    }
}

/// Step 1: Load and validate job.json
fn load_and_validate_metadata(staging_dir: &Path) -> Result<JobMetadataFile, String> {
    JobMetadataFile::read_from_dir(staging_dir)
        .map_err(|e| format!("Failed to load job.json: {}", e))
}

/// Step 2: Validate staging contents exist
fn validate_staging_contents(staging_dir: &Path) -> Result<(), String> {
    let depots_dir = staging_dir.join("depots");
    if !depots_dir.exists() {
        return Err(format!(
            "Staging directory missing depots/: {}",
            staging_dir.display()
        ));
    }

    // Verify at least one depot directory exists
    let has_depots = fs::read_dir(&depots_dir)
        .map_err(|e| format!("Failed to read depots/: {}", e))?
        .any(|entry| {
            entry
                .ok()
                .map(|e| e.path().is_dir())
                .unwrap_or(false)
        });

    if !has_depots {
        return Err("No depot directories found in depots/".to_string());
    }

    Ok(())
}

/// Step 3: Compute final output directory path
fn compute_final_output_path(
    app_handle: &AppHandle,
    metadata: &JobMetadataFile,
) -> Result<PathBuf, String> {
    let downloads_dir = resolve_downloads_dir(app_handle)?;
    let outputs_dir = downloads_dir.join("outputs");

    // Format: <GameNameSanitized>.Build.<BuildId>.<Platform>.<Branch>
    let sanitized_name = sanitize_game_name(&metadata.game_name);
    let folder_name = format!(
        "{}.Build.{}.{}.{}",
        sanitized_name, metadata.build_id, metadata.platform, metadata.branch
    );

    Ok(outputs_dir.join(folder_name))
}

pub fn resolve_archive_path(output_path: &Path) -> PathBuf {
    let file_name = output_path
        .file_name()
        .unwrap_or_else(|| output_path.as_os_str());
    let mut archive_name = OsString::from(file_name);
    archive_name.push(".7z");

    match output_path.parent() {
        Some(parent) => parent.join(archive_name),
        None => PathBuf::from(archive_name),
    }
}

/// Step 5: Build output in temporary directory
fn build_temp_output(
    app_handle: &AppHandle,
    job_id: &str,
    staging_dir: &Path,
    metadata: &JobMetadataFile,
) -> Result<PathBuf, String> {
    let downloads_dir = resolve_downloads_dir(app_handle)?;
    let outputs_dir = downloads_dir.join("outputs");
    let temp_dir = outputs_dir.join(format!(".tmp_{}", job_id));

    // Clean up temp directory if it exists from a previous failure
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .map_err(|e| format!("Failed to cleanup existing temp directory: {}", e))?;
    }

    // Create temp directory structure
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    // Transform depots/ → steamapps/common/ and collect manifests → depotcache/
    // Returns a map of depot_id → actual manifest_id (extracted from .manifest filenames)
    let manifest_map = transform_depots_to_steamapps(staging_dir, &temp_dir, metadata)?;

    // Generate appmanifest_<appid>.acf file
    let steamapps_dir = temp_dir.join("steamapps");
    let common_dir = steamapps_dir.join("common");
    let install_dir_name = sanitize_game_name(&metadata.game_name);
    acf_generator::write_acf_file(&steamapps_dir, metadata, &common_dir, &install_dir_name, &manifest_map)?;

    Ok(temp_dir)
}

fn resolve_copy_output_path(
    base_path: &Path,
    compression_enabled: bool,
) -> Result<PathBuf, String> {

    let parent = base_path
        .parent()
        .ok_or_else(|| "Output path missing parent directory".to_string())?;
    let base_name = base_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Output directory name is not valid UTF-8".to_string())?;

    for suffix in 1..=9999 {
        let candidate = parent.join(format!("{} ({})", base_name, suffix));
        if candidate.exists() {
            continue;
        }
        if compression_enabled && resolve_archive_path(&candidate).exists() {
            continue;
        }
        return Ok(candidate);
    }

    Err("Unable to find available output copy name".to_string())
}

fn remove_existing_output(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(path)
        .map_err(|e| format!("Failed to inspect existing output: {}", e))?;

    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|e| format!("Failed to remove existing output directory: {}", e))?;
    } else {
        fs::remove_file(path)
            .map_err(|e| format!("Failed to remove existing output file: {}", e))?;
    }

    Ok(())
}

fn remove_existing_archive(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(path)
        .map_err(|e| format!("Failed to inspect existing archive: {}", e))?;

    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .map_err(|e| format!("Failed to remove existing archive directory: {}", e))?;
    } else {
        fs::remove_file(path)
            .map_err(|e| format!("Failed to remove existing archive file: {}", e))?;
    }

    Ok(())
}

/// Transforms DepotDownloader's depots/ structure into Steam-compatible steamapps/common/ structure
///
/// DepotDownloader creates: depots/<depotid>/<manifestid>/(files + .DepotDownloader/)
/// We need to create:
/// - steamapps/common/<DepotName>/(files, excluding .DepotDownloader/)
/// - depotcache/*.manifest (collected from all .DepotDownloader/ directories)
///
/// # Returns
/// A map of depot_id → actual manifest_id (extracted from .manifest filenames)
fn transform_depots_to_steamapps(
    staging_dir: &Path,
    temp_dir: &Path,
    metadata: &JobMetadataFile,
) -> Result<HashMap<String, String>, String> {
    let depots_dir = staging_dir.join("depots");
    let steamapps_common_dir = temp_dir.join("steamapps").join("common");
    let depotcache_dir = temp_dir.join("depotcache");

    // Map of depot_id → actual manifest_id (extracted from .manifest filenames)
    let mut manifest_map: HashMap<String, String> = HashMap::new();

    // Create directories
    fs::create_dir_all(&steamapps_common_dir)
        .map_err(|e| format!("Failed to create steamapps/common/: {}", e))?;
    fs::create_dir_all(&depotcache_dir)
        .map_err(|e| format!("Failed to create depotcache/: {}", e))?;

    // Create a lookup map for depot names from metadata
    let depot_names: HashMap<String, String> = metadata
        .depots
        .iter()
        .map(|d| (d.depot_id.clone(), d.depot_name.clone()))
        .collect();

    // Iterate through each depot directory
    for entry in fs::read_dir(&depots_dir)
        .map_err(|e| format!("Failed to read depots directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read depot entry: {}", e))?;
        let depot_path = entry.path();

        if !depot_path.is_dir() {
            continue;
        }

        let depot_id = entry
            .file_name()
            .to_string_lossy()
            .to_string();

        // Skip .DepotDownloader directory at depot root level
        if depot_id == ".DepotDownloader" {
            continue;
        }

        // Get depot name from metadata, or use fallback
        let depot_name = depot_names
            .get(&depot_id)
            .cloned()
            .unwrap_or_else(|| format!("depot_{}", depot_id));

        // Find the manifest subdirectory (should be only one)
        let manifest_dirs: Vec<_> = fs::read_dir(&depot_path)
            .map_err(|e| format!("Failed to read depot {}: {}", depot_id, e))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        if manifest_dirs.is_empty() {
            return Err(format!("No manifest directory found in depot {}", depot_id));
        }

        // Use the first manifest directory (there should only be one)
        let manifest_dir = manifest_dirs[0].path();

        // Collect manifest files from .DepotDownloader/ subdirectory
        let dd_dir = manifest_dir.join(".DepotDownloader");
        if dd_dir.exists() {
            for manifest_entry in fs::read_dir(&dd_dir)
                .map_err(|e| format!("Failed to read .DepotDownloader directory: {}", e))?
            {
                let manifest_entry = manifest_entry
                    .map_err(|e| format!("Failed to read manifest entry: {}", e))?;
                let manifest_path = manifest_entry.path();

                // Copy .manifest files (not .manifest.sha or staging/)
                if manifest_path.is_file() && manifest_path.extension().map(|e| e == "manifest").unwrap_or(false) {
                    let manifest_filename = manifest_entry.file_name().to_string_lossy().to_string();

                    // Extract manifest ID from filename (format: {manifest_id}.manifest)
                    if let Some(manifest_id) = manifest_filename.strip_suffix(".manifest") {
                        manifest_map.insert(depot_id.clone(), manifest_id.to_string());
                    }

                    fs::copy(&manifest_path, depotcache_dir.join(&manifest_filename))
                        .map_err(|e| format!("Failed to copy manifest file: {}", e))?;
                }
            }
        }

        // Copy manifest directory contents → steamapps/common/<DepotName>/
        // Exclude .DepotDownloader/ directory
        let target_dir = steamapps_common_dir.join(&depot_name);
        copy_dir_recursive_filtered(&manifest_dir, &target_dir, |path| {
            !path.file_name().map(|n| n == ".DepotDownloader").unwrap_or(false)
        })?;
    }

    Ok(manifest_map)
}

/// Step 6: Atomic rename from temp to final
fn atomic_finalize(temp_path: &Path, final_path: &Path) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create outputs directory: {}", e))?;
    }

    // Atomic rename (both paths are under downloads/outputs/, guaranteed same filesystem)
    fs::rename(temp_path, final_path).map_err(|e| {
        format!(
            "Failed to rename temp to final output ({}→{}): {}",
            temp_path.display(),
            final_path.display(),
            e
        )
    })?;

    Ok(())
}

/// Recursively copies a directory and all its contents with filtering
///
/// The filter function receives the source path and returns true if it should be copied
fn copy_dir_recursive_filtered<F>(src: &Path, dst: &Path, filter: F) -> Result<(), String>
where
    F: Fn(&Path) -> bool + Copy,
{
    if !src.is_dir() {
        return Err(format!("Source is not a directory: {}", src.display()));
    }

    fs::create_dir_all(dst)
        .map_err(|e| format!("Failed to create directory {}: {}", dst.display(), e))?;

    for entry in fs::read_dir(src)
        .map_err(|e| format!("Failed to read directory {}: {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let src_path = entry.path();

        // Apply filter - skip if filter returns false
        if !filter(&src_path) {
            continue;
        }

        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive_filtered(&src_path, &dst_path, filter)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to copy file {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}
