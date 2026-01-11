use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::debug_console::DebugConsoleState;
use crate::job_finalization::{finalize_job, resolve_archive_path};
use crate::job_metadata::{BuildIdSource, DepotInfo, JobMetadataFile};
use crate::job_staging::{cleanup_staging_dir, create_staging_dir, generate_job_id};
use crate::manifest_preflight::{build_preflight_args, parse_preflight_output};
use crate::output_dir::resolve_downloads_dir;
use crate::steam_api::fetch_app_info;
use crate::steamdb_api::fetch_build_date;
use crate::template_metadata::{TemplateMetadata, TemplateMetadataState};
use crate::template_renderer::write_template_file;
use crate::template_store::load_template_data_internal;
use crate::zip_runner::{calculate_7z_compression_args, run_7zip_blocking, SevenZipRunnerState};

/// Metadata for a download job, received from the frontend
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobMetadata {
    pub app_id: String,
    pub os: String,
    pub branch: String,
    pub username: String,
    pub password: String,
    pub qr_enabled: bool,
    #[serde(default)]
    #[allow(dead_code)] // Forwarded from the frontend; reserved for future auth caching control.
    pub remember_password: bool,
    #[serde(default)]
    pub skip_compression: bool,
    #[serde(default)]
    pub compression_password_enabled: bool,
    #[serde(default)]
    pub compression_password: String,
}

/// Internal state tracking the running job
struct RunningJobState {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    job_id: Option<String>,
    build_datetime_utc: Option<DateTime<Utc>>,
    // Track per-depot timestamps and depot-manifest mappings during download
    depot_timestamps: std::collections::HashMap<String, DateTime<Utc>>,
    manifest_to_depot: std::collections::HashMap<String, String>,
    manifest_timestamps: std::collections::HashMap<String, DateTime<Utc>>, // manifest_id -> timestamp
    last_depot_mentioned: Option<String>,
    auth_username: Option<String>,
    // Track depot names from preflight (depot_id -> depot_name)
    depot_names: std::collections::HashMap<String, String>,
    // Join handles for log reader threads (to ensure all logs are parsed before metadata derivation)
    log_reader_threads: Option<(thread::JoinHandle<()>, thread::JoinHandle<()>)>,
}

#[derive(Clone)]
pub struct DepotRunnerState {
    inner: Arc<Mutex<RunningJobState>>,
}

impl DepotRunnerState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RunningJobState {
                child: None,
                stdin: None,
                job_id: None,
                build_datetime_utc: None,
                depot_timestamps: std::collections::HashMap::new(),
                manifest_to_depot: std::collections::HashMap::new(),
                manifest_timestamps: std::collections::HashMap::new(),
                last_depot_mentioned: None,
                auth_username: None,
                depot_names: std::collections::HashMap::new(),
                log_reader_threads: None,
            })),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusPayload {
    status: String,
    code: Option<i32>,
    job_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogPayload {
    stream: String,
    line: String,
    job_id: String,
}

/// Determines the platform-specific subdirectory name for binaries
fn get_platform_subdir() -> &'static str {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return "win-x64";

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    return "win-arm64";

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return "linux-x64";

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return "linux-arm64";

    #[cfg(all(target_os = "linux", target_arch = "arm"))]
    return "linux-arm";

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return "macos-x64";

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return "macos-arm64";

    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "arm"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )))]
    return "unknown";
}

pub fn resolve_depotdownloader_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    // Determine platform-specific binary name with extension
    #[cfg(windows)]
    let binary_name = "DepotDownloader.exe";
    #[cfg(not(windows))]
    let binary_name = "DepotDownloader";

    let platform_subdir = get_platform_subdir();

    // Use Tauri's path resolution with platform-specific subdirectory
    let sidecar_path = app_handle
        .path()
        .resolve(
            format!("binaries/{}/{}", platform_subdir, binary_name),
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Failed to resolve DepotDownloader sidecar: {}", e))?;

    if !sidecar_path.exists() {
        return Err(format!(
            "DepotDownloader sidecar not found at {}",
            sidecar_path.display()
        ));
    }

    if !is_executable(&sidecar_path) {
        return Err(format!(
            "DepotDownloader sidecar is not executable at {}",
            sidecar_path.display()
        ));
    }

    Ok(sidecar_path)
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

/// Derives metadata from downloaded content (for QR auth case where preflight was skipped)
fn derive_metadata_from_download(
    app_handle: &AppHandle,
    job: &JobMetadata,
    job_id: &str,
    staging_dir: &std::path::Path,
) -> Result<(), String> {
    use std::fs;

    // Fetch game name from Steam API
    let game_name = match fetch_app_info(&job.app_id) {
        Ok(info) => info.name,
        Err(_) => format!("app_{}", job.app_id), // Fallback
    };

    let depots_dir = staging_dir.join("depots");
    let mut depots = Vec::new();
    let mut primary_depot_id = String::new();
    let mut build_id = String::new();

    // Scan depots directory
    for entry in fs::read_dir(&depots_dir)
        .map_err(|e| format!("Failed to read depots directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read depot entry: {}", e))?;
        let depot_path = entry.path();

        if !depot_path.is_dir() {
            continue;
        }

        let depot_id = entry.file_name().to_string_lossy().to_string();

        // Skip .DepotDownloader directory
        if depot_id == ".DepotDownloader" {
            continue;
        }

        // Find manifest directory
        let manifest_dirs: Vec<_> = fs::read_dir(&depot_path)
            .map_err(|e| format!("Failed to read depot {}: {}", depot_id, e))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        if let Some(manifest_entry) = manifest_dirs.first() {
            let manifest_id = manifest_entry.file_name().to_string_lossy().to_string();

            // Use first manifest as build ID if not set
            if build_id.is_empty() {
                build_id = manifest_id.clone();
            }

            // Use first NON-SHARED depot as primary
            use crate::steam_api::is_shared_depot;
            if primary_depot_id.is_empty() && !is_shared_depot(&depot_id) {
                primary_depot_id = depot_id.clone();
            }

            depots.push(DepotInfo {
                depot_id: depot_id.clone(),
                depot_name: format!("depot_{}", depot_id), // Fallback name - will be enhanced below
                manifest_id,
                manifest_id_used: None,
            });
        }
    }

    // If no primary depot was found (all depots are shared), use the first one
    if primary_depot_id.is_empty() && !depots.is_empty() {
        primary_depot_id = depots[0].depot_id.clone();
    }

    if depots.is_empty() {
        return Err("No depots found in download".to_string());
    }

    // Enhance depot names using proper naming strategy
    // First, try to get depot names from preflight (if available)
    let preflight_depot_names = {
        app_handle
            .state::<DepotRunnerState>()
            .inner
            .lock()
            .ok()
            .map(|guard| guard.depot_names.clone())
            .unwrap_or_default()
    };

    use crate::steam_api::get_depot_name;
    for depot in &mut depots {
        // Priority 1: Use depot name from preflight if available
        if let Some(name) = preflight_depot_names.get(&depot.depot_id) {
            depot.depot_name = name.clone();
        } else {
            // Priority 2: Use naming strategy (primary depot or shared depot)
            let is_primary = depot.depot_id == primary_depot_id;
            depot.depot_name = get_depot_name(&depot.depot_id, is_primary, &game_name);
        }
    }

    // Normalize branch name (capitalize first letter)
    let branch_normalized = capitalize_first(&job.branch);

    // Normalize platform using the same logic as metadata_resolver
    let platform_normalized = map_platform_for_output(&job.os);

    // Get build timestamp - PRIMARY: SteamDB API, FALLBACK: manifest timestamps
    // Find the primary depot's manifest ID first
    let primary_manifest_id = depots
        .iter()
        .find(|d| d.depot_id == primary_depot_id)
        .map(|d| d.manifest_id.clone());

    // PRIMARY: Query SteamDB for build release date
    let mut build_datetime_utc = match fetch_build_date(&job.app_id, Some(&build_id)) {
        Ok(timestamp) => {
            eprintln!("[STEAMDB] Got build date for app {}: {}", job.app_id, timestamp);
            Some(timestamp)
        }
        Err(err) => {
            eprintln!("[STEAMDB] Failed to get build date: {}", err);
            None
        }
    };

    // FALLBACK: Use manifest timestamps from download if SteamDB failed
    if build_datetime_utc.is_none() {
        let state_handle = app_handle.state::<DepotRunnerState>().inner.clone();
        build_datetime_utc = state_handle.lock().ok().and_then(|guard| {
            // Try timeupdated if captured from DepotDownloader output
            if let Some(ts) = guard.build_datetime_utc {
                return Some(ts);
            }

            // Try manifest timestamp for primary depot
            if let Some(ref manifest_id) = primary_manifest_id {
                if let Some(ts) = guard.manifest_timestamps.get(manifest_id) {
                    return Some(*ts);
                }
            }

            // Try depot timestamps
            if let Some(ts) = guard.depot_timestamps.get(&primary_depot_id) {
                return Some(*ts);
            }

            None
        });
    }

    // Create job metadata
    let job_metadata = JobMetadataFile::new(
        job_id.to_string(),
        job.app_id.clone(),
        branch_normalized,
        platform_normalized,
        primary_depot_id,
        game_name,
        build_id,
        BuildIdSource::PrimaryManifestId, // We're using manifest ID since we don't have app-level BuildId
        build_datetime_utc,
        depots,
    );

    // Write job.json
    job_metadata.write_to_dir(staging_dir)?;

    emit_log(
        app_handle,
        "system",
        "Metadata derived from download output",
        job_id,
    );

    Ok(())
}

