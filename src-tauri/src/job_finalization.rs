use std::collections::HashMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

use crate::acf_generator;
use crate::job_metadata::JobMetadataFile;
use crate::job_staging::resolve_staging_dir;
use crate::output_conflict::{request_output_conflict_resolution, OutputConflictChoice};
use crate::output_dir::{resolve_outputs_dir, resolve_scratch_dir};
use crate::steam_api::{sanitize_game_name, sanitize_install_dir};

/// Result of a successful finalization.
pub struct FinalizedOutput {
    /// Path to the final output directory.
    pub path: PathBuf,
    /// Files one game depot overwrote from another during the merge.
    pub collisions: Vec<DepotCollision>,
}

/// A non-shared depot that overwrote files already placed by an earlier depot.
#[derive(Debug, Clone, PartialEq)]
pub struct DepotCollision {
    pub depot_id: String,
    /// Overwritten paths, relative to steamapps/common, forward slashes.
    pub files: Vec<String>,
}

/// (depot_id → manifest_id, depot_id → install-script path, collisions)
type DepotTransform = (
    HashMap<String, String>,
    HashMap<String, String>,
    Vec<DepotCollision>,
);

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
/// * `Ok(FinalizedOutput)` - Final output directory plus any depot collisions
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
) -> Result<FinalizedOutput, String> {
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
    let existing_archive = archive_path.as_deref().and_then(existing_archive_output);

    if output_exists || existing_archive.is_some() {
        let conflict_path = if output_exists {
            final_output_path.clone()
        } else {
            existing_archive
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
    let (temp_output_path, collisions) =
        build_temp_output(app_handle, job_id, &staging_dir, &job_metadata)?;

    // Step 6: Remove existing output if overwrite was selected
    if overwrite_existing {
        remove_existing_output(&final_output_path)?;
        if let Some(path) = archive_path.as_ref() {
            // Removes the single-file archive AND any split volumes
            // (.7z.001, .002, ...) so a re-run never collides with them.
            crate::depot_runner::remove_archive_outputs(path);
            if let Some(left) = existing_archive_output(path) {
                return Err(format!(
                    "Failed to remove existing archive: {}",
                    left.display()
                ));
            }
        }
    }

    // Step 7: Atomic rename: temp → final
    match atomic_finalize(&temp_output_path, &final_output_path) {
        Ok(()) => Ok(FinalizedOutput {
            path: final_output_path,
            collisions,
        }),
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
    let outputs_dir = resolve_outputs_dir(app_handle)?;

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
        .unwrap_or(output_path.as_os_str());
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
) -> Result<(PathBuf, Vec<DepotCollision>), String> {
    // Temp assembly lives in the scratch dir, which is on the same volume as the
    // final outputs dir (a hidden subfolder of the custom output dir, or <base>
    // by default), so the temp → final move stays an atomic rename.
    let temp_dir = resolve_scratch_dir(app_handle)?.join(format!(".tmp_{}", job_id));

    // Clean up temp directory if it exists from a previous failure
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir)
            .map_err(|e| format!("Failed to cleanup existing temp directory: {}", e))?;
    }

    // Create temp directory structure
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temp directory: {}", e))?;

    // A failure partway through assembly (most often a full disk while copying
    // the game files) would otherwise leave a game-sized .tmp_<job> copy behind
    // that nothing ever sweeps. Remove it before reporting the error.
    let collisions = assemble_temp_output(&temp_dir, staging_dir, metadata).inspect_err(|_| {
        let _ = fs::remove_dir_all(&temp_dir);
    })?;

    Ok((temp_dir, collisions))
}

/// Fills `temp_dir` with the final Steam-style layout (steamapps/common,
/// depotcache, .acf manifests). Split from `build_temp_output` so any error can
/// be cleaned up in one place.
fn assemble_temp_output(
    temp_dir: &Path,
    staging_dir: &Path,
    metadata: &JobMetadataFile,
) -> Result<Vec<DepotCollision>, String> {
    // Determine installdir: must match the on-disk folder name that all non-shared depot
    // files will be merged into. Steam's canonical `config.installdir` (e.g. "The Crust")
    // is the authoritative source and is used verbatim when available — including its spaces,
    // since Steam creates `steamapps/common/The Crust` literally. When it isn't (the
    // best-effort api.steamcmd.net lookup failed), fall back to the game name. The depot name
    // is deliberately NOT used: it carries a platform suffix (e.g. "Project Zomboid - windows")
    // that produces ugly folders. We sanitize with `sanitize_install_dir` (not the archive
    // sanitizer), which strips only Windows-illegal characters and PRESERVES spaces, so the
    // common folder matches Steam exactly instead of dotting "The Crust" into "The.Crust".
    // The `.acf` installdir value is written from this same string, keeping folder and
    // manifest consistent.
    let install_dir_name = metadata
        .install_dir
        .as_deref()
        .map(sanitize_install_dir)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| sanitize_install_dir(&metadata.game_name));

    // Compute per-depot sizes from the staging structure BEFORE the merge.
    // After merge, all non-shared depot files live in one folder and individual sizes
    // can no longer be determined.
    let depot_sizes = compute_depot_sizes_from_staging(staging_dir, metadata);

    // Transform depots/ → steamapps/common/ and collect manifests → depotcache/
    // Returns a map of depot_id → manifest_id and a map of depot_id → install-script path
    let (manifest_map, install_scripts, collisions) =
        transform_depots_to_steamapps(staging_dir, temp_dir, &install_dir_name)?;

    // Generate appmanifest_<appid>.acf and appmanifest_228980.acf (shared redistributables)
    let steamapps_dir = temp_dir.join("steamapps");
    let common_dir = steamapps_dir.join("common");
    acf_generator::write_acf_file(&steamapps_dir, metadata, &common_dir, &install_dir_name, &manifest_map, &depot_sizes)?;

    // Best-effort: fetch the real public buildid for the redistributables app (228980)
    // so its manifest matches what Steam records. Falls back to "0" on any failure.
    let shared_buildid = crate::steamcmd_api::fetch_public_buildid("228980")
        .unwrap_or_else(|| "0".to_string());
    acf_generator::write_shared_depots_acf(&steamapps_dir, metadata, &common_dir, &manifest_map, &depot_sizes, &install_scripts, &shared_buildid)?;

    Ok(collisions)
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
        if compression_enabled && existing_archive_output(&resolve_archive_path(&candidate)).is_some()
        {
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

/// Returns the path of an existing archive output for `archive_path`, covering
/// both the single-file form (`X.7z`) and the split form (`X.7z.001`, ...).
/// Split runs never create `X.7z` itself, so checking only that path missed a
/// prior split output entirely: no conflict prompt, then 7-Zip refused to
/// overwrite and the job "completed" with an uncompressed folder.
fn existing_archive_output(archive_path: &Path) -> Option<PathBuf> {
    if archive_path.exists() {
        return Some(archive_path.to_path_buf());
    }
    let first_volume = crate::depot_runner::first_volume_path(archive_path);
    first_volume.exists().then_some(first_volume)
}

/// Computes per-depot file sizes from the staging directory structure.
///
/// This must run BEFORE the merge into steamapps/common/ because after the merge,
/// all non-shared depot files share one folder and individual sizes are lost.
///
/// DepotDownloader layout: depots/<depot_id>/<manifest_id>/(files + .DepotDownloader/)
/// We walk each depot's content (excluding .DepotDownloader/) to get its real size.
fn compute_depot_sizes_from_staging(
    staging_dir: &Path,
    metadata: &JobMetadataFile,
) -> HashMap<String, u64> {
    let depots_dir = staging_dir.join("depots");
    let mut sizes = HashMap::new();

    for depot in &metadata.depots {
        let depot_path = depots_dir.join(&depot.depot_id);
        if !depot_path.is_dir() {
            continue;
        }

        // Find the manifest subdirectory (should be only one)
        let manifest_dir = match fs::read_dir(&depot_path) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .find(|e| e.path().is_dir())
                .map(|e| e.path()),
            Err(_) => None,
        };

        if let Some(dir) = manifest_dir {
            // Walk the content, excluding .DepotDownloader/
            let size = calculate_dir_size_filtered(&dir, |path| {
                !path.file_name().map(|n| n == ".DepotDownloader").unwrap_or(false)
            });
            sizes.insert(depot.depot_id.clone(), size);
        }
    }

    sizes
}

/// Recursively calculates directory size with a filter predicate
fn calculate_dir_size_filtered<F>(path: &Path, filter: F) -> u64
where
    F: Fn(&Path) -> bool + Copy,
{
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let entry_path = entry.path();
            if !filter(&entry_path) {
                continue;
            }
            if entry_path.is_dir() {
                total += calculate_dir_size_filtered(&entry_path, filter);
            } else if entry_path.is_file() {
                total += fs::metadata(&entry_path).map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    total
}

/// Transforms DepotDownloader's depots/ structure into Steam-compatible steamapps/common/ structure
///
/// DepotDownloader creates: depots/<depotid>/<manifestid>/(files + .DepotDownloader/)
///
/// We need to create:
/// - steamapps/common/<installdir>/(all non-shared depot files merged flat, like Steam does)
/// - steamapps/common/<SharedDepotName>/(shared depot files, e.g. "Steamworks Shared")
/// - depotcache/*.manifest (collected from all .DepotDownloader/ directories)
///
/// All non-shared depots merge into a single installdir folder. Shared depots (redistributables,
/// runtimes) each get their own sibling folder. This matches real Steam's on-disk layout.
///
/// # Returns
/// A tuple of:
/// - map of depot_id → actual manifest_id (extracted from .manifest filenames)
/// - map of depot_id → install-script path, relative to the depot's installdir, using
///   Windows-style backslashes (only populated for depots that ship an `installscript.vdf`)
/// - files a later non-shared depot overwrote during the merge (see [`DepotCollision`])
fn transform_depots_to_steamapps(
    staging_dir: &Path,
    temp_dir: &Path,
    install_dir_name: &str,
) -> Result<DepotTransform, String> {
    use crate::shared_depots::{get_shared_depot_install_dir, is_shared_depot};

    let depots_dir = staging_dir.join("depots");
    let steamapps_common_dir = temp_dir.join("steamapps").join("common");
    let depotcache_dir = temp_dir.join("depotcache");

    // Map of depot_id → actual manifest_id (extracted from .manifest filenames)
    let mut manifest_map: HashMap<String, String> = HashMap::new();
    // Map of depot_id → install-script path (relative, Windows-style separators)
    let mut install_scripts: HashMap<String, String> = HashMap::new();

    // Create directories
    fs::create_dir_all(&steamapps_common_dir)
        .map_err(|e| format!("Failed to create steamapps/common/: {}", e))?;
    fs::create_dir_all(&depotcache_dir)
        .map_err(|e| format!("Failed to create depotcache/: {}", e))?;

    // Files overwritten by a later non-shared depot during the merge.
    let mut collisions: Vec<DepotCollision> = Vec::new();

    // Merge in numeric depot-ID order. read_dir order is filesystem-dependent,
    // so when depots overlap, which one's file survived used to vary by OS.
    let mut depot_entries: Vec<_> = fs::read_dir(&depots_dir)
        .map_err(|e| format!("Failed to read depots directory: {}", e))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("Failed to read depot entry: {}", e))?;
    depot_entries.sort_by_key(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        (name.parse::<u64>().unwrap_or(u64::MAX), name)
    });

    for entry in depot_entries {
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

                    // Extract manifest ID from filename. DepotDownloader names these
                    // {depot_id}_{manifest_id}.manifest, so strip both the extension and
                    // the depot-id prefix to get the bare manifest ID Steam's .acf expects.
                    if let Some(stem) = manifest_filename.strip_suffix(".manifest") {
                        let manifest_id = stem
                            .strip_prefix(&format!("{}_", depot_id))
                            .unwrap_or(stem);
                        manifest_map.insert(depot_id.clone(), manifest_id.to_string());
                    }

                    fs::copy(&manifest_path, depotcache_dir.join(&manifest_filename))
                        .map_err(|e| format!("Failed to copy manifest file: {}", e))?;
                }
            }
        }

        // Determine target directory:
        // - Shared depots owned by the same app merge into that app's installdir
        //   (e.g. all 228980-owned depots → "Steamworks Shared/", matching real Steam)
        // - Non-shared depots merge into the game's installdir folder
        let target_dir = if is_shared_depot(&depot_id) {
            steamapps_common_dir.join(get_shared_depot_install_dir(&depot_id))
        } else {
            steamapps_common_dir.join(install_dir_name)
        };

        // For shared depots, look for an installscript.vdf inside the depot's own content.
        // Steam's InstallScripts section maps depot_id → path relative to the installdir.
        // The script ships inside the depot payload, so we derive the mapping from the
        // files themselves rather than from a hardcoded table.
        if is_shared_depot(&depot_id) {
            if let Some(rel) = find_install_script(&manifest_dir) {
                install_scripts.insert(depot_id.clone(), rel);
            }
        }

        // Move (not copy) the depot content into place. Staging and the temp
        // assembly dir share a volume, so this is a metadata-only rename: a
        // 150 GB game no longer needs a second full copy on disk (peak usage
        // was staging + copy + archive, ~3x the game) nor an hour of copying.
        // Staging is discarded after finalization either way. The
        // .DepotDownloader bookkeeping dir is filtered out and stays in staging.
        let mut replaced = Vec::new();
        move_dir_merge(
            &manifest_dir,
            &target_dir,
            |path| !path.file_name().map(|n| n == ".DepotDownloader").unwrap_or(false),
            &mut replaced,
        )?;

        // Shared redist depots overlapping each other is normal; two game
        // depots writing the same file is not (e.g. a global and a Steam China
        // depot both selected), and the later depot silently won.
        if !is_shared_depot(&depot_id) && !replaced.is_empty() {
            let mut files: Vec<String> = replaced
                .iter()
                .map(|p| {
                    p.strip_prefix(&steamapps_common_dir)
                        .unwrap_or(p)
                        .to_string_lossy()
                        .replace('\\', "/")
                })
                .collect();
            files.sort();
            collisions.push(DepotCollision { depot_id: depot_id.clone(), files });
        }
    }

    Ok((manifest_map, install_scripts, collisions))
}

