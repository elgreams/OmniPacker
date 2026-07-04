use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::checksum::calculate_checksums;
use crate::debug_console::{debug_eprintln, DebugConsoleState};
use crate::debug_log::{debug_log, DebugLog};
use crate::job_finalization::{finalize_job, resolve_archive_path};
use crate::job_metadata::{BuildIdSource, DepotInfo, JobMetadataFile};
use crate::job_staging::{cleanup_staging_dir, create_staging_dir, generate_job_id};
use crate::manifest_preflight::{build_preflight_args, parse_preflight_output};
use crate::steam_api::fetch_app_info;
use crate::steamdb_api::fetch_build_date;
use crate::template_metadata::{TemplateMetadata, TemplateMetadataState};
use crate::template_renderer::{write_template_files, TemplateProfile};
use crate::zip_runner::{
    calculate_7z_compression_args, filter_custom_args, run_7zip_blocking, SevenZipOutcome,
    SevenZipRunnerState,
};

/// Metadata for a download job, received from the frontend
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobMetadata {
    pub app_id: String,
    pub os: String,
    pub branch: String,
    /// Password for a password-protected beta branch. Forwarded to
    /// DepotDownloader as `-branchpassword` only when a non-public branch is set.
    #[serde(default)]
    pub branch_password: String,
    pub username: String,
    pub password: String,
    pub qr_enabled: bool,
    /// Set by the frontend on the final queue job when the user did not opt into
    /// saving credentials. Forwards `-clear-token` so DepotDownloader wipes the
    /// stored login token from disk after the queue completes.
    #[serde(default)]
    pub clear_token: bool,
    #[serde(default)]
    pub skip_compression: bool,
    #[serde(default)]
    pub compression_password_enabled: bool,
    #[serde(default)]
    pub compression_password: String,
    #[serde(default)]
    pub custom_compression_args: String,
    /// 7-Zip volume size token (e.g. "100m", "4g") when the user enabled archive
    /// splitting. Empty means no split. Forwarded as `-v<size>`, which makes
    /// 7-Zip emit `archive.7z.001`, `.002`, … instead of a single file.
    #[serde(default)]
    pub split_volume_size: String,
    /// Global uploader handle (from the "Uploader name" setting). Injected into
    /// the `{{username}}` token for any profile that uses it. Empty when unset.
    #[serde(default)]
    pub uploader_name: String,
    /// Global upload date (from the "Upload date" setting), resolved by the
    /// frontend to either manual text or today's date. Injected into the
    /// `{{upload_date}}` token for any profile that uses it. Empty when unset.
    #[serde(default)]
    pub upload_date: String,
    /// Template profiles selected for generation, resolved by the frontend
    /// (built-in `standard`/`crew` plus any saved custom ones). Each yields its
    /// own `.txt` next to the output. Empty falls back to the default template.
    #[serde(default)]
    pub template_profiles: Vec<TemplateProfile>,
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
    // Track depot names from preflight (depot_id -> depot_name)
    depot_names: std::collections::HashMap<String, String>,
    // Track depot dlcappids from DepotDownloader output (depot_id -> dlcappid)
    depot_dlcappids: std::collections::HashMap<String, String>,
    // Join handles for log reader threads (to ensure all logs are parsed before metadata derivation)
    log_reader_threads: Option<(thread::JoinHandle<()>, thread::JoinHandle<()>)>,
    // Download-progress tracking. DepotDownloader announces every depot up front
    // with "Downloading depot <id> manifest" (phase 1), then downloads each in
    // turn printing per-file "NN.NN%" lines and a "Depot <id> - Downloaded ..."
    // completion line (phase 2). We count announced depots as the denominator,
    // completed depots + the current depot's percent as the numerator, to drive
    // a whole-job progress bar. See progress::DownloadProgress.
    progress: DownloadProgress,
}

/// Whole-job download progress derived from DepotDownloader stdout.
#[derive(Default)]
struct DownloadProgress {
    /// Distinct depot IDs announced via "Downloading depot <id> manifest".
    announced_depots: std::collections::HashSet<String>,
    /// Number of depots that printed their "Depot <id> - Downloaded ..." line.
    completed_depots: usize,
    /// Percent (0..=100) of the depot currently downloading.
    current_depot_percent: u8,
    /// Last whole-job percent emitted, to avoid spamming identical events.
    last_emitted_percent: Option<u8>,
}

impl DownloadProgress {
    fn reset(&mut self) {
        self.announced_depots.clear();
        self.completed_depots = 0;
        self.current_depot_percent = 0;
        self.last_emitted_percent = None;
    }

    /// Whole-job percent: completed depots count as 100 each, plus the in-flight
    /// depot's percent, divided by the total announced. Returns None until at
    /// least one depot has been announced (denominator unknown).
    fn job_percent(&self) -> Option<u8> {
        let total = self.announced_depots.len();
        if total == 0 {
            return None;
        }
        let numerator = self.completed_depots * 100 + self.current_depot_percent as usize;
        let pct = (numerator / total).min(100);
        Some(pct as u8)
    }

