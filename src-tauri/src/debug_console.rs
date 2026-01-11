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

#[tauri::command]
pub fn debug_console_enabled(state: State<DebugConsoleState>) -> bool {
    state.enabled
}

#[tauri::command]
pub fn debug_console_log(state: State<DebugConsoleState>, line: String) {
    state.write_line(&line);
}