/// Searches a depot's content root for an `installscript.vdf` and returns its path
/// relative to that root, using forward slashes. The ACF writer converts these to the
/// escaped backslash form Steam's `InstallScripts` section expects. Returns `None` if
/// no script is found.
fn find_install_script(content_root: &Path) -> Option<String> {
    scan_for_install_script(content_root, content_root)
}

/// Recursively walks `dir` looking for a file named `installscript.vdf`
/// (case-insensitive), returning its path relative to `root` with forward slashes.
fn scan_for_install_script(dir: &Path, root: &Path) -> Option<String> {
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            // Skip DepotDownloader bookkeeping directories
            if path.file_name().map(|n| n == ".DepotDownloader").unwrap_or(false) {
                continue;
            }
            subdirs.push(path);
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case("installscript.vdf"))
            .unwrap_or(false)
        {
            return path
                .strip_prefix(root)
                .ok()
                .map(|rel| rel.to_string_lossy().replace('\\', "/"));
        }
    }
    // Recurse into subdirectories only after checking files at this level
    for sub in subdirs {
        if let Some(found) = scan_for_install_script(&sub, root) {
            return Some(found);
        }
    }
    None
}

/// Step 6: Atomic rename from temp to final
fn atomic_finalize(temp_path: &Path, final_path: &Path) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create outputs directory: {}", e))?;
    }

    // Atomic rename. Temp lives in the scratch dir and final in the outputs dir,
    // both rooted at the same place (the custom output folder, or <base>), so
    // they're on one filesystem and this is a real rename, not a cross-device copy.
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

