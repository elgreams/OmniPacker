use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::debug_console::DebugConsoleState;
use crate::output_dir::resolve_downloads_dir;

/// Resolves the logs directory as a sibling of the downloads directory.
fn resolve_logs_dir(app_handle: &AppHandle) -> Option<PathBuf> {
    let downloads = resolve_downloads_dir(app_handle).ok()?;
    let logs_dir = downloads.parent()?.join("logs");
    fs::create_dir_all(&logs_dir).ok()?;
    Some(logs_dir)
}

/// A debug log file writer. Only writes when --debug is active.
pub struct DebugLogFile {
    file: Option<Mutex<File>>,
}

impl DebugLogFile {
    /// Create a new debug log file. Returns a no-op writer if --debug is not set.
    pub fn new(app_handle: &AppHandle, name: &str) -> Self {
        let enabled = app_handle
            .try_state::<DebugConsoleState>()
            .map(|s| s.enabled())
            .unwrap_or(false);

        if !enabled {
            return Self { file: None };
        }

        let file = resolve_logs_dir(app_handle).and_then(|dir| {
            let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            let path = dir.join(format!("{name}-{timestamp}.log"));
            File::create(&path).ok()
        });

        Self {
            file: file.map(Mutex::new),
        }
    }

    /// Write raw bytes as hex dump + UTF-8 lossy preview.
    pub fn write_raw(&self, label: &str, bytes: &[u8]) {
        let Some(ref mutex) = self.file else { return };
        let Ok(mut f) = mutex.lock() else { return };
        let _ = write!(f, "[{label} {} bytes] ", bytes.len());
        for &b in bytes {
            let _ = write!(f, "{:02x} ", b);
        }
        let _ = writeln!(f);
        let _ = writeln!(f, "[{label} UTF8] {}", String::from_utf8_lossy(bytes));
        let _ = f.flush();
    }

    /// Write a decoded line.
    pub fn write_line(&self, label: &str, line: &str) {
        let Some(ref mutex) = self.file else { return };
        let Ok(mut f) = mutex.lock() else { return };
        let _ = writeln!(f, "[{label}] {line}");
        let _ = f.flush();
    }
}
