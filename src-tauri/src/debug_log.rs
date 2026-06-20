use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use tauri::AppHandle;

use crate::debug_console::debug_console_enabled_static;
use crate::output_dir::resolve_downloads_dir;

/// Resolves the logs directory as a sibling of the downloads directory,
/// creating it if necessary.
fn resolve_logs_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    let downloads = resolve_downloads_dir(app_handle).ok()?;
    let logs_dir = downloads.parent()?.join("logs");
    fs::create_dir_all(&logs_dir).ok()?;
    Some(logs_dir)
}

/// A diagnostic log file that only writes when the app was launched with
/// `--debug`. When debug mode is disabled (the default), no file is created
/// and every method is a cheap no-op, so call sites can log unconditionally
/// without guarding each call.
pub struct DebugLog {
    file: Option<File>,
}

impl DebugLog {
    /// Opens a log file named `{name}-{timestamp}.log` in the logs directory.
    /// Returns an inert (no-op) log unless debug mode is enabled.
    pub fn new(app_handle: &AppHandle, name: &str) -> Self {
        if !debug_console_enabled_static(app_handle) {
            return Self { file: None };
        }

        let file = resolve_logs_dir(app_handle).and_then(|dir| {
            let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            File::create(dir.join(format!("{name}-{timestamp}.log"))).ok()
        });

        Self { file }
    }

    /// Returns `true` when this log is actively writing to a file. Use this to
    /// skip building expensive log payloads (e.g. hex dumps) when disabled.
    pub fn is_active(&self) -> bool {
        self.file.is_some()
    }

    /// Writes a single line to the log, flushing immediately. A no-op when
    /// debug mode is disabled. Prefer the [`debug_log!`] macro at call sites.
    pub fn line(&mut self, args: std::fmt::Arguments<'_>) {
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "{args}");
            let _ = file.flush();
        }
    }
}

/// Writes a formatted line to a [`DebugLog`]. No-op when debug is disabled.
///
/// ```ignore
/// debug_log!(log, "Spawned pid {}", child.id());
/// ```
macro_rules! debug_log {
    ($log:expr, $($arg:tt)*) => {
        $log.line(format_args!($($arg)*))
    };
}

pub(crate) use debug_log;
