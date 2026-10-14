use serde::Serialize;
use tauri::{AppHandle, Emitter};

const DICTA_EVENT_NAME: &'static str = "dicta";

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UiEvent {
    DownloadProgress {
        model_id: String,
        downloaded: u64,
        total: Option<u64>,
    },
    Error {
        session: Option<u64>,
        message: String,
    },
    Final {
        session: u64,
        text: String,
    },
    ModelLoaded {
        source: String,
    },
}

pub fn emit_ui_event(app: &AppHandle, event_payload: UiEvent) {
    if let Err(error) = app.emit(DICTA_EVENT_NAME, event_payload) {
        eprintln!("failed to emit {DICTA_EVENT_NAME} event: {error}");
    }
}
