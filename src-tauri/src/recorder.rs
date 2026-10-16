use std::{
    sync::{Arc, Mutex, MutexGuard},
    thread,
};

use serde::{Deserialize, Serialize};

use crate::{
    auto::AutoConfig,
    pipeline::{
        self, DEFAULT_MAX_MERGED_PREVIEW_MS, Pipeline, PipelineConfig, SharedTranscriber,
        Transcriber,
    },
    segmenter::{CutReason, SegmenterConfig},
    stream::{StreamError, StreamSource},
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::thread::JoinHandle;
    use std::time::Duration;

    use super::*;
    use crate::pipeline::TranscribeError;
    use crate::stream::{AudioFormat, AudioStream};

    const TIMEOUT: Duration = Duration::from_secs(10);

    #[derive(Default)]
    struct LengthTranscriber {
        language: Arc<Mutex<Option<String>>>,
    }

    impl Transcriber for LengthTranscriber {
        fn transcribe(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
            Ok(samples.len().to_string())
        }

        fn set_language(&mut self, language: Option<&str>) {
            *self.language.lock().unwrap() = language.map(str::to_owned);
        }
    }

    struct StopGuard {
        stop: Option<Sender<()>>,
        thread: Option<JoinHandle<()>>,
    }

    impl Drop for StopGuard {
        fn drop(&mut self) {
            drop(self.stop.take());
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    struct ScriptedSource {
        signal: Vec<f32>,
    }

    impl StreamSource for ScriptedSource {
        fn start(&self, _: Option<&str>) -> Result<AudioStream, StreamError> {
            let (audio_tx, audio_rx) = mpsc::sync_channel(64);
            let (stop_tx, stop_rx) = mpsc::channel::<()>();
            let signal = self.signal.clone();
            let thread = thread::spawn(move || {
                for chunk in signal.chunks(960) {
                    if audio_tx.send(chunk.to_vec()).is_err() {
                        return;
                    }
                }
                let _ = stop_rx.recv();
            });
            Ok(AudioStream {
                audio: audio_rx,
                format: AudioFormat {
                    sample_rate: 48_000,
                    channels: 2,
                },
                stop: Box::new(StopGuard {
                    stop: Some(stop_tx),
                    thread: Some(thread),
                }),
            })
        }
    }

    struct BrokenSource;

    impl StreamSource for BrokenSource {
        fn start(&self, _: Option<&str>) -> Result<AudioStream, StreamError> {
            Err("no microphone".into())
        }
    }

    fn one_utterance() -> Vec<f32> {
        let rate = 48_000;
        let silence = std::iter::repeat_n(0.0, rate);
        let tone = (0..2 * rate)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin());
        let mono: Vec<f32> = silence.clone().chain(tone).chain(silence).collect();
        mono.iter().flat_map(|&sample| [sample, sample]).collect()
    }

    fn recorder_with(source: impl StreamSource) -> (Recorder, Receiver<RecorderEvent>) {
        let (tx, rx) = mpsc::channel();
        let sink: EventSink = Arc::new(move |event| {
            let _ = tx.send(event);
        });
        (Recorder::new(sink, Arc::new(source)), rx)
    }

    fn collect_until(
        rx: &Receiver<RecorderEvent>,
        done: impl Fn(&RecorderEvent) -> bool,
    ) -> Vec<RecorderEvent> {
        let mut events = Vec::new();
        loop {
            let event = rx
                .recv_timeout(TIMEOUT)
                .unwrap_or_else(|_| panic!("timed out, got {events:?}"));
            let finished = done(&event);
            events.push(event);
            if finished {
                return events;
            }
        }
    }

    fn is_idle(event: &RecorderEvent) -> bool {
        *event == RecorderEvent::Phase(Phase::Idle)
    }

    fn kinds(events: &[RecorderEvent]) -> Vec<&'static str> {
        events
            .iter()
            .map(|event| match event {
                RecorderEvent::Phase(Phase::Idle) => "idle",
                RecorderEvent::Phase(Phase::Recording) => "recording",
                RecorderEvent::Phase(Phase::Finalizing) => "finalizing",
                RecorderEvent::ModelLoaded { .. } => "modelLoaded",
                RecorderEvent::Preview { .. } => "preview",
                RecorderEvent::UtteranceStarted { .. } => "utteranceStarted",
                RecorderEvent::UtteranceEnded { .. } => "utteranceEnded",
                RecorderEvent::Final { .. } => "final",
                RecorderEvent::Error { .. } => "error",
            })
            .collect()
    }

    fn record_one_utterance(recorder: &Recorder, rx: &Receiver<RecorderEvent>) -> Vec<RecorderEvent> {
        let mut events = collect_until(rx, |e| matches!(e, RecorderEvent::Preview { .. }));
        recorder.stop().unwrap();
        events.extend(collect_until(rx, is_idle));
        events
    }

    #[test]
    fn starting_without_a_model_fails() {
        let (recorder, _rx) = recorder_with(ScriptedSource {
            signal: one_utterance(),
        });
        assert!(matches!(
            recorder.start(StartOptions::default()),
            Err(RecorderError::NoModel)
        ));
        assert_eq!(recorder.phase(), Phase::Idle);
    }

    #[test]
    fn stopping_while_idle_fails() {
        let (recorder, _rx) = recorder_with(ScriptedSource {
            signal: one_utterance(),
        });
        assert!(matches!(recorder.stop(), Err(RecorderError::NotRecording)));
    }

    #[test]
    fn a_session_reports_phases_preview_and_final_in_order() {
        let (recorder, rx) = recorder_with(ScriptedSource {
            signal: one_utterance(),
        });
        recorder.set_model(LengthTranscriber::default(), "test-model");
        assert!(recorder.has_model());

        assert_eq!(recorder.start(StartOptions::default()).unwrap(), 1);
        let events = record_one_utterance(&recorder, &rx);

        assert_eq!(
            kinds(&events),
            [
                "modelLoaded",
                "recording",
                "preview",
                "finalizing",
                "final",
                "idle"
            ]
        );
        assert_eq!(recorder.phase(), Phase::Idle);
    }

    #[test]
    fn the_model_loaded_event_is_reported() {
        let (recorder, rx) = recorder_with(BrokenSource);
        recorder.set_model(LengthTranscriber::default(), "ggml-small.bin");
        assert_eq!(
            rx.try_recv().unwrap(),
            RecorderEvent::ModelLoaded {
                source: "ggml-small.bin".to_owned()
            }
        );
    }

    #[test]
    fn starting_twice_is_rejected_while_recording() {
        let (recorder, rx) = recorder_with(ScriptedSource {
            signal: one_utterance(),
        });
        recorder.set_model(LengthTranscriber::default(), "test-model");
        recorder.start(StartOptions::default()).unwrap();

        assert!(matches!(
            recorder.start(StartOptions::default()),
            Err(RecorderError::Busy(Phase::Recording))
        ));

        record_one_utterance(&recorder, &rx);
    }

    #[test]
    fn a_new_session_can_start_once_the_previous_one_is_idle() {
        let (recorder, rx) = recorder_with(ScriptedSource {
            signal: one_utterance(),
        });
        recorder.set_model(LengthTranscriber::default(), "test-model");

        recorder.start(StartOptions::default()).unwrap();
        record_one_utterance(&recorder, &rx);

        assert_eq!(recorder.start(StartOptions::default()).unwrap(), 2);
        record_one_utterance(&recorder, &rx);
    }

    #[test]
    fn a_failing_audio_source_leaves_the_recorder_idle_and_silent() {
        let (recorder, rx) = recorder_with(BrokenSource);
        recorder.set_model(LengthTranscriber::default(), "test-model");
        rx.try_recv().unwrap();

        assert!(matches!(
            recorder.start(StartOptions::default()),
            Err(RecorderError::StreamSource(_))
        ));
        assert_eq!(recorder.phase(), Phase::Idle);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn the_requested_language_reaches_the_model() {
        let language = Arc::new(Mutex::new(None));
        let (recorder, rx) = recorder_with(ScriptedSource {
            signal: one_utterance(),
        });
        recorder.set_model(
            LengthTranscriber {
                language: Arc::clone(&language),
            },
            "test-model",
        );

        recorder
            .start(StartOptions {
                language: Some("de".to_owned()),
                ..StartOptions::default()
            })
            .unwrap();
        record_one_utterance(&recorder, &rx);

        assert_eq!(language.lock().unwrap().as_deref(), Some("de"));
    }

    fn utterance_then_long_silence() -> Vec<f32> {
        let rate = 48_000;
        let tone = (0..2 * rate)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin());
        let mono: Vec<f32> = tone.chain(std::iter::repeat_n(0.0, 4 * rate)).collect();
        mono.iter().flat_map(|&sample| [sample, sample]).collect()
    }

    #[test]
    fn auto_mode_reports_the_text_while_recording_continues() {
        let (recorder, rx) = recorder_with(ScriptedSource {
            signal: utterance_then_long_silence(),
        });
        recorder.set_model(LengthTranscriber::default(), "test-model");

        recorder
            .start(StartOptions {
                auto_pause_ms: Some(2_000),
                ..StartOptions::default()
            })
            .unwrap();
        let mut events = collect_until(&rx, |e| matches!(e, RecorderEvent::Final { .. }));
        assert_eq!(recorder.phase(), Phase::Recording);

        recorder.stop().unwrap();
        events.extend(collect_until(&rx, is_idle));

        assert_eq!(
            kinds(&events),
            [
                "modelLoaded",
                "recording",
                "utteranceEnded",
                "final",
                "finalizing",
                "idle"
            ]
        );
    }
}
