use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(target_os = "linux")]
use std::{
    ffi::OsStr,
    process::{Command, Stdio},
};

use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

fn ensure_writable_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|err| {
        format!(
            "Failed to create working directory {}: {err}",
            path.display()
        )
    })?;

    let test_path = path.join(".omnipacker_write_test");
    let write_result = OpenOptions::new()
        .write(true)
        .create(true)
        .open(&test_path);

    if let Ok(mut file) = write_result {
        let _ = std::fs::remove_file(&test_path);
        let _ = file.flush();
        Ok(())
    } else {
        Err(format!(
            "Working directory is not writable: {}",
            path.display()
        ))
    }
}

#[cfg(target_os = "linux")]
fn is_appimage_env() -> bool {
    std::env::var_os("APPIMAGE").is_some() || std::env::var_os("APPDIR").is_some()
}

#[cfg(target_os = "linux")]
fn is_kde_session() -> bool {
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        if desktop
            .split(':')
            .any(|entry| entry.eq_ignore_ascii_case("kde"))
        {
            return true;
        }
    }
    std::env::var_os("KDE_FULL_SESSION").is_some()
        || std::env::var_os("KDE_SESSION_VERSION").is_some()
}

#[cfg(target_os = "linux")]
fn run_sanitized_open(program: &str, args: &[&OsStr]) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    for key in [
        "HOME",
        "USER",
        "LOGNAME",
        "SHELL",
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "XAUTHORITY",
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_TYPE",
        "XDG_ACTIVATION_TOKEN",
        "DESKTOP_STARTUP_ID",
        "KDE_FULL_SESSION",
        "KDE_SESSION_VERSION",
        "LANG",
        "LC_ALL",
    ] {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }

    let status = cmd
        .status()
        .map_err(|err| format!("{program} failed to start: {err}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} exited with {status}"))
    }
}

#[cfg(target_os = "linux")]
fn open_path_appimage(path: &Path) -> Result<(), String> {
    let _ = path.metadata().map_err(|err| {
        format!(
            "Output folder does not exist: {} ({err})",
            path.display()
        )
    })?;
    let path_arg = path.as_os_str();

    let candidates = if is_kde_session() {
        vec![
            ("gio", vec![OsStr::new("open"), path_arg]),
            ("kioclient5", vec![OsStr::new("exec"), path_arg]),
            ("kioclient6", vec![OsStr::new("exec"), path_arg]),
            ("kde-open5", vec![path_arg]),
            ("kde-open6", vec![path_arg]),
            ("kde-open", vec![path_arg]),
            ("xdg-open", vec![path_arg]),
        ]
    } else {
        vec![
            ("gio", vec![OsStr::new("open"), path_arg]),
            ("xdg-open", vec![path_arg]),
            ("kde-open5", vec![path_arg]),
            ("kde-open6", vec![path_arg]),
            ("kde-open", vec![path_arg]),
            ("kioclient5", vec![OsStr::new("exec"), path_arg]),
            ("kioclient6", vec![OsStr::new("exec"), path_arg]),
        ]
    };

    let mut last_err = None;
    for (program, args) in candidates {
        match run_sanitized_open(program, &args) {
            Ok(()) => return Ok(()),
            Err(err) => last_err = Some(err),
        }
    }

    Err(match last_err {
        Some(err) => format!("{err} (path: {})", path.display()),
        None => format!("No opener succeeded (path: {})", path.display()),
    })
}

/// Marker file that, when present next to the executable, forces portable mode.
/// Portable mode keeps all data (working dirs, config, credentials) next to the
/// exe instead of in the OS app-data directory. The Windows portable zip ships
/// this file; the installers do not. A user can also drop it next to any copy to
/// make that copy portable.
const PORTABLE_MARKER: &str = ".portable";

/// The directory containing the running executable, if resolvable.
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
}

/// Whether the given directory contains the portable marker file.
fn dir_is_portable(dir: &Path) -> bool {
    dir.join(PORTABLE_MARKER).is_file()
}

/// Whether this build should run in portable mode. True only when a `.portable`
/// marker sits next to the executable. Always false in debug builds, where data
/// belongs in app-data for dev convenience regardless of any stray marker.
///
/// Detection is an explicit, intentional signal (the marker) rather than the old
/// "is the exe folder writable" heuristic, which misclassified installed builds
/// in writable locations as portable and portable builds in read-only locations
/// as installed — risky once credentials are routed through the same decision.
pub fn is_portable() -> bool {
    if cfg!(debug_assertions) {
        return false;
    }
    exe_dir().map(|dir| dir_is_portable(&dir)).unwrap_or(false)
}

