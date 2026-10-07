use serde::Serialize;
use std::{
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::debug_console::DebugConsoleState;
use crate::debug_log::{debug_log, DebugLog};

#[derive(Clone)]
pub struct SevenZipRunnerState {
    child: Arc<Mutex<Option<Child>>>,
    /// Set when the running 7-Zip child was killed by the user (cancel), as
    /// opposed to exiting on its own. Lets `run_7zip_blocking` distinguish a
    /// user cancel (which nulls the child out from under it) from a genuine
    /// non-zero exit, so a cancelled compression is not reported as success.
    cancelled: Arc<AtomicBool>,
}

/// Result of a blocking 7-Zip run. `Cancelled` means the user killed the
/// process via `cancel_7zip`; it is NOT a normal exit code and callers must
/// not treat it as success or as a recoverable compression error.
pub enum SevenZipOutcome {
    Exited(i32),
    Cancelled,
}

impl SevenZipRunnerState {
    pub fn new() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Clears a previous cancel request. Call once when a job's compression
    /// starts, not between its compress and test steps.
    pub fn reset_cancel(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// Kills the running 7-Zip child (if any) and clears it from state.
    /// Safe to call when no child is running.
    pub fn kill_child(&self) {
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut child) = guard.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
            *guard = None;
        }
    }
}

impl Drop for SevenZipRunnerState {
    fn drop(&mut self) {
        self.kill_child();
    }
}

#[derive(Clone, Serialize)]
struct StatusPayload {
    status: String,
    code: Option<i32>,
}

#[derive(Clone, Serialize)]
struct LogPayload {
    stream: String,
    line: String,
}

#[derive(Clone, Serialize)]
struct ProgressPayload {
    percent: u8,
}

pub fn resolve_7zip_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    match resolve_bundled_7zip_path(app_handle) {
        Ok(path) => Ok(path),
        Err(bundled_error) => resolve_system_7zip_path().ok_or_else(|| {
            format!(
                "7-Zip not found. Bundled lookup failed: {bundled_error}. No system installation found."
            )
        }),
    }
}

#[tauri::command]
pub fn cancel_7zip(
    app_handle: AppHandle,
    state: State<'_, SevenZipRunnerState>,
) -> Result<(), String> {
    let mut guard = state
        .child
        .lock()
        .map_err(|_| "Failed to lock 7-Zip state".to_string())?;

    // Record the cancel first, even if no child is running right now: between
    // compressing and testing an archive there's a moment with no 7-Zip
    // process, and a cancel landing there must still stop the next step.
    // The flag is only reset when a new compression run begins.
    state.cancelled.store(true, Ordering::SeqCst);

    let Some(child) = guard.as_mut() else {
        return Ok(());
    };

    child
        .kill()
        .map_err(|err| format!("Failed to terminate 7-Zip: {err}"))?;

    let status = child
        .wait()
        .map_err(|err| format!("Failed to await 7-Zip shutdown: {err}"))?;

    *guard = None;
    emit_status(&app_handle, "exited", status.code());

    Ok(())
}

