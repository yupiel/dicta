use std::sync::{Arc, Mutex, MutexGuard};

use serde::Deserialize;

use crate::stream::{StreamError, StreamSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Recording,
    Finalizing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecorderEvent {
    Phase(Phase),
    ModelLoaded {
        source: String,
    },
    Final {
        session: u64,
        text: String,
    },
    Error {
        session: Option<u64>,
        message: String,
    },
}

pub type EventSink = Arc<dyn Fn(RecorderEvent) + Send + Sync>;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StartOptions {
    pub device_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RecorderError {
    #[error("no model loaded")]
    NoModel,
    #[error("a recording is still active or being processed")]
    Busy(Phase),
    #[error("no recording in progress")]
    NotRecording,
    #[error("cannot start audio capture: {0}")]
    StreamSource(StreamError),
}

struct Active {
    session: u64,
    stop: Box<dyn Send>,
}

struct RecorderState {
    phase: Phase,
    // TODO: figure out how to push the model in here
    model: Option<()>,

    // MAYBE: Not sure if i'll need this yet but maybe for some kind of history if i want a previous transcribed text?
    active: Option<Active>,
    last_session: u64,
}

struct Shared {
    state: Mutex<RecorderState>,
    source: Arc<dyn StreamSource>,
    sink: EventSink,
}

impl Shared {
    // The state is plain data, so a panic elsewhere must not lock the app out.
    fn lock(&self) -> MutexGuard<'_, RecorderState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn emit(&self, event: RecorderEvent) {
        (self.sink)(event);
    }
}

#[derive(Clone)]
pub struct Recorder {
    shared: Arc<Shared>,
}

impl Recorder {
    pub fn new(sink: EventSink, source: Arc<dyn StreamSource>) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(RecorderState {
                    phase: Phase::Idle,
                    model: None,
                    active: None,
                    last_session: 0,
                }),
                source,
                sink,
            }),
        }
    }

    pub fn phase(&self) -> Phase {
        self.shared.lock().phase
    }

    pub fn has_model(&self) -> bool {
        self.shared.lock().model.is_some()
    }

    pub fn start(&self, options: StartOptions) -> Result<u64, RecorderError> {
        let session_id = {
            let mut state = self.shared.lock();
            if state.phase != Phase::Idle {
                return Err(RecorderError::Busy(state.phase));
            }

            let mut model = state.model.clone().ok_or(RecorderError::NoModel)?;

            let input_stream = self
                .shared
                .source
                .start(options.device_id.as_deref())
                .map_err(RecorderError::StreamSource)?;

            state.last_session += 1;
            
            todo!("Handle segmentation and transcription");
        };

        self.shared.emit(RecorderEvent::Phase(Phase::Recording));
        Ok(session_id)
    }

    /// Returns at once; the remaining transcription is reported through events.
    pub fn stop(&self) -> Result<(), RecorderError> {
        let active = {
            let mut state = self.shared.lock();
            let active = state.active.take().ok_or(RecorderError::NotRecording)?;
            state.phase = Phase::Finalizing;
            active
        };
        self.shared.emit(RecorderEvent::Phase(Phase::Finalizing));

        Ok(())
    }
}