/// Moves the contents of `src` into `dst`, merging with anything already there.
///
/// Entries whose destination doesn't exist yet are renamed wholesale (a whole
/// subtree in one cheap metadata operation). When both sides are directories
/// the move recurses so multiple depots can merge into one install folder. An
/// existing destination file is replaced, matching the previous copy-merge,
/// and its path is pushed to `replaced` so the caller can report collisions.
///
/// If a rename fails (e.g. the scratch dir somehow spans volumes), that entry
/// falls back to copy-then-delete, so the result is the same, just slower.
///
/// `filter` receives each source path; returning false leaves it in `src`.
fn move_dir_merge<F>(
    src: &Path,
    dst: &Path,
    filter: F,
    replaced: &mut Vec<PathBuf>,
) -> Result<(), String>
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

        if !filter(&src_path) {
            continue;
        }

        let dst_path = dst.join(entry.file_name());
        let src_is_dir = src_path.is_dir();

        if src_is_dir && dst_path.is_dir() {
            move_dir_merge(&src_path, &dst_path, filter, replaced)?;
            continue;
        }

        if !src_is_dir && dst_path.is_file() {
            replaced.push(dst_path.clone());
        }

        if fs::rename(&src_path, &dst_path).is_ok() {
            continue;
        }

        // Fallback: copy then remove the source.
        if src_is_dir {
            copy_dir_recursive_filtered(&src_path, &dst_path, filter)?;
            fs::remove_dir_all(&src_path).map_err(|e| {
                format!("Failed to remove moved directory {}: {}", src_path.display(), e)
            })?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to move file {} to {}: {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
            let _ = fs::remove_file(&src_path);
        }
    }

    Ok(())
}