/// Runs 7-Zip synchronously and waits for completion.
/// Unlike `run_7zip()`, this blocks until the process exits and returns the exit code.
/// Used for compression in the job finalization pipeline.
pub fn run_7zip_blocking(
    app_handle: &AppHandle,
    state: &SevenZipRunnerState,
    args: Vec<String>,
) -> Result<SevenZipOutcome, String> {
    let mut guard = state
        .child
        .lock()
        .map_err(|_| "Failed to lock 7-Zip state".to_string())?;

    if guard.is_some() {
        return Err("7-Zip is already running".to_string());
    }

    // A cancel requested before (or between) runs of this job stops it here.
    if state.cancelled.load(Ordering::SeqCst) {
        return Ok(SevenZipOutcome::Cancelled);
    }

    let path = resolve_7zip_path(app_handle)?;

    let mut command = Command::new(&path);
    command.args(&args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    // No stdin: if 7-Zip ever wants input (e.g. a password prompt) it fails
    // immediately instead of hanging the job forever.
    command.stdin(Stdio::null());

    // Hide console window on Windows
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn 7-Zip: {}", e))?;

    // Take ownership of streams for logging
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    *guard = Some(child);
    drop(guard);

    // Spawn log readers that emit events
    if let Some(stream) = stdout {
        spawn_log_reader(app_handle.clone(), stream, "stdout");
    }
    if let Some(stream) = stderr {
        spawn_log_reader(app_handle.clone(), stream, "stderr");
    }

    loop {
        let status_code = {
            let mut guard = state
                .child
                .lock()
                .map_err(|_| "Failed to lock 7-Zip state".to_string())?;

            let Some(child) = guard.as_mut() else {
                // The child was nulled out from under us. If `cancel_7zip` set
                // the cancel flag, this is a user cancel; otherwise treat it as
                // a cancel as well (the child is gone, no real exit code exists).
                return Ok(SevenZipOutcome::Cancelled);
            };

            match child.try_wait() {
                Ok(Some(status)) => {
                    *guard = None;
                    Some(status.code().unwrap_or(-1))
                }
                Ok(None) => None,
                Err(err) => {
                    *guard = None;
                    return Err(format!("Failed to wait on 7-Zip: {}", err));
                }
            }
        };

        if let Some(code) = status_code {
            // A real exit could still race a cancel (process exits just as the
            // user kills it); prefer Cancelled so we never report success.
            if state.cancelled.load(Ordering::SeqCst) {
                return Ok(SevenZipOutcome::Cancelled);
            }
            return Ok(SevenZipOutcome::Exited(code));
        }

        thread::sleep(Duration::from_millis(100));
    }
}

/// How hard 7-Zip compresses. Ultra (`-mx9`) is the historical default and
/// gives the smallest archives; the lower levels trade size for time, which
/// matters on 100+ GB games where Ultra can take hours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CompressionLevel {
    Fast,
    Normal,
    Maximum,
    #[default]
    Ultra,
}

impl CompressionLevel {
    fn mx(self) -> u8 {
        match self {
            CompressionLevel::Fast => 1,
            CompressionLevel::Normal => 5,
            CompressionLevel::Maximum => 7,
            CompressionLevel::Ultra => 9,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CompressionLevel::Fast => "Fast",
            CompressionLevel::Normal => "Normal",
            CompressionLevel::Maximum => "Maximum",
            CompressionLevel::Ultra => "Ultra",
        }
    }
}

/// Arguments to verify a finished archive (`7z t`). For split output, point
/// it at the first volume; 7-Zip follows the rest. The password is always
/// passed (empty when there is none) so 7-Zip never stops to ask for one.
pub fn archive_test_args(archive_or_first_volume: &Path, password: Option<&str>) -> Vec<String> {
    vec![
        "t".to_string(),
        format!("-p{}", password.unwrap_or("")),
        "-bsp1".to_string(),
        archive_or_first_volume.to_string_lossy().to_string(),
    ]
}

/// Calculates optimal 7-Zip compression arguments based on CPU cores.
/// Prioritizes smallest file size with `-mx9` (ultra compression).
/// Thread count is adapted to prevent system lockup on weak hardware.
/// Checks whether a 7-Zip argument conflicts with flags managed by OmniPacker.
/// Returns the matched prefix if blocked, or None if the arg is allowed.
fn blocked_arg_prefix(arg: &str) -> Option<&'static str> {
    let lower = arg.to_ascii_lowercase();
    // `-v` is managed by the dedicated split setting; block it here so a custom
    // arg can't inject a conflicting second volume size.
    const BLOCKED: &[&str] = &["-t", "-p", "-bsp", "-v", "a"];
    for prefix in BLOCKED {
        if *prefix == "a" {
            if lower == "a" {
                return Some(prefix);
            }
        } else if lower.starts_with(prefix) {
            return Some(prefix);
        }
    }
    None
}

/// Filters custom user arguments, removing any that conflict with managed flags.
/// Returns (accepted_args, rejected_args).
pub fn filter_custom_args(raw: &str) -> (Vec<String>, Vec<String>) {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for token in raw.split_whitespace() {
        if blocked_arg_prefix(token).is_some() {
            rejected.push(token.to_string());
        } else {
            accepted.push(token.to_string());
        }
    }
    (accepted, rejected)
}