/// Maps OS selection to platform string for output naming (duplicated from metadata_resolver)
fn map_platform_for_output(os: &str) -> String {
    match os {
        "Windows x64" => "Win64".to_string(),
        "Windows x86" => "Win32".to_string(),
        "Linux" => "Linux64".to_string(),
        "macOS x64" => "MacOS64".to_string(),
        "macOS arm64" => "MacOSArm64".to_string(),
        "macOS" => "MacOS64".to_string(),
        _ => "Win64".to_string(),
    }
}

/// Capitalizes the first letter of a string
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

fn redact_7z_password_args(args: &[String]) -> Vec<String> {
    args.iter()
        .map(|arg| {
            if arg.starts_with("-p") {
                "-p********".to_string()
            } else {
                arg.clone()
            }
        })
        .collect()
}

/// Compresses the finalized output folder using 7-Zip.
/// On success, deletes the uncompressed folder and returns the archive path.
/// On failure, leaves the folder intact and returns an error.
fn compress_output(
    app_handle: &AppHandle,
    output_path: &std::path::Path,
    job_id: &str,
    compression_password: Option<&str>,
) -> Result<std::path::PathBuf, String> {
    let archive_path = resolve_archive_path(output_path);

    if archive_path.exists() {
        return Err(format!(
            "Archive already exists: {}",
            archive_path.display()
        ));
    }

    let args =
        calculate_7z_compression_args(output_path, &archive_path, compression_password);
    let redacted_args = redact_7z_password_args(&args);

    emit_log(
        app_handle,
        "system",
        &format!("7-Zip command: 7zz {}", redacted_args.join(" ")),
        job_id,
    );

    let zip_state = app_handle.state::<SevenZipRunnerState>();
    let exit_code = match run_7zip_blocking(app_handle, &zip_state, args) {
        Ok(code) => code,
        Err(err) => {
            let _ = std::fs::remove_file(&archive_path);
            return Err(err);
        }
    };

    if exit_code != 0 {
        // Clean up partial archive if it exists
        let _ = std::fs::remove_file(&archive_path);
        return Err(format!("7-Zip exited with code {}", exit_code));
    }

    if !archive_path.exists() {
        return Err("Archive not found after compression".to_string());
    }

    // Default behavior: Remove uncompressed folder after successful compression
    // Future: Make this configurable via settings (keep_uncompressed)
    emit_log(
        app_handle,
        "system",
        "Removing uncompressed folder...",
        job_id,
    );

    if let Err(e) = std::fs::remove_dir_all(output_path) {
        emit_log(
            app_handle,
            "system",
            &format!(
                "Warning: Failed to remove folder: {}. Archive still created successfully.",
                e
            ),
            job_id,
        );
        // Don't fail job - archive was created successfully
    } else {
        emit_log(
            app_handle,
            "system",
            "Uncompressed folder removed.",
            job_id,
        );
    }

    Ok(archive_path)
}

/// Builds DepotDownloader command-line arguments from job metadata
fn build_depot_args(job: &JobMetadata) -> Result<Vec<String>, String> {
    let mut args = Vec::new();

    if !job.app_id.is_empty() && job.app_id != "unknown" {
        args.push("-app".to_string());
        args.push(job.app_id.clone());
    }

    if !job.branch.is_empty() {
        args.push("-branch".to_string());
        args.push(job.branch.clone());
    }

    let (os, arch) = map_os_selection(&job.os);
    args.push("-os".to_string());
    args.push(os.to_string());
    args.push("-osarch".to_string());
    args.push(arch.to_string());

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

const AUTH_ROOT_FILES: &[&str] = &["sentry.bin", "config.json", "loginusers.vdf"];
const AUTH_CONFIG_FILES: &[&str] = &["loginusers.vdf", "config.vdf", "config.json", "sentry.bin"];

fn is_auth_root_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("ssfn") {
        return true;
    }
    AUTH_ROOT_FILES.iter().any(|entry| lower == *entry)
}

fn is_auth_config_file(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    AUTH_CONFIG_FILES.iter().any(|entry| lower == *entry)
}