/// Recursively copies a directory and all its contents with filtering. Only
/// used as the cross-volume fallback for [`move_dir_merge`].
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    /// Creates a unique temporary directory for a test and returns its path.
    fn temp_dir(label: &str) -> PathBuf {
        let mut dir = env::temp_dir();
        let unique = format!(
            "omnipacker_test_{}_{}",
            label,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        dir.push(unique);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_find_install_script_nested() {
        let root = temp_dir("find_script");
        let script_dir = root.join("_CommonRedist").join("vcredist").join("2012");
        fs::create_dir_all(&script_dir).unwrap();
        fs::write(script_dir.join("installscript.vdf"), b"\"InstallScript\"\n{\n}\n").unwrap();

        let result = find_install_script(&root);
        assert_eq!(
            result,
            Some("_CommonRedist/vcredist/2012/installscript.vdf".to_string())
        );

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn move_dir_merge_moves_and_merges_depots() {
        // Two depots sharing a subfolder merge into one install dir; files are
        // moved (gone from staging), and .DepotDownloader stays behind.
        let root = temp_dir("move_merge");
        let depot_a = root.join("depots/100/build");
        let depot_b = root.join("depots/101/build");
        fs::create_dir_all(depot_a.join("bin")).unwrap();
        fs::create_dir_all(depot_a.join(".DepotDownloader")).unwrap();
        fs::create_dir_all(depot_b.join("bin")).unwrap();
        fs::create_dir_all(depot_b.join("data/deep")).unwrap();
        fs::write(depot_a.join("bin/game.exe"), b"exe").unwrap();
        fs::write(depot_a.join(".DepotDownloader/100_1.manifest"), b"m").unwrap();
        fs::write(depot_b.join("bin/extra.dll"), b"dll").unwrap();
        fs::write(depot_b.join("data/deep/pak0"), b"pak").unwrap();

        let target = root.join("out/steamapps/common/Game");
        let keep_dd = |p: &Path| !p.file_name().map(|n| n == ".DepotDownloader").unwrap_or(false);
        let mut replaced = Vec::new();
        move_dir_merge(&depot_a, &target, keep_dd, &mut replaced).unwrap();
        move_dir_merge(&depot_b, &target, keep_dd, &mut replaced).unwrap();
        assert!(replaced.is_empty(), "no overlapping files, no collisions");

        assert_eq!(fs::read(target.join("bin/game.exe")).unwrap(), b"exe");
        assert_eq!(fs::read(target.join("bin/extra.dll")).unwrap(), b"dll");
        assert_eq!(fs::read(target.join("data/deep/pak0")).unwrap(), b"pak");
        assert!(!target.join(".DepotDownloader").exists());

        // Moved, not copied.
        assert!(!depot_a.join("bin/game.exe").exists());
        assert!(!depot_b.join("data").exists());
        // Bookkeeping left in staging (still needed for the depotcache copy).
        assert!(depot_a.join(".DepotDownloader/100_1.manifest").exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn transform_reports_game_depot_collisions_in_id_order() {
        // Wonderia shape: global (2593341) and Steam China (2593343) Windows
        // depots both downloaded, same files. The higher ID merges last and its
        // overwrites are reported. Overlapping shared redists are not.
        let root = temp_dir("collisions");
        let staging = root.join("staging");
        let out = root.join("out");
        let put = |depot: &str, rel: &str, body: &[u8]| {
            let p = staging.join("depots").join(depot).join("25669041").join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        };
        put("2593343", "Wonderia.exe", b"china");
        put("2593343", "data/a.pak", b"china");
        put("2593341", "Wonderia.exe", b"global");
        put("2593341", "data/a.pak", b"global");
        put("2593341", "data/global_only.pak", b"global");
        put("228989", "_CommonRedist/vcredist/2022/installscript.vdf", b"x");
        put("228990", "_CommonRedist/vcredist/2022/installscript.vdf", b"y");

        let (_, _, collisions) = transform_depots_to_steamapps(&staging, &out, "Wonderia").unwrap();

        assert_eq!(
            collisions,
            vec![DepotCollision {
                depot_id: "2593343".into(),
                files: vec!["Wonderia/Wonderia.exe".into(), "Wonderia/data/a.pak".into()],
            }]
        );
        // Deterministic: the numerically-later depot's content wins.
        let game = out.join("steamapps/common/Wonderia");
        assert_eq!(fs::read(game.join("Wonderia.exe")).unwrap(), b"china");
        assert!(game.join("data/global_only.pak").exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn move_dir_merge_replaces_existing_file() {
        let root = temp_dir("move_replace");
        let src = root.join("src");
        let dst = root.join("dst");
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        fs::write(src.join("same.txt"), b"new").unwrap();
        fs::write(dst.join("same.txt"), b"old").unwrap();

        let mut replaced = Vec::new();
        move_dir_merge(&src, &dst, |_| true, &mut replaced).unwrap();
        assert_eq!(fs::read(dst.join("same.txt")).unwrap(), b"new");
        assert_eq!(replaced, vec![dst.join("same.txt")]);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn existing_archive_output_detects_split_volumes() {
        // Regression: a prior split run leaves X.7z.001 (never X.7z), which the
        // conflict check used to miss entirely.
        let root = temp_dir("split_conflict");
        let archive = root.join("Game.Build.1.Win64.Public.7z");
        assert_eq!(existing_archive_output(&archive), None);

        fs::write(root.join("Game.Build.1.Win64.Public.7z.001"), b"x").unwrap();
        assert_eq!(
            existing_archive_output(&archive),
            Some(root.join("Game.Build.1.Win64.Public.7z.001"))
        );

        // The copy-name search must also skip past a split-occupied name.
        let candidate = resolve_copy_output_path(&root.join("Game.Build.1.Win64.Public"), true)
            .unwrap();
        assert_eq!(candidate, root.join("Game.Build.1.Win64.Public (1)"));
        fs::write(root.join("Game.Build.1.Win64.Public (1).7z.001"), b"x").unwrap();
        let candidate = resolve_copy_output_path(&root.join("Game.Build.1.Win64.Public"), true)
            .unwrap();
        assert_eq!(candidate, root.join("Game.Build.1.Win64.Public (2)"));

        // Overwrite path: removing outputs clears every volume.
        fs::write(root.join("Game.Build.1.Win64.Public.7z.002"), b"x").unwrap();
        crate::depot_runner::remove_archive_outputs(&archive);
        assert_eq!(existing_archive_output(&archive), None);
        assert!(!root.join("Game.Build.1.Win64.Public.7z.002").exists());

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_find_install_script_absent() {
        let root = temp_dir("no_script");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::write(root.join("data").join("game.bin"), b"x").unwrap();

        assert_eq!(find_install_script(&root), None);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn test_find_install_script_case_insensitive() {
        let root = temp_dir("case_script");
        fs::write(root.join("InstallScript.VDF"), b"x").unwrap();

        assert_eq!(find_install_script(&root), Some("InstallScript.VDF".to_string()));

        fs::remove_dir_all(&root).ok();
    }
}