pub fn calculate_7z_compression_args(
    source_dir: &std::path::Path,
    output_archive: &std::path::Path,
    password: Option<&str>,
    custom_args: Option<&str>,
    split_volume_size: Option<&str>,
    level: CompressionLevel,
) -> Vec<String> {
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * MB;
    let cpu_cores = num_cpus::get();

    // Conservative threading: leave headroom for system responsiveness
    let mut max_threads = match cpu_cores {
        1..=2 => 1,
        3..=4 => 2,
        5..=8 => 4,
        9..=16 => 8,
        _ => 12,
    };

    let mut system = System::new_with_specifics(
        RefreshKind::new()
            .with_memory(MemoryRefreshKind::everything())
            .with_cpu(CpuRefreshKind::everything()),
    );
    system.refresh_memory();
    system.refresh_cpu();
    thread::sleep(Duration::from_millis(200));
    system.refresh_cpu();

    let cpu_usage = system.global_cpu_info().cpu_usage();
    if cpu_usage >= 80.0 {
        max_threads = (max_threads / 2).max(1);
    } else if cpu_usage >= 60.0 {
        max_threads = ((max_threads * 2) / 3).max(1);
    }

    let total_bytes = system.total_memory();
    let available_bytes = system.available_memory();
    let used_bytes = total_bytes.saturating_sub(available_bytes);
    let used_ratio = if total_bytes > 0 {
        used_bytes as f64 / total_bytes as f64
    } else {
        0.0
    };

    let high_memory_pressure = used_ratio >= 0.80 || available_bytes < 3 * GB;
    let medium_memory_pressure = !high_memory_pressure
        && (used_ratio >= 0.60 || available_bytes < 6 * GB);

    let base_reserved_bytes = std::cmp::max(512 * MB, total_bytes / 5);
    let reserved_bytes = if high_memory_pressure {
        base_reserved_bytes.max(available_bytes / 2)
    } else if medium_memory_pressure {
        base_reserved_bytes.max(available_bytes / 3)
    } else {
        base_reserved_bytes.max(available_bytes / 4)
    };
    let usable_bytes = available_bytes.saturating_sub(reserved_bytes);

    if high_memory_pressure {
        max_threads = max_threads.min(2);
    } else if medium_memory_pressure {
        max_threads = max_threads.min(4);
    }

    let min_per_thread_bytes = if high_memory_pressure {
        GB
    } else if medium_memory_pressure {
        768 * MB
    } else {
        512 * MB
    };
    let memory_thread_cap = if usable_bytes >= min_per_thread_bytes {
        (usable_bytes / min_per_thread_bytes) as usize
    } else {
        1
    };
    max_threads = max_threads.min(memory_thread_cap.max(1));

    const DICT_SIZES: &[(u64, &str)] = &[
        (8 * MB, "8m"),
        (16 * MB, "16m"),
        (32 * MB, "32m"),
        (64 * MB, "64m"),
        (128 * MB, "128m"),
        (256 * MB, "256m"),
    ];

    let mut threads = max_threads.max(1);
    let dict_label = loop {
        let per_thread_budget = if usable_bytes == 0 {
            0
        } else {
            usable_bytes / threads as u64
        };
        let max_dict_bytes = per_thread_budget / 12;
        let mut selected = None;
        for (size, label) in DICT_SIZES.iter().rev() {
            if *size <= max_dict_bytes {
                selected = Some(*label);
                break;
            }
        }
        if let Some(label) = selected {
            break label;
        }
        if threads <= 1 {
            break DICT_SIZES[0].1;
        }
        threads = threads.saturating_sub(1).max(1);
    };

    let mut args = vec![
        "a".to_string(),                                    // Add to archive
        "-t7z".to_string(),                                 // 7z format (best compression)
        format!("-mx{}", level.mx()),                       // Compression level (Ultra = 9)
        format!("-mmt{}", threads),                         // Multi-threading
        format!("-md={}", dict_label),                      // Dictionary size tuned by resources
        "-bsp1".to_string(),                                // Progress output to stdout
    ];

    if let Some(password) = password {
        if !password.is_empty() {
            args.push(format!("-p{}", password));
            args.push("-mhe=on".to_string()); // Encrypt file headers by default
        }
    }

    // Splice in user-supplied custom arguments (already filtered by caller)
    if let Some(custom) = custom_args {
        let (accepted, _rejected) = filter_custom_args(custom);
        args.extend(accepted);
    }

    // Split into volumes when requested. `split_volume_size` is a 7-Zip size
    // token (e.g. "100m", "4g") that makes 7-Zip emit archive.7z.001, .002, …
    if let Some(size) = split_volume_size {
        let size = size.trim();
        if !size.is_empty() {
            args.push(format!("-v{}", size));
        }
    }

    args.push(output_archive.to_string_lossy().to_string()); // Archive path
    // Append a wildcard so 7-Zip archives the *contents* of source_dir rather
    // than the directory itself. 7-Zip stores entries relative to the wildcard's
    // base dir, so the archive root becomes `steamapps\…` instead of the long
    // `<Game>.Build.<id>.<plat>.<branch>\steamapps\…`. That redundant wrapper
    // folder (whose name is already carried by the .7z filename) added ~40 chars
    // to every internal path and pushed deeply-nested game files past Windows'
    // MAX_PATH limit on extraction. This matches SuperSteamPacker's `7z a … *`.
    args.push(source_dir.join("*").to_string_lossy().to_string()); // Source contents
    args
}