fn collect_auth_files(root: &Path) -> Result<Vec<(PathBuf, PathBuf)>, String> {
    let mut files = Vec::new();

    for entry in std::fs::read_dir(root)
        .map_err(|e| format!("Failed to read auth source directory: {}", e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read auth source entry: {}", e))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = match path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string())
        {
            Some(name) => name,
            None => continue,
        };
        if is_auth_root_file(&file_name) {
            files.push((path, PathBuf::from(file_name)));
        }
    }

    let config_dir = root.join("config");
    if config_dir.is_dir() {
        for entry in std::fs::read_dir(&config_dir)
            .map_err(|e| format!("Failed to read auth config directory: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read auth config entry: {}", e))?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = match path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
            {
                Some(name) => name,
                None => continue,
            };
            if is_auth_config_file(&file_name) {
                files.push((path, PathBuf::from("config").join(file_name)));
            }
        }
    }

    Ok(files)
}

fn sanitize_auth_username(username: &str) -> String {
    let mut sanitized = String::with_capacity(username.len());
    for ch in username.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "user".to_string()
    } else {
        trimmed.to_string()
    }
}

fn resolve_auth_cache_dir(app_handle: &AppHandle, username: &str) -> Result<PathBuf, String> {
    let downloads_dir = resolve_downloads_dir(app_handle)?;
    let auth_root = downloads_dir.join(".auth");
    Ok(auth_root.join(sanitize_auth_username(username)))
}

fn restore_auth_cache(
    app_handle: &AppHandle,
    username: &str,
    target_dir: &Path,
    job_id: &str,
) -> Result<(), String> {
    if username.trim().is_empty() {
        return Ok(());
    }
    let cache_dir = resolve_auth_cache_dir(app_handle, username)?;
    if !cache_dir.exists() {
        return Ok(());
    }

    let entries = collect_auth_files(&cache_dir)?;
    let mut restored = Vec::new();
    for (path, rel_path) in entries {
        let dest = target_dir.join(&rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create auth restore directory: {}", e))?;
        }
        std::fs::copy(&path, &dest)
            .map_err(|e| format!("Failed to restore auth cache file: {}", e))?;
        restored.push(rel_path.to_string_lossy().replace('\\', "/"));
    }

    if !restored.is_empty() {
        emit_log(
            app_handle,
            "system",
            &format!("Auth cache restored: {}", restored.join(", ")),
            job_id,
        );
    }

    Ok(())
}

fn persist_auth_cache(
    app_handle: &AppHandle,
    username: &str,
    source_dir: &Path,
    job_id: &str,
) -> Result<(), String> {
    if username.trim().is_empty() {
        return Ok(());
    }
    let cache_dir = resolve_auth_cache_dir(app_handle, username)?;
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create auth cache directory: {}", e))?;

    let entries = collect_auth_files(source_dir)?;
    let mut persisted = Vec::new();
    for (path, rel_path) in entries {
        let dest = cache_dir.join(&rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create auth cache directory: {}", e))?;
        }
        std::fs::copy(&path, &dest)
            .map_err(|e| format!("Failed to persist auth cache file: {}", e))?;
        persisted.push(rel_path.to_string_lossy().replace('\\', "/"));
    }

    if !persisted.is_empty() {
        emit_log(
            app_handle,
            "system",
            &format!("Auth cache saved: {}", persisted.join(", ")),
            job_id,
        );
    }

    Ok(())
}