    /// Applies a single stdout line to the progress state. Returns true when the
    /// state changed (so the caller may emit), and a second flag that is true
    /// when the line marked a depot complete (so the caller always emits to
    /// advance the N/M counter even if the percent is unchanged). Pure: holds no
    /// AppHandle, so it is unit-testable.
    fn apply_line(&mut self, line: &str) -> (bool, bool) {
        static MANIFEST_ANNOUNCE_RE: OnceLock<Regex> = OnceLock::new();
        static DEPOT_DONE_RE: OnceLock<Regex> = OnceLock::new();
        static PERCENT_RE: OnceLock<Regex> = OnceLock::new();

        let manifest_announce = MANIFEST_ANNOUNCE_RE.get_or_init(|| {
            Regex::new(r"(?i)Downloading\s+depot\s+(\d+)\s+manifest").unwrap()
        });
        let depot_done = DEPOT_DONE_RE.get_or_init(|| {
            Regex::new(r"(?i)Depot\s+(\d+)\s*[-–]\s*Downloaded\s+\d+\s+bytes").unwrap()
        });
        // Leading, optionally space-padded "NN.NN%" at the start of the line.
        let percent = PERCENT_RE
            .get_or_init(|| Regex::new(r"^\s*(\d{1,3})(?:\.\d+)?%").unwrap());

        if let Some(caps) = manifest_announce.captures(line) {
            // Phase 1: a depot was announced. Counts toward the denominator.
            let depot_id = caps.get(1).unwrap().as_str().to_string();
            let inserted = self.announced_depots.insert(depot_id);
            (inserted, false)
        } else if depot_done.captures(line).is_some() {
            // A depot finished. Bump completed and reset the in-flight percent so
            // the next depot starts its bar segment from 0.
            let total = self.announced_depots.len().max(1);
            if self.completed_depots < total {
                self.completed_depots += 1;
            }
            self.current_depot_percent = 0;
            (true, true)
        } else if let Some(caps) = percent.captures(line) {
            // Phase 2: per-file percent within the current depot.
            if let Ok(p) = caps.get(1).unwrap().as_str().parse::<u8>() {
                let p = p.min(100);
                if p != self.current_depot_percent {
                    self.current_depot_percent = p;
                    return (true, false);
                }
            }
            (false, false)
        } else {
            (false, false)
        }
    }
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
                depot_names: std::collections::HashMap::new(),
                depot_dlcappids: std::collections::HashMap::new(),
                log_reader_threads: None,
                progress: DownloadProgress::default(),
            })),
        }
    }
}

impl DepotRunnerState {
    /// Kills the running DepotDownloader child (if any) and clears it from state.
    /// Safe to call when no child is running.
    pub fn kill_child(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            if let Some(ref mut child) = guard.child {
                let _ = child.kill();
                let _ = child.wait();
            }
            guard.child = None;
        }
    }
}