fn emit_status(app_handle: &AppHandle, status: &str, code: Option<i32>) {
    let _ = app_handle.emit(
        "7z:status",
        StatusPayload {
            status: status.to_string(),
            code,
        },
    );
}

fn emit_progress(app_handle: &AppHandle, percent: u8) {
    let _ = app_handle.emit("7z:progress", ProgressPayload { percent });
}

fn extract_percent(line: &str) -> Option<u8> {
    let bytes = line.as_bytes();
    for idx in (0..bytes.len()).rev() {
        if bytes[idx] == b'%' {
            let mut start = idx;
            while start > 0 && bytes[start - 1].is_ascii_digit() {
                start -= 1;
            }
            if start < idx {
                if let Ok(value) = line[start..idx].parse::<u8>() {
                    if value <= 100 {
                        return Some(value);
                    }
                }
            }
        }
    }
    None
}

fn is_progress_line(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.ends_with('%') {
        return false;
    }
    let number = trimmed.trim_end_matches('%');
    if number.is_empty() {
        return false;
    }
    if !number.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    number.parse::<u8>().is_ok()
}

fn spawn_log_reader(app_handle: AppHandle, stream: impl std::io::Read + Send + 'static, tag: &str) {
    let stream_name = tag.to_string();
    let mut log = DebugLog::new(&app_handle, &format!("7z-{tag}"));

    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buffer = [0u8; 1024];
        let mut current_line = String::new();
        let mut last_percent: Option<u8> = None;
        let mut last_was_cr = false;

        // Read until EOF (Ok(0)) or a read error.
        while let Ok(n @ 1..) = reader.read(&mut buffer) {
            let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
            debug_log!(log, "[RAW {n} bytes] {chunk}");
            for ch in chunk.chars() {
                match ch {
                    '\r' => {
                        if !current_line.is_empty() {
                            let line = current_line.clone();
                            current_line.clear();
                            if !is_progress_line(&line) {
                                let debug_state = app_handle.state::<DebugConsoleState>();
                                if debug_state.enabled() {
                                    debug_state.write_line(&format!("[7z:{stream_name}] {line}"));
                                }
                                let _ = app_handle.emit(
                                    "7z:log",
                                    LogPayload {
                                        stream: stream_name.clone(),
                                        line,
                                    },
                                );
                            }
                        }
                        last_was_cr = true;
                        continue;
                    }
                    '\n' => {
                        if !last_was_cr && !current_line.is_empty() {
                            let line = current_line.clone();
                            current_line.clear();
                            if !is_progress_line(&line) {
                                let debug_state = app_handle.state::<DebugConsoleState>();
                                if debug_state.enabled() {
                                    debug_state.write_line(&format!("[7z:{stream_name}] {line}"));
                                }
                                let _ = app_handle.emit(
                                    "7z:log",
                                    LogPayload {
                                        stream: stream_name.clone(),
                                        line,
                                    },
                                );
                            }
                        }
                        last_was_cr = false;
                        continue;
                    }
                    '\u{0008}' => {
                        current_line.pop();
                    }
                    _ => {
                        current_line.push(ch);
                    }
                }

                last_was_cr = false;

                if let Some(percent) = extract_percent(&current_line) {
                    if Some(percent) != last_percent {
                        last_percent = Some(percent);
                        emit_progress(&app_handle, percent);
                    }
                }
            }
        }

        if !current_line.is_empty() {
            let line = current_line.trim_end_matches('\r').to_string();
            if !is_progress_line(&line) {
                let debug_state = app_handle.state::<DebugConsoleState>();
                if debug_state.enabled() {
                    debug_state.write_line(&format!("[7z:{stream_name}] {line}"));
                }
                let _ = app_handle.emit(
                    "7z:log",
                    LogPayload {
                        stream: stream_name.clone(),
                        line,
                    },
                );
            }
        }
    });
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

