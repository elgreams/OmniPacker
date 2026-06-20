use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::State;

/// Process-wide debug flag, set once at startup from `--debug`. Lets modules
/// without access to Tauri's managed `DebugConsoleState` (e.g. the pure
/// `steamdb_api` / `steamcmd_api` clients) check whether debug output is on.
static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

/// Returns whether the app was launched with `--debug`. Usable from any module,
/// including ones with no `AppHandle`.
pub fn debug_enabled() -> bool {
    DEBUG_ENABLED.load(Ordering::Relaxed)
}

/// Like `eprintln!`, but only prints when the app was launched with `--debug`.
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        if $crate::debug_console::debug_enabled() {
            eprintln!($($arg)*);
        }
    };
}

pub(crate) use debug_eprintln;

#[derive(Clone)]
pub struct DebugConsoleState {
    enabled: bool,
}

impl DebugConsoleState {
    pub fn new(enabled: bool) -> Self {
        DEBUG_ENABLED.store(enabled, Ordering::Relaxed);
        Self { enabled }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn write_line(&self, line: &str) {
        if !self.enabled {
            return;
        }
        let mut stdout = io::stdout();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }
}

/// Check if debug console is enabled from an AppHandle (non-command context).
pub fn debug_console_enabled_static(app_handle: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    app_handle.state::<DebugConsoleState>().enabled()
}

#[tauri::command]
pub fn debug_console_enabled(state: State<DebugConsoleState>) -> bool {
    state.enabled
}

#[tauri::command]
pub fn debug_console_log(state: State<DebugConsoleState>, line: String) {
    state.write_line(&line);
}
