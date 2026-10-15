use std::{
    sync::{Arc, Mutex, MutexGuard},
    thread,
};

use serde::{Deserialize, Serialize};

use crate::{
    auto::AutoConfig, pipeline::{
        self, DEFAULT_MAX_MERGED_PREVIEW_MS, Pipeline, PipelineConfig, SharedTranscriber, Transcriber,
    }, segmenter::{CutReason, SegmenterConfig}, stream::{StreamError, StreamSource},
};

#[derive(Debug, Clone, Serialize, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
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
}

pub type EventSink = Arc<dyn Fn(RecorderEvent) + Send + Sync>;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StartOptions {
    pub device_id: Option<String>,
    /// Whisper language code such as "de"; `None` auto-detects.
    pub language: Option<String>,
    pub pause_ms: Option<u32>,
    pub max_segment_ms: Option<u32>,
    /// Enables auto mode: a pause this long ends the utterance, and recording continues until stopped.
    pub auto_pause_ms: Option<u32>,
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
    pipeline: Pipeline,
}

struct RecorderState {
    phase: Phase,
    model: Option<SharedTranscriber>,
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

    /// Takes effect from the next recording on; a running one keeps its model.
    pub fn set_model(&self, transcriber: impl Transcriber, source: impl Into<String>) {
        self.shared.lock().model = Some(SharedTranscriber::new(transcriber));
        self.shared.emit(RecorderEvent::ModelLoaded {
            source: source.into(),
        });
    }

    pub fn start(&self, options: StartOptions) -> Result<u64, RecorderError> {
        let session = {
            let mut state = self.shared.lock();
            if state.phase != Phase::Idle {
                return Err(RecorderError::Busy(state.phase));
            }
            let mut model = state.model.clone().ok_or(RecorderError::NoModel)?;
            model.set_language(options.language.as_deref());

            let stream = self
                .shared
                .source
                .start(options.device_id.as_deref())
                .map_err(RecorderError::StreamSource)?;

            state.last_session += 1;
            let session = state.last_session;
            let defaults = SegmenterConfig::default();
            let config = PipelineConfig {
                input_sample_rate: stream.format.sample_rate,
                input_channels: stream.format.channels,
                segmenter: SegmenterConfig {
                    pause_ms: options.pause_ms.unwrap_or(defaults.pause_ms),
                    max_segment_ms: options.max_segment_ms.unwrap_or(defaults.max_segment_ms),
                    ..defaults
                },
                max_merged_preview_ms: DEFAULT_MAX_MERGED_PREVIEW_MS,
                auto: options.auto_pause_ms.map(AutoConfig::new),
            };

            let sink = Arc::clone(&self.shared.sink);
            let pipeline = Pipeline::start(config, stream.audio, model, move |event| {
                sink(translate(session, event));
            });

            state.active = Some(Active {
                session,
                stop: stream.stop,
                pipeline,
            });
            state.phase = Phase::Recording;
            session
        };

        self.shared.emit(RecorderEvent::Phase(Phase::Recording));
        Ok(session)
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

        let shared = Arc::clone(&self.shared);
        thread::spawn(move || {
            let Active {
                session,
                stop,
                pipeline,
            } = active;
            pipeline.stop_previews();
            drop(stop);
            if pipeline.finish().is_err() {
                shared.emit(RecorderEvent::Error {
                    session: Some(session),
                    message: "the transcription worker crashed".to_owned(),
                });
            }
            shared.lock().phase = Phase::Idle;
            shared.emit(RecorderEvent::Phase(Phase::Idle));
        });
        Ok(())
    }
}

fn translate(session: u64, event: pipeline::Event) -> RecorderEvent {
    match event {
        pipeline::Event::Preview {
            index,
            duration_ms,
            reason,
            text,
            ..
        } => RecorderEvent::Preview {
            session,
            index,
            text,
            duration_ms,
            reason,
        },
        pipeline::Event::UtteranceStarted => RecorderEvent::UtteranceStarted { session },
        pipeline::Event::UtteranceEnded => RecorderEvent::UtteranceEnded { session },
        pipeline::Event::Final { text } => RecorderEvent::Final { session, text },
        pipeline::Event::Failed { message } => RecorderEvent::Error {
            session: Some(session),
            message,
        },
    }
}