fn resolve_auth_username(
    state_handle: &Arc<Mutex<RunningJobState>>,
    job: &JobMetadata,
    job_id: &str,
) -> Option<String> {
    if let Ok(guard) = state_handle.lock() {
        if guard.job_id.as_deref() == Some(job_id) {
            if let Some(username) = guard.auth_username.as_ref() {
                if !username.trim().is_empty() {
                    return Some(username.clone());
                }
            }
        }
    }

    let trimmed = job.username.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn maybe_update_auth_username(
    state_handle: &Arc<Mutex<RunningJobState>>,
    line: &str,
    job_id: &str,
) {
    if !line.contains("-remember-password") || !line.contains("-username") {
        return;
    }

    let mut parts = line.split_whitespace();
    while let Some(part) = parts.next() {
        if part == "-username" {
            if let Some(username) = parts.next() {
                if let Ok(mut guard) = state_handle.lock() {
                    if guard.job_id.as_deref() == Some(job_id) {
                        guard.auth_username = Some(username.to_string());
                    }
                }
            }
            break;
        }
    }
}

#[tauri::command]
pub fn run_depotdownloader(
    app_handle: AppHandle,
    state: State<'_, DepotRunnerState>,
    job: JobMetadata,
) -> Result<String, String> {
    let job_id = {
        let mut guard = state
            .inner
            .lock()
            .map_err(|_| "Failed to lock DepotDownloader state".to_string())?;

        if guard.child.is_some() {
            return Err("DepotDownloader is already running".to_string());
        }
        guard.build_datetime_utc = None;
        guard.depot_timestamps.clear();
        guard.manifest_to_depot.clear();
        guard.manifest_timestamps.clear();
        guard.last_depot_mentioned = None;
        guard.auth_username = None;

        let job_id = generate_job_id();
        guard.job_id = Some(job_id.clone());
        job_id
    };

    emit_status(&app_handle, "starting", None, &job_id);

    let app_handle_clone = app_handle.clone();
    let state_handle = state.inner.clone();
    let job_clone = job.clone();
    let job_id_clone = job_id.clone();

    thread::spawn(move || {
        run_depotdownloader_worker(app_handle_clone, state_handle, job_clone, job_id_clone);
    });

    Ok(job_id)
}

fn run_depotdownloader_worker(
    app_handle: AppHandle,
    state_handle: Arc<Mutex<RunningJobState>>,
    job: JobMetadata,
    job_id: String,
) {
    let path = match resolve_depotdownloader_path(&app_handle) {
        Ok(path) => path,
        Err(err) => {
            emit_status(&app_handle, "error", None, &job_id);
            clear_runner_state(&state_handle, &job_id);
            eprintln!("Failed to resolve DepotDownloader path: {err}");
            return;
        }
    };

    let staging_dir = match create_staging_dir(&app_handle, &job_id) {
        Ok(dir) => dir,
        Err(err) => {
            emit_status(&app_handle, "error", None, &job_id);
            clear_runner_state(&state_handle, &job_id);
            eprintln!("Failed to create staging directory: {err}");
            return;
        }
    };

    emit_log(
        &app_handle,
        "system",
        &format!("Job ID: {}", job_id),
        &job_id,
    );
    emit_log(
        &app_handle,
        "system",
        &format!("Staging directory: {}", staging_dir.display()),
        &job_id,
    );

    if let Ok(mut guard) = state_handle.lock() {
        if guard.job_id.as_deref() == Some(&job_id) {
            let trimmed = job.username.trim();
            guard.auth_username = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };
        }
    }

    if let Some(username) = resolve_auth_username(&state_handle, &job, &job_id) {
        if let Err(err) = restore_auth_cache(&app_handle, &username, &staging_dir, &job_id) {
            emit_log(
                &app_handle,
                "system",
                &format!("Failed to restore auth cache: {}", err),
                &job_id,
            );
        }
    }

    let state_wrapper = DepotRunnerState {
        inner: state_handle.clone(),
    };

    if let Err(err) =
        run_preflight_before_download(&app_handle, &state_wrapper, &job, &job_id, &staging_dir)
    {
        emit_log(
            &app_handle,
            "system",
            &format!("Preflight failed: {err}"),
            &job_id,
        );
        emit_status(&app_handle, "error", None, &job_id);
        let _ = cleanup_staging_dir(&app_handle, &job_id);
        clear_runner_state(&state_handle, &job_id);
        return;
    }

    if let Some(username) = resolve_auth_username(&state_handle, &job, &job_id) {
        if let Err(err) = restore_auth_cache(&app_handle, &username, &staging_dir, &job_id) {
            emit_log(
                &app_handle,
                "system",
                &format!("Failed to restore auth cache: {}", err),
                &job_id,
            );
        }
    }

    if let Ok(guard) = state_handle.lock() {
        if guard.job_id.is_none() {
            let _ = cleanup_staging_dir(&app_handle, &job_id);
            return;
        }
    }

    emit_log(
        &app_handle,
        "system",
        "Starting DepotDownloader...",
        &job_id,
    );

    let args = match build_depot_args(&job) {
        Ok(args) => args,
        Err(err) => {
            emit_log(
                &app_handle,
                "system",
                &format!("Failed to build DepotDownloader args: {err}"),
                &job_id,
            );
            emit_status(&app_handle, "error", None, &job_id);
            let _ = cleanup_staging_dir(&app_handle, &job_id);
            clear_runner_state(&state_handle, &job_id);
            return;
        }
    };

    emit_log(
        &app_handle,
        "system",
        &format!("DepotDownloader args: {}", args.join(" ")),
        &job_id,
    );

    let mut command = Command::new(&path);
    command.args(&args);
    command.current_dir(&staging_dir);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::piped());

    // Hide console window on Windows
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            emit_status(&app_handle, "error", None, &job_id);
            let _ = cleanup_staging_dir(&app_handle, &job_id);
            clear_runner_state(&state_handle, &job_id);
            emit_log(
                &app_handle,
                "system",
                &format!("Failed to spawn DepotDownloader: {err}"),
                &job_id,
            );
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();

    if let Ok(mut guard) = state_handle.lock() {
        guard.child = Some(child);
        guard.stdin = stdin;
        guard.job_id = Some(job_id.clone());
    }

    emit_status(&app_handle, "running", None, &job_id);

    let stdout_handle = stdout.map(|stream| {
        spawn_log_reader(
            app_handle.clone(),
            stream,
            "stdout",
            job_id.clone(),
            state_handle.clone(),
        )
    });
    let stderr_handle = stderr.map(|stream| {
        spawn_log_reader(
            app_handle.clone(),
            stream,
            "stderr",
            job_id.clone(),
            state_handle.clone(),
        )
    });

    if let (Some(stdout_h), Some(stderr_h)) = (stdout_handle, stderr_handle) {
        if let Ok(mut guard) = state_handle.lock() {
            guard.log_reader_threads = Some((stdout_h, stderr_h));
        }
    }

    let app_handle_clone = app_handle.clone();
    let job_id_for_monitor = job_id.clone();
    let job_for_monitor = job.clone();
    let staging_dir_for_monitor = staging_dir.clone();

    thread::spawn(move || loop {
        let status = {
            let mut lock = match state_handle.lock() {
                Ok(lock) => lock,
                Err(_) => {
                    emit_status(&app_handle_clone, "error", None, &job_id_for_monitor);
                    // Clean up staging on error
                    let _ = cleanup_staging_dir(&app_handle_clone, &job_id_for_monitor);
                    return;
                }
            };

            let Some(child) = lock.child.as_mut() else {
                return;
            };

            match child.try_wait() {
                Ok(Some(status)) => {
                    lock.child = None;
                    Some(status)
                }
                Ok(None) => None,
                Err(err) => {
                    lock.child = None;
                    emit_status(&app_handle_clone, "error", None, &job_id_for_monitor);
                    eprintln!("Failed to wait on DepotDownloader: {err}");
                    // Clean up staging on error
                    let _ = cleanup_staging_dir(&app_handle_clone, &job_id_for_monitor);
                    clear_runner_state(&state_handle, &job_id_for_monitor);
                    return;
                }
            }
        };

        if let Some(status) = status {
            let exit_code = status.code();

            if exit_code == Some(0) {
                // Success: Wait for log readers to finish, then derive metadata
                emit_log(
                    &app_handle_clone,
                    "system",
                    "Waiting for log processing to complete...",
                    &job_id_for_monitor,
                );

                // CRITICAL: Wait for log reader threads to finish before deriving metadata
                // This ensures all timestamps are captured before we look them up
                if let Ok(mut guard) = state_handle.lock() {
                    if let Some((stdout_h, stderr_h)) = guard.log_reader_threads.take() {
                        drop(guard); // Release lock before joining
                        let _ = stdout_h.join();
                        let _ = stderr_h.join();
                    }
                }

                if let Some(username) =
                    resolve_auth_username(&state_handle, &job_for_monitor, &job_id_for_monitor)
                {
                    if let Err(err) = persist_auth_cache(
                        &app_handle_clone,
                        &username,
                        &staging_dir_for_monitor,
                        &job_id_for_monitor,
                    ) {
                        emit_log(
                            &app_handle_clone,
                            "system",
                            &format!("Failed to persist auth cache: {}", err),
                            &job_id_for_monitor,
                        );
                    }
                }

                emit_log(
                    &app_handle_clone,
                    "system",
                    "Deriving metadata from download output...",
                    &job_id_for_monitor,
                );

                if let Err(err) = derive_metadata_from_download(
                    &app_handle_clone,
                    &job_for_monitor,
                    &job_id_for_monitor,
                    &staging_dir_for_monitor,
                ) {
                    emit_log(
                        &app_handle_clone,
                        "system",
                        &format!("Failed to derive metadata: {}", err),
                        &job_id_for_monitor,
                    );
                    emit_status(&app_handle_clone, "error", None, &job_id_for_monitor);
                    let _ = cleanup_staging_dir(&app_handle_clone, &job_id_for_monitor);
                    clear_runner_state(&state_handle, &job_id_for_monitor);
                    return;
                }

                emit_log(
                    &app_handle_clone,
                    "system",
                    "Download completed successfully. Finalizing output...",
                    &job_id_for_monitor,
                );
                emit_status(&app_handle_clone, "finalizing", None, &job_id_for_monitor);

                let compression_enabled = !job_for_monitor.skip_compression;
                match finalize_job(
                    &app_handle_clone,
                    &job_id_for_monitor,
                    compression_enabled,
                ) {
                    Ok(output_path) => {
                        emit_log(
                            &app_handle_clone,
                            "system",
                            &format!("Finalization complete. Output: {}", output_path.display()),
                            &job_id_for_monitor,
                        );
                        if let Ok(metadata) =
                            JobMetadataFile::read_from_dir(&staging_dir_for_monitor)
                        {
                            let template_metadata = TemplateMetadata::from_job_metadata(&metadata);
                            app_handle_clone
                                .state::<TemplateMetadataState>()
                                .set(template_metadata);
                        }

                        // === COMPRESSION PHASE ===
                        let mut final_output_path = output_path.clone();
                        if job_for_monitor.skip_compression {
                            emit_log(
                                &app_handle_clone,
                                "system",
                                "Compression skipped (disabled in settings).",
                                &job_id_for_monitor,
                            );
                        } else {
                            emit_status(&app_handle_clone, "compressing", None, &job_id_for_monitor);
                            emit_log(
                                &app_handle_clone,
                                "system",
                                "Starting compression with 7-Zip...",
                                &job_id_for_monitor,
                            );

                            let compression_password =
                                if job_for_monitor.compression_password_enabled {
                                    let password =
                                        job_for_monitor.compression_password.as_str();
                                    if password.trim().is_empty() {
                                        None
                                    } else {
                                        Some(password)
                                    }
                                } else {
                                    None
                                };

                            match compress_output(
                                &app_handle_clone,
                                &output_path,
                                &job_id_for_monitor,
                                compression_password,
                            ) {
                                Ok(archive_path) => {
                                    emit_log(
                                        &app_handle_clone,
                                        "system",
                                        &format!("Compression complete: {}", archive_path.display()),
                                        &job_id_for_monitor,
                                    );
                                    final_output_path = archive_path;
                                }
                                Err(err) => {
                                    emit_log(
                                        &app_handle_clone,
                                        "system",
                                        &format!("Compression failed: {}. Uncompressed output available.", err),
                                        &job_id_for_monitor,
                                    );
                                }
                            }
                        }
                        // === END COMPRESSION ===

                        // === TEMPLATE GENERATION ===
                        // Generate template text file with job metadata
                        if let Some(template_metadata) = app_handle_clone
                            .state::<TemplateMetadataState>()
                            .get()
                        {
                            emit_log(
                                &app_handle_clone,
                                "system",
                                "Generating template file...",
                                &job_id_for_monitor,
                            );

                            // Load user's template (or use default)
                            let template_blocks = load_template_data_internal(&app_handle_clone)
                                .map(|payload| payload.blocks);

                            let template_blocks_ref = template_blocks.as_ref().map(|v| v.as_slice());

                            match write_template_file(&final_output_path, &template_metadata, template_blocks_ref) {
                                Ok(()) => {
                                    emit_log(
                                        &app_handle_clone,
                                        "system",
                                        "Template file generated successfully.",
                                        &job_id_for_monitor,
                                    );
                                }
                                Err(err) => {
                                    emit_log(
                                        &app_handle_clone,
                                        "system",
                                        &format!("Failed to generate template file: {}", err),
                                        &job_id_for_monitor,
                                    );
                                }
                            }
                        }
                        // === END TEMPLATE GENERATION ===

                        emit_status(&app_handle_clone, "completed", Some(0), &job_id_for_monitor);
                        // Cleanup staging after successful finalization
                        let _ = cleanup_staging_dir(&app_handle_clone, &job_id_for_monitor);
                    }
                    Err(err) => {
                        emit_log(
                            &app_handle_clone,
                            "system",
                            &format!("Finalization failed: {}", err),
                            &job_id_for_monitor,
                        );
                        emit_status(&app_handle_clone, "finalization_failed", None, &job_id_for_monitor);
                        // Cleanup staging on finalization failure
                        let _ = cleanup_staging_dir(&app_handle_clone, &job_id_for_monitor);
                    }
                }
            } else {
                // Failure: cleanup staging
                emit_status(&app_handle_clone, "exited", exit_code, &job_id_for_monitor);
                emit_log(
                    &app_handle_clone,
                    "system",
                    "Job failed. Cleaning up staging directory.",
                    &job_id_for_monitor,
                );
                if let Some(username) =
                    resolve_auth_username(&state_handle, &job_for_monitor, &job_id_for_monitor)
                {
                    if let Err(err) = persist_auth_cache(
                        &app_handle_clone,
                        &username,
                        &staging_dir_for_monitor,
                        &job_id_for_monitor,
                    ) {
                        emit_log(
                            &app_handle_clone,
                            "system",
                            &format!("Failed to persist auth cache: {}", err),
                            &job_id_for_monitor,
                        );
                    }
                }
                let _ = cleanup_staging_dir(&app_handle_clone, &job_id_for_monitor);
            }

            clear_runner_state(&state_handle, &job_id_for_monitor);
            return;
        }

        thread::sleep(Duration::from_millis(100));
    });
}

