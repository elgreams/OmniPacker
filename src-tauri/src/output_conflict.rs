use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{mpsc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputConflictChoice {
    Overwrite,
    Copy,
    Cancel,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OutputConflictPayload {
    job_id: String,
    output_path: String,
    output_name: String,
}

pub struct OutputConflictState {
    pending: Mutex<HashMap<String, mpsc::Sender<OutputConflictChoice>>>,
}

impl OutputConflictState {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
        }
    }
}

pub fn request_output_conflict_resolution(
    app_handle: &AppHandle,
    job_id: &str,
    output_path: &Path,
) -> Result<OutputConflictChoice, String> {
    let output_name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output")
        .to_string();
    let output_path_display = output_path.to_string_lossy().to_string();

    let (sender, receiver) = mpsc::channel();

    {
        let state = app_handle.state::<OutputConflictState>();
        let mut pending = state
            .pending
            .lock()
            .map_err(|_| "Failed to lock output conflict state".to_string())?;

        if pending.contains_key(job_id) {
            return Err("Output conflict resolution already pending".to_string());
        }
        pending.insert(job_id.to_string(), sender);
    }

    if let Err(err) = app_handle.emit(
        "dd:output_conflict",
        OutputConflictPayload {
            job_id: job_id.to_string(),
            output_path: output_path_display,
            output_name,
        },
    ) {
        let state = app_handle.state::<OutputConflictState>();
        let mut pending = state
            .pending
            .lock()
            .map_err(|_| "Failed to lock output conflict state".to_string())?;
        pending.remove(job_id);
        return Err(format!("Failed to emit output conflict prompt: {err}"));
    }

    receiver
        .recv()
        .map_err(|_| "Output conflict resolution channel closed".to_string())
}

#[tauri::command]
pub fn resolve_output_conflict(
    state: State<'_, OutputConflictState>,
    job_id: String,
    choice: OutputConflictChoice,
) -> Result<(), String> {
    let sender = {
        let mut pending = state
            .pending
            .lock()
            .map_err(|_| "Failed to lock output conflict state".to_string())?;
        pending.remove(&job_id)
    };

    let Some(sender) = sender else {
        return Err("No pending output conflict for this job".to_string());
    };

    sender
        .send(choice)
        .map_err(|_| "Failed to deliver output conflict choice".to_string())
}
