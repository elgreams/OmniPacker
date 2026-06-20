use std::io::{self, Write};
use tauri::State;

#[derive(Clone)]
pub struct DebugConsoleState {
    enabled: bool,
}

impl DebugConsoleState {
    pub fn new(enabled: bool) -> Self {
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
#[allow(dead_code)]
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