#[tauri::command]
pub fn cancel_depotdownloader(
    app_handle: AppHandle,
    state: State<'_, DepotRunnerState>,
) -> Result<(), String> {
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| "Failed to lock DepotDownloader state".to_string())?;

    // Clone job_id before getting mutable reference to child (borrow checker)
    let job_id = guard
        .job_id
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    let Some(child) = guard.child.as_mut() else {
        return Err("DepotDownloader is not running".to_string());
    };

    child
        .kill()
        .map_err(|err| format!("Failed to terminate DepotDownloader: {err}"))?;

    let status = child
        .wait()
        .map_err(|err| format!("Failed to await DepotDownloader shutdown: {err}"))?;

    guard.child = None;
    guard.stdin = None;
    guard.job_id = None;
    guard.build_datetime_utc = None;
    guard.depot_timestamps.clear();
    guard.manifest_to_depot.clear();
    guard.manifest_timestamps.clear();
    guard.last_depot_mentioned = None;
    guard.auth_username = None;

    emit_status(&app_handle, "exited", status.code(), &job_id);

    // Clean up staging directory on cancellation
    emit_log(
        &app_handle,
        "system",
        "Job cancelled. Cleaning up staging directory.",
        &job_id,
    );
    let _ = cleanup_staging_dir(&app_handle, &job_id);

    Ok(())
}