/// Resolves the default base working directory (outputs/scratch live here when no
/// custom output folder is set — see `resolve_outputs_dir` / `resolve_scratch_dir`).
///
/// - Portable: the exe folder. If that isn't writable, fall back to app-data so a
///   job can still run — working output is not sensitive, so a lenient fallback is
///   acceptable here (unlike `resolve_config_dir`, which hard-fails).
/// - Installed / debug: the app-data directory.
pub fn resolve_base_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let fallback_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|err| format!("Failed to resolve app data directory: {err}"))?;

    if is_portable() {
        if let Some(dir) = exe_dir() {
            if ensure_writable_dir(&dir).is_ok() {
                return Ok(dir);
            }
        }
        // Portable but the exe folder isn't writable: fall through to app-data
        // rather than failing the job over a non-sensitive output location.
    }

    ensure_writable_dir(&fallback_dir)?;
    Ok(fallback_dir)
}

/// Resolves the directory that holds persistent config and credentials
/// (`login.dat`, `profiles/`, `output_dir.txt`).
///
/// - Portable: the exe folder. If that isn't writable this **hard-fails** rather
///   than silently falling back to app-data, so a portable user's saved
///   credentials are never written to the host machine they meant to keep clean.
/// - Installed / debug: the app-data directory (same location as before).
pub fn resolve_config_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if is_portable() {
        let dir = exe_dir().ok_or_else(|| {
            "Could not resolve the executable directory for portable mode.".to_string()
        })?;
        ensure_writable_dir(&dir).map_err(|err| {
            format!(
                "Portable mode is enabled but the program folder is not writable, \
                 so settings/credentials cannot be saved there: {err}"
            )
        })?;
        return Ok(dir);
    }

    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|err| format!("Failed to resolve app data directory: {err}"))?;
    ensure_writable_dir(&app_data_dir)?;
    Ok(app_data_dir)
}

/// Where finished `<Game>.Build.…` packages are written.
///
/// - Custom output folder set: the user's chosen directory *directly* — packages
///   land where they pointed, with no intermediate `outputs/` level (B2).
/// - Default: `<base>/outputs`.
///
/// Validates writability here (not just when the override is saved) so an
/// unplugged/removed drive fails the job loudly instead of silently reverting.
pub fn resolve_outputs_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Some(custom) = crate::output_override::read_override(app_handle) {
        ensure_writable_dir(&custom)?;
        return Ok(custom);
    }
    Ok(resolve_base_dir(app_handle)?.join("outputs"))
}

/// Where transient scratch (download staging, temp output assembly, debug logs)
/// is written. Kept separate from finished output so it can be hidden and
/// cleaned up independently.
///
/// - Custom output folder set: a hidden `.omnipacker-tmp/` *inside* the user's
///   folder, so the working set sits on the same volume as the final packages
///   (atomic finalize stays a rename) without visibly cluttering their folder.
/// - Default: `<base>` itself (staging/, logs/, and temp dirs hang directly off
///   it, matching the pre-override layout).
pub fn resolve_scratch_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Some(custom) = crate::output_override::read_override(app_handle) {
        let scratch = custom.join(".omnipacker-tmp");
        ensure_writable_dir(&scratch)?;
        return Ok(scratch);
    }
    resolve_base_dir(app_handle)
}

#[tauri::command]
pub fn get_output_folder(app_handle: AppHandle) -> Result<String, String> {
    let path = resolve_outputs_dir(&app_handle)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_output_folder(app_handle: AppHandle) -> Result<(), String> {
    // Finished packages land in the outputs directory (the user's custom folder
    // directly, or <base>/outputs by default).
    let outputs_dir = resolve_outputs_dir(&app_handle)?;
    // Create it if no job has produced output yet, so the open doesn't fail.
    std::fs::create_dir_all(&outputs_dir)
        .map_err(|err| format!("Failed to create outputs directory: {err}"))?;
    // Append a trailing separator so file managers open the folder's contents
    // instead of revealing/selecting it inside its parent directory.
    let path = with_trailing_separator(&outputs_dir);
    #[cfg(target_os = "linux")]
    if is_appimage_env() {
        return open_path_appimage(&path)
            .map_err(|err| format!("Failed to open output folder: {err}"));
    }
    app_handle
        .opener()
        .open_path(path.to_string_lossy().to_string(), None::<String>)
        .map_err(|err| format!("Failed to open output folder: {err}"))
}

fn with_trailing_separator(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(std::path::MAIN_SEPARATOR.to_string());
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates a unique empty temp directory without pulling in a test-only crate.
    fn unique_temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("omnipacker_test_{tag}_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn dir_is_portable_only_when_marker_present() {
        let dir = unique_temp_dir("marker");
        assert!(!dir_is_portable(&dir), "no marker => not portable");

        std::fs::write(dir.join(PORTABLE_MARKER), b"").unwrap();
        assert!(dir_is_portable(&dir), "marker file => portable");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dir_is_portable_ignores_marker_as_directory() {
        // A directory named `.portable` must not be treated as the marker file,
        // so an unrelated folder can't accidentally flip portable mode on.
        let dir = unique_temp_dir("markerdir");
        std::fs::create_dir_all(dir.join(PORTABLE_MARKER)).unwrap();
        assert!(!dir_is_portable(&dir), "marker as dir => not portable");

        std::fs::remove_dir_all(&dir).ok();
    }
}
