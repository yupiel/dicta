use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::{
    history::{HistoryEntry, HistoryStore},
    live_text::LiveTextStore,
    recorder::{Phase, RecorderEvent},
    segmenter::CutReason,
};

const DICTA_EVENT_NAME: &'static str = "dicta";

#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UiEvent {
    Phase {
        phase: Phase,
    },
    ModelLoaded {
        source: String,
    },
    Preview {
        session: u64,
        index: usize,
        text: String,
        duration_ms: u64,
        reason: CutReason,
    },
    UtteranceStarted {
        session: u64,
    },
    UtteranceEnded {
        session: u64,
    },
    Final {
        session: u64,
        text: String,
    },
    Error {
        session: Option<u64>,
        message: String,
    },
    HistorySaved {
        entry: HistoryEntry,
    },
    DownloadProgress {
        model_id: String,
        downloaded: u64,
        total: Option<u64>,
    },
}

impl From<RecorderEvent> for UiEvent {
    fn from(event: RecorderEvent) -> Self {
        match event {
            RecorderEvent::Phase(phase) => Self::Phase {
                phase: phase.into(),
            },
            RecorderEvent::ModelLoaded { source } => Self::ModelLoaded { source },
            RecorderEvent::Preview {
                session,
                index,
                text,
                duration_ms,
                reason,
            } => Self::Preview {
                session,
                index,
                text,
                duration_ms,
                reason: reason.into(),
            },
            RecorderEvent::UtteranceStarted { session } => Self::UtteranceStarted { session },
            RecorderEvent::UtteranceEnded { session } => Self::UtteranceEnded { session },
            RecorderEvent::Final { session, text } => Self::Final { session, text },
            RecorderEvent::Error { session, message } => Self::Error { session, message },
        }
    }
}

pub fn emit_ui_event(app: &AppHandle, event_payload: UiEvent) {
    if let Err(error) = app.emit(DICTA_EVENT_NAME, event_payload) {
        eprintln!("failed to emit {DICTA_EVENT_NAME} event: {error}");
    }
}

pub fn handle_recorder_event(
    app: &AppHandle,
    history: &HistoryStore,
    live_text: &LiveTextStore,
    event: RecorderEvent,
) {
    live_text.apply(&event);

    let saved = match &event {
        RecorderEvent::Final { session, text } => Some((*session, history.add(*session, text))),
        _ => None,
    };

    emit_ui_event(app, event.into());

    match saved {
        Some((_, Ok(Some(entry)))) => emit_ui_event(
            app,
            UiEvent::HistorySaved {
                entry: entry.into(),
            },
        ),
        Some((session, Err(error))) => emit_ui_event(
            app,
            UiEvent::Error {
                session: Some(session),
                message: error.to_string(),
            },
        ),
        _ => {}
    }
}