#[tauri::command]
pub fn submit_steam_guard_code(
    code: String,
    state: State<'_, DepotRunnerState>,
) -> Result<(), String> {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return Err("Steam Guard code is empty".to_string());
    }

    let mut guard = state
        .inner
        .lock()
        .map_err(|_| "Failed to lock DepotDownloader state".to_string())?;

    if guard.child.is_none() {
        return Err("DepotDownloader is not running".to_string());
    }

    let Some(stdin) = guard.stdin.as_mut() else {
        return Err("DepotDownloader stdin is unavailable".to_string());
    };

    stdin
        .write_all(trimmed.as_bytes())
        .map_err(|err| format!("Failed to write Steam Guard code: {err}"))?;
    stdin
        .write_all(b"\n")
        .map_err(|err| format!("Failed to submit Steam Guard code: {err}"))?;
    stdin
        .flush()
        .map_err(|err| format!("Failed to flush Steam Guard code: {err}"))?;

    Ok(())
}

fn emit_status(app_handle: &AppHandle, status: &str, code: Option<i32>, job_id: &str) {
    let _ = app_handle.emit(
        "dd:status",
        StatusPayload {
            status: status.to_string(),
            code,
            job_id: job_id.to_string(),
        },
    );
}

fn emit_log(app_handle: &AppHandle, stream: &str, line: &str, job_id: &str) {
    let _ = app_handle.emit(
        "dd:log",
        LogPayload {
            stream: stream.to_string(),
            line: line.to_string(),
            job_id: job_id.to_string(),
        },
    );

    let debug_state = app_handle.state::<DebugConsoleState>();
    if debug_state.enabled() {
        debug_state.write_line(&format!("[{stream}] {line}"));
    }
}

fn clear_runner_state(state_handle: &Arc<Mutex<RunningJobState>>, job_id: &str) {
    if let Ok(mut guard) = state_handle.lock() {
        if guard.job_id.as_deref() == Some(job_id) {
            guard.job_id = None;
            guard.stdin = None;
            guard.build_datetime_utc = None;
            guard.depot_timestamps.clear();
            guard.manifest_to_depot.clear();
            guard.last_depot_mentioned = None;
            guard.auth_username = None;
        }
    }
}

fn run_preflight_before_download(
    app_handle: &AppHandle,
    state: &DepotRunnerState,
    job: &JobMetadata,
    job_id: &str,
    staging_dir: &PathBuf,
) -> Result<(), String> {
    if job.qr_enabled {
        // For QR auth, preflight can't run before download - SteamDB API will be used instead
        return Ok(());
    }

    use std::fs;

    let preflight_dir = staging_dir.join(".preflight");
    if let Err(err) = fs::create_dir_all(&preflight_dir) {
        emit_log(
            app_handle,
            "system",
            &format!("Preflight skipped: {}", err),
            job_id,
        );
        return Ok(());
    }

    if !job.username.trim().is_empty() {
        if let Err(err) =
            restore_auth_cache(app_handle, &job.username, &preflight_dir, job_id)
        {
            emit_log(
                app_handle,
                "system",
                &format!("Failed to restore auth cache: {}", err),
                job_id,
            );
        }
    }

    let dd_path = resolve_depotdownloader_path(app_handle)?;
    let mut args = build_preflight_args(job)?;
    args.push("-manifest-only".to_string());

    emit_log(
        app_handle,
        "system",
        "Running preflight to resolve depot metadata...",
        job_id,
    );

    let mut command = Command::new(&dd_path);
    command.args(&args);
    command.current_dir(&preflight_dir);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::piped());

    // Hide console window on Windows
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = command
        .spawn()
        .map_err(|err| format!("Failed to spawn DepotDownloader preflight: {err}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();

    {
        let mut guard = state
            .inner
            .lock()
            .map_err(|_| "Failed to lock DepotDownloader state".to_string())?;
        guard.child = Some(child);
        guard.stdin = stdin;
        guard.job_id = Some(job_id.to_string());
    }

    let output_lines: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let stdout_handle = stdout.map(|stream| {
        spawn_preflight_reader(
            app_handle.clone(),
            stream,
            "stdout",
            job_id.to_string(),
            output_lines.clone(),
        )
    });

    let stderr_handle = stderr.map(|stream| {
        spawn_preflight_reader(
            app_handle.clone(),
            stream,
            "stderr",
            job_id.to_string(),
            output_lines.clone(),
        )
    });

    let status = loop {
        let status = {
            let mut guard = state
                .inner
                .lock()
                .map_err(|_| "Failed to lock DepotDownloader state".to_string())?;

            if guard.job_id.as_deref() != Some(job_id) {
                let _ = fs::remove_dir_all(&preflight_dir);
                return Ok(());
            }

            let Some(child) = guard.child.as_mut() else {
                let _ = fs::remove_dir_all(&preflight_dir);
                return Ok(());
            };

            match child.try_wait() {
                Ok(Some(status)) => {
                    guard.child = None;
                    guard.stdin = None;
                    Some(status)
                }
                Ok(None) => None,
                Err(err) => {
                    guard.child = None;
                    guard.stdin = None;
                    emit_log(
                        app_handle,
                        "system",
                        &format!("Preflight failed to wait: {err}"),
                        job_id,
                    );
                    let _ = fs::remove_dir_all(&preflight_dir);
                    return Ok(());
                }
            }
        };

        if let Some(status) = status {
            break status;
        }

        thread::sleep(Duration::from_millis(100));
    };

    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }
    if let Some(handle) = stderr_handle {
        let _ = handle.join();
    }

    let lines = output_lines
        .lock()
        .map_err(|_| "Failed to lock preflight output".to_string())?
        .clone();

    let parsed = parse_preflight_output(&lines);

    if !status.success() && parsed.depots.is_empty() {
        emit_log(
            app_handle,
            "system",
            &format!(
                "Preflight failed with exit code {:?}. Continuing without preflight.",
                status.code()
            ),
            job_id,
        );
        let _ = fs::remove_dir_all(&preflight_dir);
        return Ok(());
    }

    if let Ok(mut guard) = state.inner.lock() {
        if guard.job_id.as_deref() == Some(job_id) {
            if let Some(timestamp) = parsed.build_datetime_utc {
                if guard.build_datetime_utc.is_none() {
                    guard.build_datetime_utc = Some(timestamp);
                }
            }

            for depot in parsed.depots {
                if let Some(name) = depot.depot_name {
                    guard.depot_names.insert(depot.depot_id, name);
                }
            }
        }
    }

    if !job.username.trim().is_empty() {
        if let Err(err) =
            persist_auth_cache(app_handle, &job.username, &preflight_dir, job_id)
        {
            emit_log(
                app_handle,
                "system",
                &format!("Failed to persist auth cache: {}", err),
                job_id,
            );
        }
    }

    let _ = fs::remove_dir_all(&preflight_dir);
    Ok(())
}

