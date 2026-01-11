use serde::Serialize;
use std::{
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::debug_console::DebugConsoleState;

#[derive(Clone)]
pub struct SevenZipRunnerState {
    child: Arc<Mutex<Option<Child>>>,
}

impl SevenZipRunnerState {
    pub fn new() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
        }
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
pub fn run_7zip(
    app_handle: AppHandle,
    state: State<'_, SevenZipRunnerState>,
    args: String,
) -> Result<(), String> {
    let mut guard = state
        .child
        .lock()
        .map_err(|_| "Failed to lock 7-Zip state".to_string())?;

    if guard.is_some() {
        return Err("7-Zip is already running".to_string());
    }

    emit_status(&app_handle, "starting", None);

    let path = match resolve_7zip_path(&app_handle) {
        Ok(path) => path,
        Err(err) => {
            emit_status(&app_handle, "error", None);
            return Err(err);
        }
    };

    let mut command = Command::new(&path);
    let arg_list = args.split_whitespace().collect::<Vec<_>>();
    if !arg_list.is_empty() {
        command.args(arg_list);
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    // Hide console window on Windows
    #[cfg(windows)]
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            emit_status(&app_handle, "error", None);
            return Err(format!("Failed to spawn 7-Zip: {err}"));
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    *guard = Some(child);
    drop(guard);

    emit_status(&app_handle, "running", None);

    if let Some(stream) = stdout {
        spawn_log_reader(app_handle.clone(), stream, "stdout");
    }

    if let Some(stream) = stderr {
        spawn_log_reader(app_handle.clone(), stream, "stderr");
    }

    let state_handle = state.child.clone();
    let app_handle_clone = app_handle.clone();

    thread::spawn(move || loop {
        let status = {
            let mut lock = match state_handle.lock() {
                Ok(lock) => lock,
                Err(_) => {
                    emit_status(&app_handle_clone, "error", None);
                    return;
                }
            };

            let Some(child) = lock.as_mut() else {
                return;
            };

            match child.try_wait() {
                Ok(Some(status)) => {
                    *lock = None;
                    Some(status)
                }
                Ok(None) => None,
                Err(err) => {
                    *lock = None;
                    emit_status(&app_handle_clone, "error", None);
                    eprintln!("Failed to wait on 7-Zip: {err}");
                    return;
                }
            }
        };

        if let Some(status) = status {
            emit_status(&app_handle_clone, "exited", status.code());
            return;
        }

        thread::sleep(Duration::from_millis(100));
    });

    Ok(())
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

    let Some(child) = guard.as_mut() else {
        return Err("7-Zip is not running".to_string());
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
) -> Result<i32, String> {
    let mut guard = state
        .child
        .lock()
        .map_err(|_| "Failed to lock 7-Zip state".to_string())?;

    if guard.is_some() {
        return Err("7-Zip is already running".to_string());
    }

    let path = resolve_7zip_path(app_handle)?;

    let mut command = Command::new(&path);
    command.args(&args);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

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
                return Ok(-1);
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
            return Ok(code);
        }

        thread::sleep(Duration::from_millis(100));
    }
}

/// Calculates optimal 7-Zip compression arguments based on CPU cores.
/// Prioritizes smallest file size with `-mx9` (ultra compression).
/// Thread count is adapted to prevent system lockup on weak hardware.
pub fn calculate_7z_compression_args(
    source_dir: &std::path::Path,
    output_archive: &std::path::Path,
    password: Option<&str>,
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

    let total_bytes = system.total_memory().saturating_mul(1024);
    let available_bytes = system.available_memory().saturating_mul(1024);
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
        1 * GB
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
        let max_dict_bytes = per_thread_budget / 3;
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
        "-mx9".to_string(),                                 // Ultra compression level
        format!("-mmt{}", threads),                         // Multi-threading
        format!("-md={}", dict_label),                      // Dictionary size tuned by resources
        "-bsp1".to_string(),                                // Progress output to stdout
    ];

    if let Some(password) = password {
        if !password.is_empty() {
            args.push(format!("-p{}", password));
        }
    }

    args.push(output_archive.to_string_lossy().to_string()); // Archive path
    args.push(source_dir.to_string_lossy().to_string()); // Source directory
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

    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buffer = [0u8; 1024];
        let mut current_line = String::new();
        let mut last_percent: Option<u8> = None;
        let mut last_was_cr = false;

        loop {
            let n = match reader.read(&mut buffer) {
                Ok(n) => n,
                Err(_) => break,
            };

            if n == 0 {
                break;
            }

            let chunk = String::from_utf8_lossy(&buffer[..n]).to_string();
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
