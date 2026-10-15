use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::auto::{AutoConfig, Waiter};
use crate::resample::{downmix_to_mono, Resampler};
use crate::segmenter::{ms_to_samples, CutReason, Segment, Segmenter, SegmenterConfig};
use crate::whisper::WHISPER_SAMPLE_RATE;

pub type TranscribeError = Box<dyn std::error::Error + Send + Sync>;

pub trait Transcriber: Send + 'static {
    /// `samples` are 16 kHz mono f32.
    fn transcribe(&mut self, samples: &[f32]) -> Result<String, TranscribeError>;

    /// Like `transcribe`, but may trade accuracy for speed. Used for live previews only.
    fn transcribe_preview(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
        self.transcribe(samples)
    }

    /// `None` lets the model detect the language itself.
    fn set_language(&mut self, _language: Option<&str>) {}
}

// whisper.cpp pads every input to a 30 s window, so one call costs about the same for 3 s or 28 s
// of audio. Merging queued previews into a single call is what lets previews catch up after a
// backlog; the margin keeps the merged audio inside that one window.
pub const DEFAULT_MAX_MERGED_PREVIEW_MS: u32 = 28000;

/// Lets one loaded model serve many recordings, since loading it takes seconds.
#[derive(Clone)]
pub struct SharedTranscriber(Arc<Mutex<Box<dyn Transcriber>>>);

impl SharedTranscriber {
    pub fn new(transcriber: impl Transcriber) -> Self {
        Self(Arc::new(Mutex::new(Box::new(transcriber))))
    }
}

impl Transcriber for SharedTranscriber {
    fn transcribe(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
        let mut inner = self.0.lock().map_err(|_| "transcriber lock poisoned")?;
        inner.transcribe(samples)
    }

    fn transcribe_preview(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
        let mut inner = self.0.lock().map_err(|_| "transcriber lock poisoned")?;
        inner.transcribe_preview(samples)
    }