impl Drop for DepotRunnerState {
    fn drop(&mut self) {
        self.kill_child();
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressPayload {
    /// Whole-job download percent (0..=100).
    job_percent: u8,
    /// Depots finished so far.
    completed_depots: usize,
    /// Total depots announced for this job.
    total_depots: usize,
    /// Percent of the depot currently downloading (0..=100).
    depot_percent: u8,
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

/// Strip the Windows extended-length path prefix (`\\?\`) that Tauri's path
/// resolver adds.  .NET executables crash when launched via `\\?\` paths
/// (CLR exception 0xE0434352), so we must use plain paths for sidecars.
pub(crate) fn strip_extended_length_prefix(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path
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

    // Strip \\?\ prefix that breaks .NET CLR startup
    let sidecar_path = strip_extended_length_prefix(sidecar_path);

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
        "Linux x64" => ("linux", "64"),
        "Linux x86" => ("linux", "32"),
        "Linux" => ("linux", "64"),
        "macOS x64" => ("macos", "64"),
        "macOS" => ("macos", "64"),
        _ => ("windows", "64"),
    }
}

/// Reads the real manifest ID for a depot from the on-disk manifest file.
///
/// DepotDownloader writes `{depot_id}_{manifest_id}.manifest` into the
/// `.DepotDownloader/` subdirectory of each depot's manifest directory. The
/// manifest directory itself is named after the build ID, so the filename is
/// the only on-disk source of the true manifest ID. Returns `None` if no
/// matching manifest file is found.
fn read_manifest_id_from_disk(manifest_dir: &std::path::Path, depot_id: &str) -> Option<String> {
    let dd_dir = manifest_dir.join(".DepotDownloader");
    let prefix = format!("{depot_id}_");

    for entry in std::fs::read_dir(&dd_dir).ok()?.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if let Some(stem) = name.strip_suffix(".manifest") {
            if let Some(manifest_id) = stem.strip_prefix(&prefix) {
                return Some(manifest_id.to_string());
            }
        }
    }

    None
}

/// Derives metadata from downloaded content (for QR auth case where preflight was skipped)
fn derive_metadata_from_download(
    app_handle: &AppHandle,
    job: &JobMetadata,
    job_id: &str,
    staging_dir: &std::path::Path,
) -> Result<(), String> {
    use std::fs;

    // Fetch game name (and store metadata for the crew template preset) from
    // the Steam API. Description/website are best-effort: an API failure still
    // yields a usable job via the app_<id> name fallback.
    let (game_name, game_description, website) = match fetch_app_info(&job.app_id) {
        Ok(info) => (info.name, info.short_description, info.website),
        Err(_) => (format!("app_{}", job.app_id), String::new(), None),
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
            // The manifest subdirectory is named after the build ID, not the
            // manifest ID. Use it as the build ID...
            let manifest_dir = manifest_entry.path();
            if build_id.is_empty() {
                build_id = manifest_entry.file_name().to_string_lossy().to_string();
            }

            // ...but read the real per-depot manifest ID from the on-disk
            // {depot_id}_{manifest_id}.manifest file that DepotDownloader writes
            // into the manifest dir's .DepotDownloader/ subdirectory. This is the
            // authoritative source (the same one .acf generation uses) and does
            // not depend on parsing DepotDownloader's stdout, which is unreliable
            // for QR-auth downloads.
            let manifest_id = read_manifest_id_from_disk(&manifest_dir, &depot_id)
                .unwrap_or_else(|| build_id.clone());

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
                dlcappid: None, // Best-effort enrichment below
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

    // Retrieve parsed manifest-to-depot mappings, depot names, and dlcappids from download log output
    let (parsed_manifest_to_depot, preflight_depot_names, parsed_dlcappids) = {
        app_handle
            .state::<DepotRunnerState>()
            .inner
            .lock()
            .ok()
            .map(|guard| (guard.manifest_to_depot.clone(), guard.depot_names.clone(), guard.depot_dlcappids.clone()))
            .unwrap_or_default()
    };

    // Build reverse lookup: depot_id -> manifest_id from the parsed log output
    let depot_to_manifest: std::collections::HashMap<String, String> = parsed_manifest_to_depot
        .iter()
        .map(|(manifest_id, depot_id)| (depot_id.clone(), manifest_id.clone()))
        .collect();

    // Fallback only: if reading the manifest ID from disk failed above (so it
    // still holds the build ID), try the IDs parsed from DepotDownloader's
    // stdout. The on-disk .manifest filename is the primary, more reliable
    // source, so this never overwrites a value already recovered from disk.
    for depot in &mut depots {
        if depot.manifest_id == build_id {
            if let Some(real_manifest_id) = depot_to_manifest.get(&depot.depot_id) {
                depot.manifest_id = real_manifest_id.clone();
            }
        }
    }

    // Enrich depots with their dlcappid BEFORE naming, since DLC depots are named
    // after their DLC app.
    // Primary source: parsed from DepotDownloader stdout (authoritative, from Steam PICS data).
    // Fallback: api.steamcmd.net (community-run mirror).
    for depot in &mut depots {
        if let Some(dlcappid) = parsed_dlcappids.get(&depot.depot_id) {
            depot.dlcappid = Some(dlcappid.clone());
        }
    }
    let any_missing_dlcappid = depots.iter().any(|d| d.dlcappid.is_none());
    if any_missing_dlcappid {
        let dlcappids = crate::steamcmd_api::fetch_depot_dlcappids(&job.app_id);
        for depot in &mut depots {
            if depot.dlcappid.is_none() {
                if let Some(dlcappid) = dlcappids.get(&depot.depot_id) {
                    depot.dlcappid = Some(dlcappid.clone());
                }
            }
        }
    }

    // Resolve depot display names. The goal is that a depot is named after a real
    // app as much as possible; a bare `depot_<id>` is a genuine last resort.
    //
    // Priority:
    //   1. Primary depot          -> the game's own name
    //   2. Shared/redist depot     -> the OWNER app's real name (e.g. 228989/228990
    //      -> "Steamworks Common Redistributables"). Detected data-drivenly via
    //      appinfo `sharedinstall`/`depotfromapp`, with the hardcoded
    //      `is_shared_depot` list as an offline fallback.
    //   3. DLC depot (has dlcappid) -> that DLC app's real name
    //   4. Parsed name from DepotDownloader stdout -- but ONLY when it is not just
    //      the app's own name echoed onto a non-primary depot (that echo is what
    //      mislabeled the redist depots as the game name).
    //   5. `depot_<id>` -- last resort, when no app/name signal exists at all.
    //
    // All network lookups are cached and best-effort: a failure silently degrades
    // to the next-lower-priority source and never fails the job.
    use crate::steam_api::{get_shared_depot_owner, is_shared_depot};

    // Data-driven shared-depot ownership from appinfo, unioned with the hardcoded
    // list so known redists still resolve offline.
    let shared_owners = crate::steamcmd_api::fetch_shared_depots(&job.app_id);

    // Resolve (and cache locally) an owner/DLC app's real name.
    let mut app_name_cache: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    let mut resolve_app_name = |owner_appid: &str| -> Option<String> {
        if let Some(cached) = app_name_cache.get(owner_appid) {
            return cached.clone();
        }
        let name = crate::steamcmd_api::fetch_app_name(owner_appid);
        app_name_cache.insert(owner_appid.to_string(), name.clone());
        name
    };

    for depot in &mut depots {
        let depot_id = depot.depot_id.clone();
        let is_primary = depot_id == primary_depot_id;

        // 1. Primary depot is the game itself.
        if is_primary {
            depot.depot_name = game_name.clone();
            continue;
        }

        // 2. Shared/redist depot -> owner app's real name.
        let shared_owner = shared_owners
            .get(&depot_id)
            .cloned()
            .or_else(|| is_shared_depot(&depot_id).then(|| get_shared_depot_owner(&depot_id).to_string()));
        if let Some(owner) = shared_owner {
            if let Some(name) = resolve_app_name(&owner) {
                depot.depot_name = name;
                continue;
            }
        }

        // 3. DLC depot -> that DLC's real name.
        if let Some(dlcappid) = depot.dlcappid.as_deref() {
            if let Some(name) = resolve_app_name(dlcappid) {
                depot.depot_name = name;
                continue;
            }
        }

        // 4. Parsed stdout name, unless it's just the app's own name echoed onto a
        //    non-primary depot (the redist mislabel) -- in that case skip it so we
        //    fall through to a better source or the numeric last resort.
        if let Some(name) = preflight_depot_names.get(&depot_id) {
            if name != &game_name {
                depot.depot_name = name.clone();
                continue;
            }
        }

        // 5. Last resort.
        depot.depot_name = format!("depot_{}", depot_id);
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
            debug_eprintln!("[STEAMDB] Got build date for app {}: {}", job.app_id, timestamp);
            Some(timestamp)
        }
        Err(err) => {
            debug_eprintln!("[STEAMDB] Failed to get build date: {}", err);
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
    let mut job_metadata = JobMetadataFile::new(
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
    job_metadata.game_description = game_description;
    job_metadata.website = website;
    // Best-effort: Steam's canonical install-folder name (config.installdir).
    // When available this is used verbatim as the merged depot folder name in
    // finalization; otherwise finalization falls back to the depot/game name.
    job_metadata.install_dir = crate::steamcmd_api::fetch_install_dir(&job.app_id);

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
        "Linux x64" => "Linux64".to_string(),
        "Linux x86" => "Linux32".to_string(),
        "Linux" => "Linux64".to_string(),
        "macOS x64" => "MacOS64".to_string(),
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

/// Redacts the value following `-password` / `-branchpassword` in a
/// DepotDownloader arg vector before it is written to a log.
fn redact_dd_password_args(args: &[String]) -> Vec<String> {
    let mut redacted = Vec::with_capacity(args.len());
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        redacted.push(arg.clone());
        if arg == "-password" || arg == "-branchpassword" {
            if iter.next().is_some() {
                redacted.push("********".to_string());
            }
        }
    }
    redacted
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
/// Failure modes for `compress_output`. `Cancelled` means the user aborted the
/// job mid-compression and must NOT be treated as a recoverable error (which
/// would leave the uncompressed output and report the job as completed).
enum CompressionError {
    Cancelled,
    Failed(String),
}

/// Path of the first volume 7-Zip writes when splitting (`archive.7z.001`).
fn first_volume_path(archive_path: &std::path::Path) -> std::path::PathBuf {
    let mut name = archive_path.as_os_str().to_os_string();
    name.push(".001");
    std::path::PathBuf::from(name)
}

/// Removes any archive output left behind, covering both the single-file case
/// (`archive.7z`) and the split case (`archive.7z.001`, `.002`, …). 7-Zip does
/// not pad volume numbers beyond three digits until they overflow, so we scan
/// the directory for siblings sharing the archive file name as a prefix.
fn remove_archive_outputs(archive_path: &std::path::Path) {
    let _ = std::fs::remove_file(archive_path);

    let (Some(parent), Some(file_name)) =
        (archive_path.parent(), archive_path.file_name().and_then(|n| n.to_str()))
    else {
        return;
    };

    // Split volumes are named "<file_name>.NNN"; match that exact shape so we
    // never delete unrelated files that merely start with the same prefix.
    let volume_prefix = format!("{}.", file_name);
    if let Ok(entries) = std::fs::read_dir(parent) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(suffix) = name.strip_prefix(&volume_prefix) {
                    if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

fn compress_output(
    app_handle: &AppHandle,
    output_path: &std::path::Path,
    job_id: &str,
    compression_password: Option<&str>,
    custom_compression_args: Option<&str>,
    split_volume_size: Option<&str>,
) -> Result<std::path::PathBuf, CompressionError> {
    let archive_path = resolve_archive_path(output_path);

    // When splitting, 7-Zip writes archive.7z.001, .002, … rather than
    // archive.7z itself, so check for the first volume in that case.
    let split = split_volume_size.is_some();
    let first_volume_path = first_volume_path(&archive_path);
    let existing = if split {
        &first_volume_path
    } else {
        &archive_path
    };
    if existing.exists() {
        return Err(CompressionError::Failed(format!(
            "Archive already exists: {}",
            existing.display()
        )));
    }

    let args = calculate_7z_compression_args(
        output_path,
        &archive_path,
        compression_password,
        custom_compression_args,
        split_volume_size,
    );
    let redacted_args = redact_7z_password_args(&args);

    emit_log(
        app_handle,
        "system",
        &format!("7-Zip command: 7zz {}", redacted_args.join(" ")),
        job_id,
    );

    let zip_state = app_handle.state::<SevenZipRunnerState>();
    let exit_code = match run_7zip_blocking(app_handle, &zip_state, args) {
        Ok(SevenZipOutcome::Exited(code)) => code,
        Ok(SevenZipOutcome::Cancelled) => {
            // User cancelled: remove any partial archive output (a single file,
            // or all .001/.002/… volumes when splitting) and signal cancel so
            // the worker stops the queue instead of falling through to success.
            remove_archive_outputs(&archive_path);
            return Err(CompressionError::Cancelled);
        }
        Err(err) => {
            remove_archive_outputs(&archive_path);
            return Err(CompressionError::Failed(err));
        }
    };

    if exit_code != 0 {
        // Clean up partial archive output if any exists
        remove_archive_outputs(&archive_path);
        return Err(CompressionError::Failed(format!(
            "7-Zip exited with code {}",
            exit_code
        )));
    }

    // When splitting, the single archive.7z never exists; the first volume
    // (archive.7z.001) is the canonical "did it produce output" marker.
    let produced = if split {
        first_volume_path.exists()
    } else {
        archive_path.exists()
    };
    if !produced {
        return Err(CompressionError::Failed(
            "Archive not found after compression".to_string(),
        ));
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

/// Builds DepotDownloader command-line arguments from job metadata.
///
/// `config_dir`, when provided, is passed as `-config-dir` so the fork stores its
/// account file (the saved login/refresh token) there instead of in the OS
/// isolated-storage location — keeping it in OmniPacker's portable-aware data
/// folder alongside `login.dat`.
fn build_depot_args(job: &JobMetadata, config_dir: Option<&str>) -> Result<Vec<String>, String> {
    let mut args = Vec::new();

    if let Some(dir) = config_dir {
        if !dir.is_empty() {
            args.push("-config-dir".to_string());
            args.push(dir.to_string());
        }
    }

    if !job.app_id.is_empty() && job.app_id != "unknown" {
        args.push("-app".to_string());
        args.push(job.app_id.clone());
    }

    if !job.branch.is_empty() {
        args.push("-branch".to_string());
        args.push(job.branch.clone());

        // -branchpassword unlocks a password-protected beta. DepotDownloader
        // rejects it without a -branch, and "public" is never password-gated, so
        // only emit it for a non-public branch with a password supplied.
        if !job.branch_password.is_empty() && job.branch != "public" {
            args.push("-branchpassword".to_string());
            args.push(job.branch_password.clone());
        }
    }

    let (os, arch) = map_os_selection(&job.os);
    args.push("-os".to_string());
    args.push(os.to_string());
    args.push("-osarch".to_string());
    args.push(arch.to_string());

    if job.qr_enabled {
        args.push("-qr".to_string());
        // -remember-password makes DepotDownloader open a PERSISTENT QR session
        // (IsPersistentSession=true) and store the resulting refresh token, so
        // subsequent queued jobs can reuse the login without re-scanning the QR.
        // Without it the QR token is non-persistent and Steam rejects its reuse
        // on job 2+ with AccessDenied.
        args.push("-remember-password".to_string());

        // Wipe the stored token after the final queue job when the user did not
        // opt into saving credentials (see the username branch below).
        if job.clear_token {
            args.push("-clear-token".to_string());
        }
    } else if !job.username.is_empty() {
        args.push("-username".to_string());
        args.push(job.username.clone());

        if !job.password.is_empty() {
            args.push("-password".to_string());
            args.push(job.password.clone());
        }
        args.push("-remember-password".to_string());

        // Set by the frontend on the final queue job when the user did not opt
        // into saving credentials. DepotDownloader still reuses the stored token
        // for every job in the queue (no Steam Guard re-prompt), then wipes it
        // from disk after this last job so nothing persists past the queue.
        if job.clear_token {
            args.push("-clear-token".to_string());
        }
    }
    // If both username and password are empty, attempt anonymous download (no auth args)

    Ok(args)
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
        guard.depot_dlcappids.clear();
        guard.last_depot_mentioned = None;
        guard.progress.reset();

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

    // Scope state_wrapper so its Drop impl runs before we spawn the real DD
    // process. The Drop impl kills any child stored in the mutex, so if it
    // outlives the preflight it would kill the main download.
    {
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
    } // state_wrapper dropped here, before main DD spawn

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

    // Keep the saved login token in OmniPacker's portable-aware config dir (next
    // to login.dat) rather than the fork's default isolated-storage location. A
    // failure here is non-fatal: omit -config-dir and let the fork fall back.
    let config_dir = crate::output_dir::resolve_config_dir(&app_handle)
        .ok()
        .map(|dir| dir.to_string_lossy().into_owned());

    let args = match build_depot_args(&job, config_dir.as_deref()) {
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

    // Open a main diagnostic log file for this DD run (no-op unless --debug)
    let mut main_log = DebugLog::new(&app_handle, "dd-main");

    let redacted_args = redact_dd_password_args(&args);

    debug_log!(main_log, "=== DD Main Log ===");
    debug_log!(main_log, "Binary: {}", path.display());
    debug_log!(main_log, "Args: {}", redacted_args.join(" "));
    debug_log!(main_log, "Working dir: {}", staging_dir.display());

    emit_log(
        &app_handle,
        "system",
        &format!("DepotDownloader args: {}", redacted_args.join(" ")),
        &job_id,
    );

    let mut command = Command::new(&path);
    command.args(&args);
    command.current_dir(&staging_dir);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::piped());

    // Hide console window on Windows (skip when --debug is passed to allow
    // diagnosing issues that only reproduce under CREATE_NO_WINDOW).
    #[cfg(windows)]
    {
        let debug_mode = crate::debug_console::debug_console_enabled_static(&app_handle);
        if debug_mode {
            debug_log!(main_log, "DEBUG MODE: skipping CREATE_NO_WINDOW");
        } else {
            debug_log!(main_log, "Using CREATE_NO_WINDOW");
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
    }

    debug_log!(main_log, "Spawning...");

    let mut child = match command.spawn() {
        Ok(child) => {
            debug_log!(main_log, "Spawn OK, pid: {}", child.id());
            child
        }
        Err(err) => {
            debug_log!(main_log, "Spawn FAILED: {err}");
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
        )
    });
    let stderr_handle = stderr.map(|stream| {
        spawn_log_reader(
            app_handle.clone(),
            stream,
            "stderr",
            job_id.clone(),
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

    thread::spawn(move || {
    // Move main_log into monitor thread for exit code logging
    let mut main_log = main_log;
    loop {
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
            debug_log!(main_log, "DD exited with code: {:?}", exit_code);
            #[cfg(windows)]
            {
                // On Windows, also log the raw exit status for NTSTATUS codes
                debug_log!(
                    main_log,
                    "DD raw exit status: 0x{:08X}",
                    status.code().unwrap_or(0) as u32
                );
            }
            emit_log(
                &app_handle_clone,
                "system",
                &format!("DepotDownloader exited with code: {:?}", exit_code),
                &job_id_for_monitor,
            );

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
                            let mut template_metadata = TemplateMetadata::from_job_metadata(&metadata);
                            template_metadata.set_uploader(
                                job_for_monitor.uploader_name.clone(),
                            );
                            template_metadata.set_upload_date(
                                job_for_monitor.upload_date.clone(),
                            );
                            app_handle_clone
                                .state::<TemplateMetadataState>()
                                .set(template_metadata);
                        }

                        // === COMPRESSION PHASE ===
                        let mut final_output_path = output_path.clone();
                        let mut compression_cancelled = false;
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

                            let custom_args_raw = job_for_monitor.custom_compression_args.as_str();
                            let custom_compression_args = if custom_args_raw.trim().is_empty() {
                                None
                            } else {
                                let (_accepted, rejected) = filter_custom_args(custom_args_raw);
                                if !rejected.is_empty() {
                                    emit_log(
                                        &app_handle_clone,
                                        "system",
                                        &format!(
                                            "Blocked managed 7-Zip flags from custom args: {}",
                                            rejected.join(", ")
                                        ),
                                        &job_id_for_monitor,
                                    );
                                }
                                Some(custom_args_raw)
                            };

                            let split_size_raw = job_for_monitor.split_volume_size.trim();
                            let split_volume_size = if split_size_raw.is_empty() {
                                None
                            } else {
                                Some(split_size_raw)
                            };

                            match compress_output(
                                &app_handle_clone,
                                &output_path,
                                &job_id_for_monitor,
                                compression_password,
                                custom_compression_args,
                                split_volume_size,
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
                                Err(CompressionError::Failed(err)) => {
                                    // Genuine compression error: keep the
                                    // uncompressed output and let the job finish
                                    // as completed so the user still gets files.
                                    emit_log(
                                        &app_handle_clone,
                                        "system",
                                        &format!("Compression failed: {}. Uncompressed output available.", err),
                                        &job_id_for_monitor,
                                    );
                                }
                                Err(CompressionError::Cancelled) => {
                                    // User cancelled mid-compression: do NOT fall
                                    // through to template generation / completed.
                                    compression_cancelled = true;
                                    emit_log(
                                        &app_handle_clone,
                                        "system",
                                        "Compression cancelled by user.",
                                        &job_id_for_monitor,
                                    );
                                }
                            }
                        }
                        // === END COMPRESSION ===

                        // === CHECKSUM PHASE ===
                        // Hash files on disk, so the template can embed a verifiable SHA-256.
                        // Runs after compression: final_output_path is `Game.7z`,
                        // `.7z.001.../.002...` volumes, or, if compression was
                        // skipped (in which case `compute_output_checksums` returns an empty list
                        // and the block renders blank rather than erroring).
                        match calculate_checksums(&final_output_path) {
                            Ok(checksums) => {
                                if let Some(mut template_metadata) =
                                    app_handle_clone.state::<TemplateMetadataState>().get()
                                {
                                    template_metadata.set_checksums(checksums);
                                    app_handle_clone
                                        .state::<TemplateMetadataState>()
                                        .set(template_metadata);
                                }
                            }
                            Err(err) => {
                                emit_log(
                                    &app_handle_clone,
                                    "system",
                                    &format!("Failed to compute checksum: {err}. Continuing without it."),
                                    &job_id_for_monitor,
                                );
                            }
                        }
                        // === END CHECKSUM PHASE ===

                        // === TEMPLATE GENERATION ===
                        // Generate the template text file next to the output. This
                        // runs even when compression was cancelled: the uncompressed
                        // output folder is kept in that case (the partial archive was
                        // removed, so final_output_path points at the folder), and the
                        // template describes the output regardless of whether it was
                        // ever compressed.
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

                            // Render one .txt per profile the frontend selected.
                            // An empty list falls back to the default template.
                            match write_template_files(
                                &final_output_path,
                                &template_metadata,
                                &job_for_monitor.template_profiles,
                            ) {
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

                        if compression_cancelled {
                            // The user cancelled during compression. The uncompressed
                            // output folder and its template are kept; mark the job
                            // cancelled (not completed, so the frontend halts the queue
                            // instead of advancing) and clean up staging.
                            emit_status(&app_handle_clone, "cancelled", None, &job_id_for_monitor);
                        } else {
                            emit_status(&app_handle_clone, "completed", Some(0), &job_id_for_monitor);
                        }
                        // Cleanup staging after finalization (kept output folder is
                        // separate from staging).
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
                let _ = cleanup_staging_dir(&app_handle_clone, &job_id_for_monitor);
            }

            clear_runner_state(&state_handle, &job_id_for_monitor);
            return;
        }

        thread::sleep(Duration::from_millis(100));
    } // end loop
    }); // end thread::spawn
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
    guard.depot_dlcappids.clear();
    guard.last_depot_mentioned = None;

    // Emit "cancelled" (not "exited") so the frontend halts the whole queue
    // rather than auto-advancing to the next job as it does on a normal exit.
    let _ = status;
    emit_status(&app_handle, "cancelled", None, &job_id);

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

/// Parses a single DepotDownloader stdout line for download-progress signals and
/// emits a `dd:progress` event when the whole-job percent changes.
///
/// Recognized signals (see DownloadProgress):
/// - `Downloading depot <id> manifest` — phase 1, announces a depot (denominator)
/// - `Depot <id> - Downloaded <n> bytes` — a depot finished
/// - `<NN.NN>% <path>` — per-file progress within the current depot
///
/// Additive and self-contained: it does not interfere with the metadata parser
/// in `maybe_store_build_datetime`.
fn maybe_update_progress(app_handle: &AppHandle, line: &str, job_id: &str) {
    let state = app_handle.state::<DepotRunnerState>();
    let Ok(mut guard) = state.inner.lock() else {
        return;
    };
    if guard.job_id.as_deref() != Some(job_id) {
        return;
    }

    let (changed, depot_completed) = guard.progress.apply_line(line);
    if !changed {
        return;
    }

    let Some(job_percent) = guard.progress.job_percent() else {
        return;
    };
    // Skip identical-percent spam, but always emit on a depot-completion so the
    // N/M counter advances even when the percent is unchanged.
    if guard.progress.last_emitted_percent == Some(job_percent) && !depot_completed {
        return;
    }
    guard.progress.last_emitted_percent = Some(job_percent);
    let payload = ProgressPayload {
        job_percent,
        completed_depots: guard.progress.completed_depots,
        total_depots: guard.progress.announced_depots.len(),
        depot_percent: guard.progress.current_depot_percent,
        job_id: job_id.to_string(),
    };
    drop(guard);
    let _ = app_handle.emit("dd:progress", payload);
}

fn clear_runner_state(state_handle: &Arc<Mutex<RunningJobState>>, job_id: &str) {
    if let Ok(mut guard) = state_handle.lock() {
        if guard.job_id.as_deref() == Some(job_id) {
            guard.job_id = None;
            guard.stdin = None;
            guard.build_datetime_utc = None;
            guard.depot_timestamps.clear();
            guard.manifest_to_depot.clear();
            guard.depot_dlcappids.clear();
            guard.last_depot_mentioned = None;
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

    let dd_path = resolve_depotdownloader_path(app_handle)?;
    // Point preflight at the same portable-aware account store as the main run.
    let config_dir = crate::output_dir::resolve_config_dir(app_handle)
        .ok()
        .map(|dir| dir.to_string_lossy().into_owned());
    let mut args = build_preflight_args(job, config_dir.as_deref())?;
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

    // Hide console window on Windows (skip when --debug is passed)
    #[cfg(windows)]
    {
        let debug_mode = crate::debug_console::debug_console_enabled_static(&app_handle);
        if !debug_mode {
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
    }

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

    let _ = fs::remove_dir_all(&preflight_dir);
    Ok(())
}

fn spawn_log_reader(
    app_handle: AppHandle,
    stream: impl std::io::Read + Send + 'static,
    tag: &str,
    job_id: String,
) -> thread::JoinHandle<()> {
    let stream_name = tag.to_string();
    const EMAIL_PROMPT: &str =
        "STEAM GUARD! Please enter the auth code sent to the email at";

    // Open the diagnostic log before spawning the thread (no-op unless --debug)
    let mut log = DebugLog::new(&app_handle, &format!("dd-{tag}"));

    thread::spawn(move || {
        use std::io::BufReader;

        let mut reader = BufReader::new(stream);
        let mut buffer = [0u8; 1024];
        let mut pending: Vec<u8> = Vec::new();
        let mut prompt_emitted = false;

        debug_log!(log, "=== Log started ===");

        loop {
            let n = match reader.read(&mut buffer) {
                Ok(n) => n,
                Err(e) => {
                    debug_log!(log, "[ERROR] read failed: {e}");
                    break;
                }
            };

            if n == 0 {
                debug_log!(log, "[EOF]");
                break; // EOF
            }

            if log.is_active() {
                let chunk = &buffer[..n];
                let hex: String =
                    chunk.iter().map(|b| format!("{b:02x} ")).collect();
                debug_log!(log, "[RAW {n} bytes] {hex}");
                debug_log!(log, "[UTF8] {}", String::from_utf8_lossy(chunk));
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
                debug_log!(log, "[DECODED] {line}");
                emit_log(&app_handle, &stream_name, &line, &job_id);
                maybe_store_build_datetime(&app_handle, &line, &job_id);
                maybe_update_progress(&app_handle, &line, &job_id);
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

    let mut log = DebugLog::new(&app_handle, &format!("dd-preflight-{tag}"));

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

            debug_log!(log, "[RAW {} bytes] {}", n, String::from_utf8_lossy(&buffer[..n]));

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
    static DEPOT_DLCAPPID_RE: OnceLock<Regex> = OnceLock::new();
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
    let depot_dlcappid = DEPOT_DLCAPPID_RE.get_or_init(|| {
        Regex::new(r"[Dd]epot\s+(\d+)\s+dlcappid\s+(\d+)").unwrap()
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
                debug_eprintln!("[DOWNLOAD] Found depot name: {} -> {}", depot_id, name);
                guard.depot_names.insert(depot_id.clone(), name);
                guard.last_depot_mentioned = Some(depot_id);
                return;
            }
        }

        // Track depot dlcappids: Depot 12345 dlcappid 67890
        if let Some(caps) = depot_dlcappid.captures(line) {
            if let (Some(depot_id), Some(dlcappid)) = (
                caps.get(1).map(|m| m.as_str().to_string()),
                caps.get(2).map(|m| m.as_str().to_string()),
            ) {
                guard.depot_dlcappids.insert(depot_id.clone(), dlcappid);
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
                debug_eprintln!("[DOWNLOAD] Found depot name (appinfo): {} -> {}", depot_id, name);
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
    // .NET pipes stdout as UTF-8 when there is no console attached.
    // Try UTF-8 first; only fall back to the Windows console codepage
    // for legacy programs that emit OEM-encoded bytes.
    if let Ok(s) = std::str::from_utf8(buf) {
        return s.to_string();
    }

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

#[cfg(test)]
mod tests {
    use super::{map_os_selection, DownloadProgress};

    #[test]
    fn progress_unknown_until_a_depot_is_announced() {
        let mut p = DownloadProgress::default();
        // A stray percent line before any manifest announce: no denominator yet.
        let (changed, _) = p.apply_line(" 50.00% depots/x/file");
        assert!(changed); // percent recorded
        assert_eq!(p.job_percent(), None);
    }

    #[test]
    fn progress_counts_announced_depots_as_denominator() {
        let mut p = DownloadProgress::default();
        p.apply_line("Downloading depot 228984 manifest 111");
        p.apply_line("Downloading depot 2379781 manifest 222");
        // Re-announcing the same depot must not double-count.
        let (changed, _) = p.apply_line("Downloading depot 228984 manifest 111");
        assert!(!changed);
        assert_eq!(p.announced_depots.len(), 2);
        // Nothing downloaded yet -> 0%.
        assert_eq!(p.job_percent(), Some(0));
    }

    #[test]
    fn progress_whole_job_percent_across_two_depots() {
        let mut p = DownloadProgress::default();
        p.apply_line("Downloading depot 228984 manifest 111");
        p.apply_line("Downloading depot 2379781 manifest 222");

        // First depot at 50% -> (0*100 + 50)/2 = 25.
        p.apply_line(" 50.00% depots/228984/file");
        assert_eq!(p.job_percent(), Some(25));

        // First depot done -> (1*100 + 0)/2 = 50, and percent resets.
        let (_, completed) = p.apply_line("Depot 228984 - Downloaded 13436144 bytes (13742505 bytes uncompressed)");
        assert!(completed);
        assert_eq!(p.current_depot_percent, 0);
        assert_eq!(p.job_percent(), Some(50));

        // Second depot at 80% -> (1*100 + 80)/2 = 90.
        p.apply_line(" 80.00% depots/2379781/file");
        assert_eq!(p.job_percent(), Some(90));

        // Second depot done -> (2*100)/2 = 100.
        p.apply_line("Depot 2379781 - Downloaded 60189696 bytes (66662933 bytes uncompressed)");
        assert_eq!(p.completed_depots, 2);
        assert_eq!(p.job_percent(), Some(100));
    }

    #[test]
    fn progress_completed_never_exceeds_total() {
        let mut p = DownloadProgress::default();
        p.apply_line("Downloading depot 228984 manifest 111");
        // Two completion lines for the single known depot: completed is clamped.
        p.apply_line("Depot 228984 - Downloaded 1 bytes (1 bytes uncompressed)");
        p.apply_line("Depot 228984 - Downloaded 1 bytes (1 bytes uncompressed)");
        assert_eq!(p.completed_depots, 1);
        assert_eq!(p.job_percent(), Some(100));
    }

    #[test]
    fn os_selection_maps_each_arch() {
        assert_eq!(map_os_selection("Windows x64"), ("windows", "64"));
        assert_eq!(map_os_selection("Windows x86"), ("windows", "32"));
        assert_eq!(map_os_selection("Linux x64"), ("linux", "64"));
        assert_eq!(map_os_selection("Linux x86"), ("linux", "32"));
        assert_eq!(map_os_selection("macOS x64"), ("macos", "64"));
    }

    #[test]
    fn os_selection_falls_back_to_windows_x64() {
        assert_eq!(map_os_selection("anything else"), ("windows", "64"));
    }
}