fn spawn_log_reader(
    app_handle: AppHandle,
    stream: impl std::io::Read + Send + 'static,
    tag: &str,
    job_id: String,
    state_handle: Arc<Mutex<RunningJobState>>,
) -> thread::JoinHandle<()> {
    let stream_name = tag.to_string();
    const EMAIL_PROMPT: &str =
        "STEAM GUARD! Please enter the auth code sent to the email at";

    thread::spawn(move || {
        use std::io::BufReader;

        let mut reader = BufReader::new(stream);
        let mut buffer = [0u8; 1024];
        let mut pending: Vec<u8> = Vec::new();
        let mut prompt_emitted = false;

        loop {
            let n = match reader.read(&mut buffer) {
                Ok(n) => n,
                Err(_) => break,
            };

            if n == 0 {
                break; // EOF
            }

            pending.extend_from_slice(&buffer[..n]);

            while let Some(pos) = pending.iter().position(|&byte| byte == b'\n') {
                let mut line_bytes: Vec<u8> = pending.drain(..=pos).collect();
                if let Some(b'\n') = line_bytes.last() {
                    line_bytes.pop();
                }
                if let Some(b'\r') = line_bytes.last() {
                    line_bytes.pop();
                }
                let line = decode_stream_bytes(&line_bytes);
                emit_log(&app_handle, &stream_name, &line, &job_id);
                maybe_update_auth_username(&state_handle, &line, &job_id);
                maybe_store_build_datetime(&app_handle, &line, &job_id);
            }

            if !prompt_emitted
                && pending
                    .windows(EMAIL_PROMPT.len())
                    .any(|window| window == EMAIL_PROMPT.as_bytes())
            {
                let mut line_bytes = std::mem::take(&mut pending);
                if let Some(b'\r') = line_bytes.last() {
                    line_bytes.pop();
                }
                let line = decode_stream_bytes(&line_bytes);
                emit_log(&app_handle, &stream_name, &line, &job_id);
                maybe_update_auth_username(&state_handle, &line, &job_id);
                prompt_emitted = true;
            }
        }

        if !pending.is_empty() {
            let mut line_bytes = std::mem::take(&mut pending);
            if let Some(b'\r') = line_bytes.last() {
                line_bytes.pop();
            }
            let line = decode_stream_bytes(&line_bytes);
            emit_log(&app_handle, &stream_name, &line, &job_id);
            maybe_update_auth_username(&state_handle, &line, &job_id);
        }
    })
}

fn spawn_preflight_reader(
    app_handle: AppHandle,
    stream: impl std::io::Read + Send + 'static,
    tag: &str,
    job_id: String,
    output: Arc<Mutex<Vec<String>>>,
) -> thread::JoinHandle<()> {
    let stream_name = tag.to_string();
    const EMAIL_PROMPT: &str =
        "STEAM GUARD! Please enter the auth code sent to the email at";

    thread::spawn(move || {
        use std::io::BufReader;

        let mut reader = BufReader::new(stream);
        let mut buffer = [0u8; 1024];
        let mut pending: Vec<u8> = Vec::new();
        let mut prompt_emitted = false;

        loop {
            let n = match reader.read(&mut buffer) {
                Ok(n) => n,
                Err(_) => break,
            };

            if n == 0 {
                break; // EOF
            }

            pending.extend_from_slice(&buffer[..n]);

            while let Some(pos) = pending.iter().position(|&byte| byte == b'\n') {
                let mut line_bytes: Vec<u8> = pending.drain(..=pos).collect();
                if let Some(b'\n') = line_bytes.last() {
                    line_bytes.pop();
                }
                if let Some(b'\r') = line_bytes.last() {
                    line_bytes.pop();
                }
                let line = decode_stream_bytes(&line_bytes);
                if let Ok(mut guard) = output.lock() {
                    guard.push(line.clone());
                }
                emit_log(&app_handle, &stream_name, &line, &job_id);
            }

            if !prompt_emitted
                && pending
                    .windows(EMAIL_PROMPT.len())
                    .any(|window| window == EMAIL_PROMPT.as_bytes())
            {
                let mut line_bytes = std::mem::take(&mut pending);
                if let Some(b'\r') = line_bytes.last() {
                    line_bytes.pop();
                }
                let line = decode_stream_bytes(&line_bytes);
                if let Ok(mut guard) = output.lock() {
                    guard.push(line.clone());
                }
                emit_log(&app_handle, &stream_name, &line, &job_id);
                prompt_emitted = true;
            }
        }

        if !pending.is_empty() {
            let mut line_bytes = std::mem::take(&mut pending);
            if let Some(b'\r') = line_bytes.last() {
                line_bytes.pop();
            }
            let line = decode_stream_bytes(&line_bytes);
            if let Ok(mut guard) = output.lock() {
                guard.push(line.clone());
            }
            emit_log(&app_handle, &stream_name, &line, &job_id);
        }
    })
}

fn decode_stream_bytes(bytes: &[u8]) -> String {
    #[cfg(windows)]
    {
        decode_console_bytes(bytes)
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes).to_string()
    }
}

