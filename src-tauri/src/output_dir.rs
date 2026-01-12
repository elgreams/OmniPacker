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
            "Failed to create downloads directory {}: {err}",
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
            "Downloads directory is not writable: {}",
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

pub fn resolve_downloads_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let fallback_dir = app_handle
        .path()
        .resolve("downloads", tauri::path::BaseDirectory::AppData)
        .map_err(|err| format!("Failed to resolve app data downloads directory: {err}"))?;

    if cfg!(debug_assertions) {
        ensure_writable_dir(&fallback_dir)?;
        return Ok(fallback_dir);
    }

    let portable_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("downloads")));

    if let Some(dir) = portable_dir {
        if ensure_writable_dir(&dir).is_ok() {
            return Ok(dir);
        }
    }

    ensure_writable_dir(&fallback_dir)?;
    Ok(fallback_dir)
}

#[tauri::command]
pub fn get_output_folder(app_handle: AppHandle) -> Result<String, String> {
    let path = resolve_downloads_dir(&app_handle)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_output_folder(app_handle: AppHandle) -> Result<(), String> {
    let path = resolve_downloads_dir(&app_handle)?;
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