    fn set_language(&mut self, language: Option<&str>) {
        if let Ok(mut inner) = self.0.lock() {
            inner.set_language(language);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Preview {
        index: usize,
        duration_ms: u64,
        reason: CutReason,
        text: String,
    },
    UtteranceStarted,
    UtteranceEnded,
    Final {
        text: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub input_sample_rate: u32,
    pub input_channels: u16,
    pub segmenter: SegmenterConfig,
    /// Queued previews are merged into one transcription up to this much audio; 0 disables merging.
    pub max_merged_preview_ms: u32,
    /// Ends an utterance after a long pause and keeps listening for the next one. Previews are
    /// off in this mode: they would queue ahead of the final transcription and delay it.
    pub auto: Option<AutoConfig>,
}

enum Job {
    Preview { index: usize, segment: Segment },
    UtteranceStarted,
    UtteranceEnded,
    Final(Vec<f32>),
}

pub struct Pipeline {
    processing: JoinHandle<()>,
    transcription: JoinHandle<()>,
    finalizing: Arc<AtomicBool>,
}

impl Pipeline {
    /// Runs until `audio` is closed (all senders dropped), then transcribes the recording once
    /// in full and emits `Event::Final`. With `auto` set, every long pause emits `Final` for the utterance so far
    /// and the pipeline waits for speech; the next one is announced by `Event::UtteranceStarted`.
    pub fn start<T, F>(
        config: PipelineConfig,
        audio: Receiver<Vec<f32>>,
        transcriber: T,
        on_event: F,
    ) -> Self
    where
        T: Transcriber,
        F: FnMut(Event) + Send + 'static,
    {
        let (job_tx, job_rx) = mpsc::channel();
        let max_merged_preview_samples =
            config.max_merged_preview_ms as usize * WHISPER_SAMPLE_RATE as usize / 1000;
        let finalizing = Arc::new(AtomicBool::new(false));
        let processing = {
            let finalizing = Arc::clone(&finalizing);
            thread::spawn(move || run_processing(config, audio, job_tx, &finalizing))
        };
        let transcription = {
            let finalizing = Arc::clone(&finalizing);
            thread::spawn(move || {
                run_transcription(
                    transcriber,
                    on_event,
                    max_merged_preview_samples,
                    job_rx,
                    &finalizing,
                );
            })
        };
        Self {
            processing,
            transcription,
            finalizing,
        }
    }

    /// Abandons previews that are queued or not yet cut, so the full transcription starts as
    /// soon as the model is free. A preview already inside the model cannot be interrupted;
    /// its result is dropped.
    pub fn stop_previews(&self) {
        self.finalizing.store(true, Ordering::Release);
    }

    pub fn finish(self) -> thread::Result<()> {
        let processing = self.processing.join();
        let transcription = self.transcription.join();
        processing.and(transcription)
    }
}

enum Stage {
    Speaking,
    Waiting(Box<Waiter>),
}

struct Processor<'a> {
    segmenter_config: SegmenterConfig,
    auto: Option<AutoConfig>,
    jobs: Sender<Job>,
    finalizing: &'a AtomicBool,
    segmenter: Segmenter,
    stage: Stage,
    recording: Vec<f32>,
    next_index: usize,
}

impl<'a> Processor<'a> {
    fn new(config: PipelineConfig, jobs: Sender<Job>, finalizing: &'a AtomicBool) -> Self {
        Self {
            segmenter: Segmenter::new(config.segmenter.clone()),
            segmenter_config: config.segmenter,
            auto: config.auto,
            jobs,
            finalizing,
            stage: Stage::Speaking,
            recording: Vec::new(),
            next_index: 0,
        }
    }

    fn submit(&mut self, segment: Segment) -> bool {
        let job = Job::Preview {
            index: self.next_index,
            segment,
        };
        self.next_index += 1;
        self.jobs.send(job).is_ok()
    }

    fn submit_all(&mut self, segments: Vec<Segment>) -> bool {
        if self.auto.is_some() {
            return true;
        }
        segments.into_iter().all(|segment| self.submit(segment))
    }

    /// Returns false once the transcription side is gone and further work is pointless.
    fn feed(&mut self, samples: &[f32]) -> bool {
        let finalizing = self.finalizing.load(Ordering::Acquire);
        match &mut self.stage {
            Stage::Speaking => {
                self.recording.extend_from_slice(samples);
                if finalizing {
                    return true;
                }
                let segments = self.segmenter.push(samples);
                if !self.submit_all(segments) {
                    return false;
                }
                self.end_utterance_after_long_pause()
            }
            Stage::Waiting(waiter) => {
                if finalizing {
                    return true;
                }
                match waiter.push(samples) {
                    Some(audio) => self.begin_utterance(audio),
                    None => true,
                }
            }
        }
    }

    fn end_utterance_after_long_pause(&mut self) -> bool {
        let Some(auto) = &self.auto else {
            return true;
        };
        let pause_len = ms_to_samples(auto.pause_ms, self.segmenter_config.sample_rate);
        let Some(silence) = self
            .segmenter
            .silence_since_speech()
            .filter(|silence| *silence >= pause_len)
        else {
            return true;
        };

        let waiter = Waiter::new(auto, &self.segmenter_config);
        let pre_roll_len = ms_to_samples(
            self.segmenter_config.pre_roll_ms,
            self.segmenter_config.sample_rate,
        );
        let mut samples = std::mem::take(&mut self.recording);
        // Most of the pause is silence the model gains nothing from; a short tail is enough.
        samples.truncate(
            samples
                .len()
                .saturating_sub(silence.saturating_sub(pre_roll_len)),
        );

        self.stage = Stage::Waiting(Box::new(waiter));
        self.jobs.send(Job::UtteranceEnded).is_ok() && self.jobs.send(Job::Final(samples)).is_ok()
    }

    fn begin_utterance(&mut self, audio: Vec<f32>) -> bool {
        self.segmenter = Segmenter::new(self.segmenter_config.clone());
        self.stage = Stage::Speaking;
        if self.jobs.send(Job::UtteranceStarted).is_err() {
            return false;
        }
        let segments = self.segmenter.push(&audio);
        self.recording = audio;
        self.submit_all(segments)
    }

    fn end(mut self) {
        if matches!(self.stage, Stage::Waiting(_)) {
            return;
        }
        if !self.finalizing.load(Ordering::Acquire) {
            let fresh = Segmenter::new(self.segmenter_config.clone());
            let tail = std::mem::replace(&mut self.segmenter, fresh).finish();
            if !self.submit_all(tail) {
                return;
            }
        }
        let _ = self.jobs.send(Job::Final(self.recording));
    }
}

fn run_processing(
    config: PipelineConfig,
    audio: Receiver<Vec<f32>>,
    jobs: Sender<Job>,
    finalizing: &AtomicBool,
) {
    let channels = usize::from(config.input_channels);
    let mut resampler = Resampler::new(config.input_sample_rate, WHISPER_SAMPLE_RATE);
    let mut processor = Processor::new(config, jobs, finalizing);

    let mut mono = Vec::new();
    let mut resampled = Vec::new();

    for chunk in audio {
        mono.clear();
        resampled.clear();
        downmix_to_mono(&chunk, channels, &mut mono);
        resampler.process(&mono, &mut resampled);
        if !processor.feed(&resampled) {
            return;
        }
    }

    resampled.clear();
    resampler.finish(&mut resampled);
    if processor.feed(&resampled) {
        processor.end();
    }
}

fn transcription_event(
    result: Result<String, TranscribeError>,
    on_success: impl FnOnce(String) -> Event,
) -> Event {
    match result {
        Ok(text) => on_success(text),
        Err(error) => Event::Failed {
            message: error.to_string(),
        },
    }
}

struct JobQueue {
    receiver: Receiver<Job>,
    held: Option<Job>,
}

impl JobQueue {
    fn new(receiver: Receiver<Job>) -> Self {
        Self {
            receiver,
            held: None,
        }
    }

    fn next(&mut self) -> Option<Job> {
        self.held.take().or_else(|| self.receiver.recv().ok())
    }

    fn merge_waiting_previews(&mut self, segment: &mut Segment, max_samples: usize) {
        while segment.samples.len() < max_samples {
            match self.receiver.try_recv() {
                Ok(Job::Preview { segment: next, .. })
                    if segment.samples.len() + next.samples.len() <= max_samples =>
                {
                    segment.samples.extend_from_slice(&next.samples);
                    segment.reason = next.reason;
                }
                Ok(other) => {
                    self.held = Some(other);
                    break;
                }
                Err(_) => break,
            }
        }
    }
}

fn run_transcription<T, F>(
    mut transcriber: T,
    mut on_event: F,
    max_merged_preview_samples: usize,
    jobs: Receiver<Job>,
    finalizing: &AtomicBool,
) where
    T: Transcriber,
    F: FnMut(Event),
{
    let mut queue = JobQueue::new(jobs);
    while let Some(job) = queue.next() {
        match job {
            Job::Preview { .. } if finalizing.load(Ordering::Acquire) => {}
            Job::Preview { index, mut segment } => {
                queue.merge_waiting_previews(&mut segment, max_merged_preview_samples);
                let duration_ms =
                    segment.samples.len() as u64 * 1000 / u64::from(WHISPER_SAMPLE_RATE);
                let reason = segment.reason;
                let result = transcriber.transcribe_preview(&segment.samples);
                if finalizing.load(Ordering::Acquire) {
                    continue;
                }
                on_event(transcription_event(result, |text| Event::Preview {
                    index,
                    duration_ms,
                    reason,
                    text,
                }));
            }
            Job::UtteranceStarted => on_event(Event::UtteranceStarted),
            Job::UtteranceEnded => on_event(Event::UtteranceEnded),
            Job::Final(samples) if samples.is_empty() => {
                on_event(Event::Final {
                    text: String::new(),
                });
            }
            Job::Final(samples) => {
                on_event(transcription_event(
                    transcriber.transcribe(&samples),
                    |text| Event::Final { text },
                ));
            }
        }
    }
}