fn maybe_store_build_datetime(app_handle: &AppHandle, line: &str, job_id: &str) {
    static DEPOT_MANIFEST_RE: OnceLock<Regex> = OnceLock::new();
    static DEPOT_RE: OnceLock<Regex> = OnceLock::new();
    static DEPOT_NAME_RE: OnceLock<Regex> = OnceLock::new();
    static APPINFO_NAME_RE: OnceLock<Regex> = OnceLock::new();
    static MANIFEST_CREATIONTIME_RE: OnceLock<Regex> = OnceLock::new();

    let depot_manifest = DEPOT_MANIFEST_RE.get_or_init(|| {
        Regex::new(r"[Dd]epot\s+(\d+)\s*[-–]\s*[Mm]anifest\s+(\d+)").unwrap()
    });
    let depot = DEPOT_RE.get_or_init(|| {
        Regex::new(r"[Dd]epot\s+(\d+)").unwrap()
    });
    let depot_name = DEPOT_NAME_RE.get_or_init(|| {
        Regex::new(r#"[Dd]epot\s+(\d+)\s+"([^"]+)""#).unwrap()
    });
    let appinfo_name = APPINFO_NAME_RE.get_or_init(|| {
        Regex::new(r#""name"\s+"([^"]+)""#).unwrap()
    });
    let manifest_creationtime = MANIFEST_CREATIONTIME_RE.get_or_init(|| {
        Regex::new(r"(?i)Manifest\s+(\d+)\s+\((.+?)\)").unwrap()
    });

    if let Ok(mut guard) = app_handle.state::<DepotRunnerState>().inner.lock() {
        if guard.job_id.as_deref() != Some(job_id) {
            return;
        }

        // Track depot names: Depot 12345 "Depot Name"
        if let Some(caps) = depot_name.captures(line) {
            if let (Some(depot_id), Some(name)) = (
                caps.get(1).map(|m| m.as_str().to_string()),
                caps.get(2).map(|m| m.as_str().to_string()),
            ) {
                eprintln!("[DOWNLOAD] Found depot name: {} -> {}", depot_id, name);
                guard.depot_names.insert(depot_id.clone(), name);
                guard.last_depot_mentioned = Some(depot_id);
                return;
            }
        }

        // Track depot-manifest pairs
        if let Some(caps) = depot_manifest.captures(line) {
            if let (Some(depot_id), Some(manifest_id)) = (
                caps.get(1).map(|m| m.as_str().to_string()),
                caps.get(2).map(|m| m.as_str().to_string()),
            ) {
                guard.manifest_to_depot.insert(manifest_id, depot_id.clone());
                guard.last_depot_mentioned = Some(depot_id);
                return;
            }
        }

        // Track appinfo name field (when in depot context)
        if let Some(caps) = appinfo_name.captures(line) {
            if let Some(depot_id) = guard.last_depot_mentioned.clone() {
                let name = caps.get(1).map(|m| m.as_str().to_string()).unwrap();
                eprintln!("[DOWNLOAD] Found depot name (appinfo): {} -> {}", depot_id, name);
                guard.depot_names.insert(depot_id, name);
            }
        }

        // Track standalone depot mentions
        if let Some(caps) = depot.captures(line) {
            if let Some(depot_id) = caps.get(1).map(|m| m.as_str().to_string()) {
                guard.last_depot_mentioned = Some(depot_id);
            }
        }

        // Check for timeupdated (build release date) - captures for fallback if SteamDB fails
        static TIMEUPDATED_ONLY_RE: OnceLock<Regex> = OnceLock::new();
        let timeupdated_only = TIMEUPDATED_ONLY_RE.get_or_init(|| {
            Regex::new(r"(?i)timeupdated[^0-9]*(\d{9,})").unwrap()
        });
        if let Some(caps) = timeupdated_only.captures(line) {
            if let Some(epoch_str) = caps.get(1).map(|m| m.as_str()) {
                if let Some(timestamp) = parse_epoch_timestamp(Some(epoch_str)) {
                    guard.build_datetime_utc = Some(timestamp);
                    return;
                }
            }
        }

        // Capture manifest timestamps (for per-depot tracking as fallback)
        if let Some(caps) = manifest_creationtime.captures(line) {
            if let (Some(manifest_id), Some(datetime_str)) = (
                caps.get(1).map(|m| m.as_str().to_string()),
                caps.get(2).map(|m| m.as_str()),
            ) {
                if let Some(timestamp) = parse_datetime_string(datetime_str) {
                    guard.manifest_timestamps.insert(manifest_id.clone(), timestamp);

                    if let Some(depot_id) = guard.manifest_to_depot.get(&manifest_id).cloned() {
                        guard.depot_timestamps.insert(depot_id, timestamp);
                    }
                }
            }
        }

        // FALLBACK: If no timeupdated was found yet, try other app-level timestamp patterns
        // (but NOT manifest creation times - those are only for per-depot tracking)
        if guard.build_datetime_utc.is_none() {
            // Check for lastupdated or builddate patterns
            static LASTUPDATED_ONLY_RE: OnceLock<Regex> = OnceLock::new();
            static BUILDDATE_ONLY_RE: OnceLock<Regex> = OnceLock::new();
            let lastupdated_only = LASTUPDATED_ONLY_RE.get_or_init(|| {
                Regex::new(r"(?i)last\s*updated[^0-9]*(\d{9,})").unwrap()
            });
            let builddate_only = BUILDDATE_ONLY_RE.get_or_init(|| {
                Regex::new(r"(?i)build(?:_|\s)*date[^0-9]*(\d{9,})").unwrap()
            });

            if let Some(caps) = lastupdated_only.captures(line) {
                if let Some(timestamp) = parse_epoch_timestamp(caps.get(1).map(|m| m.as_str())) {
                    guard.build_datetime_utc = Some(timestamp);
                }
            } else if let Some(caps) = builddate_only.captures(line) {
                if let Some(timestamp) = parse_epoch_timestamp(caps.get(1).map(|m| m.as_str())) {
                    guard.build_datetime_utc = Some(timestamp);
                }
            }
        }
    }
}

/// Parse a datetime string in various formats (.NET DateTime, ISO, etc.)
fn parse_datetime_string(datetime_str: &str) -> Option<DateTime<Utc>> {
    static ISO_RE: OnceLock<Regex> = OnceLock::new();
    static DOTNET_DATETIME_RE: OnceLock<Regex> = OnceLock::new();

    let iso = ISO_RE.get_or_init(|| {
        Regex::new(r"(?i)(\d{4}-\d{2}-\d{2})[ T](\d{2}:\d{2}:\d{2})(?:\s*UTC|Z)?").unwrap()
    });
    let dotnet_datetime = DOTNET_DATETIME_RE.get_or_init(|| {
        Regex::new(r"(\d{1,2})/(\d{1,2})/(\d{4})\s+(\d{1,2}):(\d{2}):(\d{2})(?:\s*([AP]M))?").unwrap()
    });

    // Try .NET DateTime format first (most common)
    if let Some(parsed) = parse_dotnet_datetime_str(datetime_str, dotnet_datetime) {
        return Some(parsed);
    }

    // Try ISO format
    if let Some(iso_caps) = iso.captures(datetime_str) {
        if let Some(parsed) = parse_iso_datetime(
            iso_caps.get(1).map(|m| m.as_str()),
            iso_caps.get(2).map(|m| m.as_str()),
        ) {
            return Some(parsed);
        }
    }

    None
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

fn parse_dotnet_datetime_str(text: &str, pattern: &Regex) -> Option<DateTime<Utc>> {
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

#[cfg(windows)]
fn decode_console_bytes(buf: &[u8]) -> String {
    use codepage_strings::Coding;
    use windows_sys::Win32::{Globalization::GetOEMCP, System::Console::GetConsoleOutputCP};

    let mut candidates = Vec::new();
    let output_cp = unsafe { GetConsoleOutputCP() } as u16;
    if output_cp != 0 {
        candidates.push(output_cp);
    }

    let oem_cp = unsafe { GetOEMCP() } as u16;
    if oem_cp != 0 && !candidates.contains(&oem_cp) {
        candidates.push(oem_cp);
    }

    for fallback_cp in [437u16, 850u16, 1252u16] {
        if !candidates.contains(&fallback_cp) {
            candidates.push(fallback_cp);
        }
    }

    for codepage in candidates {
        if let Ok(coding) = Coding::new(codepage) {
            return coding.decode_lossy(buf).into_owned();
        }
    }

    String::from_utf8_lossy(buf).into_owned()
}

#[cfg(unix)]
fn is_executable(path: &PathBuf) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(path: &PathBuf) -> bool {
    path.is_file()
}