fn resolve_bundled_7zip_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    // Determine platform-specific binary name with extension
    #[cfg(windows)]
    let binary_name = "7za.exe";
    #[cfg(not(windows))]
    let binary_name = "7zz";

    let platform_subdir = get_platform_subdir();

    // Use Tauri's path resolution with platform-specific subdirectory
    let sidecar_path = app_handle
        .path()
        .resolve(
            format!("binaries/{}/{}", platform_subdir, binary_name),
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Failed to resolve 7-Zip sidecar: {}", e))?;

    // Strip \\?\ prefix for consistency with depot_runner
    let sidecar_path = crate::depot_runner::strip_extended_length_prefix(sidecar_path);

    if !sidecar_path.exists() {
        return Err(format!("7-Zip sidecar not found at {}", sidecar_path.display()));
    }

    if !is_executable(&sidecar_path) {
        return Err(format!(
            "7-Zip sidecar is not executable at {}",
            sidecar_path.display()
        ));
    }

    Ok(sidecar_path)
}

fn resolve_system_7zip_path() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(windows)]
    {
        let program_files = std::env::var_os("ProgramFiles");
        let program_files_x86 = std::env::var_os("ProgramFiles(x86)");

        candidates.extend([
            program_files
                .as_ref()
                .map(|root| PathBuf::from(root).join("7-Zip").join("7z.exe")),
            program_files
                .as_ref()
                .map(|root| PathBuf::from(root).join("7-Zip").join("7za.exe")),
            program_files_x86
                .as_ref()
                .map(|root| PathBuf::from(root).join("7-Zip").join("7z.exe")),
            program_files_x86
                .as_ref()
                .map(|root| PathBuf::from(root).join("7-Zip").join("7za.exe")),
        ]);
    }

    let path_hits = find_in_path(&["7zz", "7z", "7za"]);
    candidates.extend(path_hits);

    candidates
        .into_iter()
        .flatten()
        .find(|path| path.exists() && is_executable(path))
}

fn find_in_path(executables: &[&str]) -> Vec<Option<PathBuf>> {
    let Some(paths) = std::env::var_os("PATH") else {
        return Vec::new();
    };

    let path_var = std::env::split_paths(&paths);

    let mut hits = Vec::new();
    for dir in path_var {
        for candidate in executables {
            let path = dir.join(candidate);
            if path.exists() {
                hits.push(Some(path));
            }
        }
    }

    hits
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    std::fs::metadata(path)
        .map(|meta| meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn split_size_adds_volume_arg_before_paths() {
        let args = calculate_7z_compression_args(
            Path::new("/tmp/out"),
            Path::new("/tmp/out.7z"),
            None,
            None,
            Some("100m"),
            CompressionLevel::default(),
        );
        let v_idx = args.iter().position(|a| a == "-v100m").expect("-v arg present");
        let archive_idx = args
            .iter()
            .position(|a| a == "/tmp/out.7z")
            .expect("archive path present");
        assert!(v_idx < archive_idx, "-v must precede the archive path");
    }

    #[test]
    fn no_split_when_size_absent_or_blank() {
        let none = calculate_7z_compression_args(
            Path::new("/tmp/out"),
            Path::new("/tmp/out.7z"),
            None,
            None,
            None,
            CompressionLevel::default(),
        );
        assert!(none.iter().all(|a| !a.starts_with("-v")));

        let blank = calculate_7z_compression_args(
            Path::new("/tmp/out"),
            Path::new("/tmp/out.7z"),
            None,
            None,
            Some("   "),
            CompressionLevel::default(),
        );
        assert!(blank.iter().all(|a| !a.starts_with("-v")));
    }

    #[test]
    fn compression_level_maps_to_mx_and_custom_mx_overrides() {
        let args = |level, custom| {
            calculate_7z_compression_args(
                Path::new("/tmp/out"),
                Path::new("/tmp/out.7z"),
                None,
                custom,
                None,
                level,
            )
        };
        let mx = |a: &Vec<String>| a.iter().filter(|x| x.starts_with("-mx")).cloned().collect::<Vec<_>>();
        assert_eq!(mx(&args(CompressionLevel::Ultra, None)), vec!["-mx9"]);
        assert_eq!(mx(&args(CompressionLevel::Maximum, None)), vec!["-mx7"]);
        assert_eq!(mx(&args(CompressionLevel::Normal, None)), vec!["-mx5"]);
        assert_eq!(mx(&args(CompressionLevel::Fast, None)), vec!["-mx1"]);
        // A user's own -mx comes after ours, so 7-Zip uses it (last switch wins).
        let a = args(CompressionLevel::Ultra, Some("-mx3"));
        assert_eq!(mx(&a), vec!["-mx9", "-mx3"]);
        // Older job payloads without the field get the historical Ultra.
        let legacy: CompressionLevel = serde_json::from_value(serde_json::json!("ultra")).unwrap();
        assert_eq!(legacy, CompressionLevel::Ultra);
        assert_eq!(CompressionLevel::default(), CompressionLevel::Ultra);
    }

    /// Runs the bundled 7-Zip directly (no AppHandle) to check that the test
    /// step really detects damage in split, password-protected archives.
    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn archive_test_args_catch_a_corrupt_split_volume() {
        use std::io::{Seek, SeekFrom, Write};
        let zz = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries/linux-x64/7zz");
        let dir = std::env::temp_dir().join(format!(
            "omnipacker_7ztest_{}",
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        // Incompressible data so the archive really spans several volumes.
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let data: Vec<u8> = (0..3_000_000)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed as u8
            })
            .collect();
        std::fs::write(src.join("game.bin"), &data).unwrap();

        let archive = dir.join("out.7z");
        let mut args = calculate_7z_compression_args(
            &src, &archive, Some("pw"), None, Some("1m"), CompressionLevel::Fast,
        );
        args.retain(|a| !a.starts_with("-mmt") && !a.starts_with("-md="));
        let run = |args: Vec<String>| {
            Command::new(&zz).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
                .status().unwrap().code().unwrap()
        };
        assert_eq!(run(args), 0, "compression");
        let first = dir.join("out.7z.001");
        assert!(dir.join("out.7z.003").exists(), "expected several volumes");

        assert_eq!(run(archive_test_args(&first, Some("pw"))), 0, "intact archive passes");
        assert_ne!(run(archive_test_args(&first, Some("wrong"))), 0, "wrong password fails");
        assert_ne!(run(archive_test_args(&first, None)), 0, "missing password fails, never prompts");

        let mut f = std::fs::OpenOptions::new().write(true).open(dir.join("out.7z.002")).unwrap();
        f.seek(SeekFrom::Start(4096)).unwrap();
        f.write_all(&[0xFF; 64]).unwrap();
        drop(f);
        assert_ne!(run(archive_test_args(&first, Some("pw"))), 0, "corrupt middle volume fails");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cancel_flag_survives_between_compress_and_test() {
        // Cancel with nothing running (the gap between compress and test) must
        // be remembered so the next 7-Zip run is skipped, until reset_cancel.
        let state = SevenZipRunnerState::new();
        state.cancelled.store(true, Ordering::SeqCst);
        assert!(state.cancelled.load(Ordering::SeqCst));
        state.reset_cancel();
        assert!(!state.cancelled.load(Ordering::SeqCst));
    }

    #[test]
    fn archive_test_args_always_supply_a_password() {
        let a = archive_test_args(Path::new("/o/x.7z.001"), Some("pw"));
        assert_eq!(a, vec!["t", "-ppw", "-bsp1", "/o/x.7z.001"]);
        let b = archive_test_args(Path::new("/o/x.7z"), None);
        assert_eq!(b[1], "-p", "empty -p so 7-Zip never prompts");
    }

    #[test]
    fn custom_args_cannot_inject_volume_flag() {
        let (accepted, rejected) = filter_custom_args("-mhe=off -v50m");
        assert!(accepted.contains(&"-mhe=off".to_string()));
        assert!(rejected.contains(&"-v50m".to_string()));
    }

    #[test]
    fn source_is_dir_contents_not_dir_itself() {
        // The source must be `<dir>/*` so 7-Zip archives the folder's contents
        // (root = steamapps\…) instead of nesting them under the long
        // `<Game>.Build.…` wrapper folder, which overflowed MAX_PATH.
        let args = calculate_7z_compression_args(
            Path::new("/tmp/out"),
            Path::new("/tmp/out.7z"),
            None,
            None,
            None,
            CompressionLevel::default(),
        );
        let source = args.last().expect("source arg present");
        let expected = Path::new("/tmp/out").join("*").to_string_lossy().to_string();
        assert_eq!(source, &expected);
        assert!(!args.iter().any(|a| a == "/tmp/out"), "bare dir must not be passed");
    }
}
