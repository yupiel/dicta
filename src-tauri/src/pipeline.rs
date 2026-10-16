use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::auto::{AutoConfig, Waiter};
use crate::resample::{Resampler, downmix_to_mono};
use crate::segmenter::{CutReason, Segment, Segmenter, SegmenterConfig, ms_to_samples};
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

#[cfg(test)]
mod tests {
    use super::*;

    struct LengthTranscriber;

    impl Transcriber for LengthTranscriber {
        fn transcribe(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
            Ok(samples.len().to_string())
        }
    }

    struct FailingTranscriber;

    impl Transcriber for FailingTranscriber {
        fn transcribe(&mut self, _: &[f32]) -> Result<String, TranscribeError> {
            Err("model unavailable".into())
        }
    }

    fn tone(rate: u32, seconds: u32) -> Vec<f32> {
        (0..rate * seconds)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin())
            .collect()
    }

    fn stereo_input(rate: u32) -> Vec<f32> {
        let silence = vec![0.0; rate as usize];
        let mono = [
            silence.clone(),
            tone(rate, 2),
            silence.clone(),
            tone(rate, 12),
            silence,
        ]
        .concat();
        mono.iter().flat_map(|&s| [s, s]).collect()
    }

    fn run_configured<T: Transcriber>(
        transcriber: T,
        input: &[f32],
        auto: Option<AutoConfig>,
    ) -> Vec<Event> {
        let (audio_tx, audio_rx) = mpsc::sync_channel(64);
        let (event_tx, event_rx) = mpsc::channel();
        let config = PipelineConfig {
            input_sample_rate: 48_000,
            input_channels: 2,
            segmenter: SegmenterConfig::default(),
            max_merged_preview_ms: 0,
            auto,
        };

        let pipeline = Pipeline::start(config, audio_rx, transcriber, move |event| {
            event_tx.send(event).unwrap();
        });
        for chunk in input.chunks(960) {
            audio_tx.send(chunk.to_vec()).unwrap();
        }
        drop(audio_tx);
        pipeline.finish().unwrap();

        event_rx.try_iter().collect()
    }

    fn run_pipeline<T: Transcriber>(transcriber: T, input: &[f32]) -> Vec<Event> {
        run_configured(transcriber, input, None)
    }

    #[test]
    fn emits_previews_then_one_final_over_the_whole_recording() {
        let events = run_pipeline(LengthTranscriber, &stereo_input(48_000));

        let previews: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::Preview { .. }))
            .collect();
        assert_eq!(previews.len(), 3);

        let Some(Event::Final { text }) = events.last() else {
            panic!("last event must be Final, got {events:?}");
        };
        let final_len: i64 = text.parse().unwrap();
        assert!((final_len - 17 * 16_000).abs() <= 2, "got {final_len}");
    }

    #[test]
    fn preview_indices_are_sequential() {
        let events = run_pipeline(LengthTranscriber, &stereo_input(48_000));
        let indices: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Event::Preview { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn previews_report_duration_and_cut_reason() {
        let events = run_pipeline(LengthTranscriber, &stereo_input(48_000));
        let previews: Vec<(u64, CutReason)> = events
            .iter()
            .filter_map(|e| match e {
                Event::Preview {
                    duration_ms,
                    reason,
                    ..
                } => Some((*duration_ms, *reason)),
                _ => None,
            })
            .collect();

        let expected = [
            (2_600, CutReason::Pause),
            (10_000, CutReason::MaxLength),
            (2_600, CutReason::Pause),
        ];
        assert_eq!(previews.len(), expected.len(), "got {previews:?}");
        for ((duration, reason), (want_duration, want_reason)) in previews.iter().zip(expected) {
            assert_eq!(*reason, want_reason, "got {previews:?}");
            assert!(duration.abs_diff(want_duration) <= 60, "got {previews:?}");
        }
    }

    fn preview_job(index: usize, len: usize, reason: CutReason) -> Job {
        Job::Preview {
            index,
            segment: Segment {
                samples: vec![0.0; len],
                reason,
            },
        }
    }

    fn queue_with(jobs: Vec<Job>) -> JobQueue {
        let (tx, rx) = mpsc::channel();
        for job in jobs {
            tx.send(job).unwrap();
        }
        JobQueue::new(rx)
    }

    fn first_preview(queue: &mut JobQueue) -> (usize, Segment) {
        match queue.next() {
            Some(Job::Preview { index, segment }) => (index, segment),
            _ => panic!("expected a preview job"),
        }
    }

    #[test]
    fn waiting_previews_are_merged_up_to_the_limit() {
        let mut queue = queue_with(vec![
            preview_job(0, 100, CutReason::Pause),
            preview_job(1, 100, CutReason::MaxLength),
            preview_job(2, 100, CutReason::Pause),
        ]);
        let (index, mut segment) = first_preview(&mut queue);

        queue.merge_waiting_previews(&mut segment, 250);

        assert_eq!(index, 0);
        assert_eq!(segment.samples.len(), 200);
        assert_eq!(segment.reason, CutReason::MaxLength);
        let (next_index, next) = first_preview(&mut queue);
        assert_eq!((next_index, next.samples.len()), (2, 100));
    }

    #[test]
    fn a_waiting_final_is_never_merged() {
        let mut queue = queue_with(vec![
            preview_job(0, 100, CutReason::Pause),
            Job::Final(vec![0.0; 5]),
        ]);
        let (_, mut segment) = first_preview(&mut queue);

        queue.merge_waiting_previews(&mut segment, 10_000);

        assert_eq!(segment.samples.len(), 100);
        assert!(matches!(queue.next(), Some(Job::Final(samples)) if samples.len() == 5));
    }

    #[test]
    fn a_limit_of_zero_disables_merging() {
        let mut queue = queue_with(vec![
            preview_job(0, 100, CutReason::Pause),
            preview_job(1, 100, CutReason::Pause),
        ]);
        let (_, mut segment) = first_preview(&mut queue);

        queue.merge_waiting_previews(&mut segment, 0);

        assert_eq!(segment.samples.len(), 100);
        assert_eq!(first_preview(&mut queue).0, 1);
    }

    #[test]
    fn an_empty_queue_ends_the_iteration() {
        let mut queue = queue_with(vec![]);
        assert!(queue.next().is_none());
    }

    struct PreviewMarker;

    impl Transcriber for PreviewMarker {
        fn transcribe(&mut self, _: &[f32]) -> Result<String, TranscribeError> {
            Ok("full".to_owned())
        }

        fn transcribe_preview(&mut self, _: &[f32]) -> Result<String, TranscribeError> {
            Ok("fast".to_owned())
        }
    }

    #[test]
    fn previews_use_the_fast_path_and_the_final_uses_the_full_one() {
        let events = run_pipeline(PreviewMarker, &stereo_input(48_000));

        let texts: Vec<_> = events
            .iter()
            .map(|event| match event {
                Event::Preview { text, .. } | Event::Final { text } => text.as_str(),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(texts, ["fast", "fast", "fast", "full"]);
    }

    struct BlockingFirstCall {
        started: mpsc::Sender<()>,
        release: Receiver<()>,
        calls: usize,
    }

    impl Transcriber for BlockingFirstCall {
        fn transcribe(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
            self.calls += 1;
            if self.calls == 1 {
                self.started.send(()).unwrap();
                self.release.recv().unwrap();
            }
            Ok(samples.len().to_string())
        }
    }

    fn pipeline_events(config_rate: u32) -> (PipelineConfig, Receiver<Event>, mpsc::Sender<Event>) {
        let (event_tx, event_rx) = mpsc::channel();
        let config = PipelineConfig {
            input_sample_rate: config_rate,
            input_channels: 2,
            segmenter: SegmenterConfig::default(),
            max_merged_preview_ms: 0,
            auto: None,
        };
        (config, event_rx, event_tx)
    }

    #[test]
    fn previews_stopped_up_front_leave_only_the_final() {
        let (config, event_rx, event_tx) = pipeline_events(48_000);
        let (audio_tx, audio_rx) = mpsc::sync_channel(64);
        let pipeline = Pipeline::start(config, audio_rx, LengthTranscriber, move |event| {
            event_tx.send(event).unwrap();
        });

        pipeline.stop_previews();
        for chunk in stereo_input(48_000).chunks(960) {
            audio_tx.send(chunk.to_vec()).unwrap();
        }
        drop(audio_tx);
        pipeline.finish().unwrap();

        let events: Vec<_> = event_rx.try_iter().collect();
        let [Event::Final { text }] = events.as_slice() else {
            panic!("expected only Final, got {events:?}");
        };
        let final_len: i64 = text.parse().unwrap();
        assert!((final_len - 17 * 16_000).abs() <= 2, "got {final_len}");
    }

    #[test]
    fn a_preview_in_flight_when_stopping_is_dropped_and_later_ones_are_skipped() {
        let (config, event_rx, event_tx) = pipeline_events(48_000);
        let (audio_tx, audio_rx) = mpsc::sync_channel(64);
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let transcriber = BlockingFirstCall {
            started: started_tx,
            release: release_rx,
            calls: 0,
        };
        let pipeline = Pipeline::start(config, audio_rx, transcriber, move |event| {
            event_tx.send(event).unwrap();
        });

        let rate = 48_000;
        let silence = vec![0.0; rate];
        let mono = [
            silence.clone(),
            tone(rate as u32, 2),
            silence.clone(),
            tone(rate as u32, 2),
            silence,
        ]
        .concat();
        let stereo: Vec<f32> = mono.iter().flat_map(|&s| [s, s]).collect();
        for chunk in stereo.chunks(960) {
            audio_tx.send(chunk.to_vec()).unwrap();
        }
        started_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();

        pipeline.stop_previews();
        release_tx.send(()).unwrap();
        drop(audio_tx);
        pipeline.finish().unwrap();

        let events: Vec<_> = event_rx.try_iter().collect();
        let [Event::Final { text }] = events.as_slice() else {
            panic!("expected only Final, got {events:?}");
        };
        let final_len: i64 = text.parse().unwrap();
        assert!((final_len - 7 * 16_000).abs() <= 2, "got {final_len}");
    }

    #[test]
    fn empty_recording_gives_empty_final() {
        let events = run_pipeline(LengthTranscriber, &[]);
        assert_eq!(
            events,
            vec![Event::Final {
                text: String::new()
            }]
        );
    }

    #[test]
    fn transcriber_errors_are_reported_without_stopping_the_pipeline() {
        let events = run_pipeline(FailingTranscriber, &stereo_input(48_000));
        assert!(events.iter().all(|e| matches!(e, Event::Failed { .. })));
        assert_eq!(events.len(), 4);
    }

    fn stereo(mono: &[f32]) -> Vec<f32> {
        mono.iter().flat_map(|&s| [s, s]).collect()
    }

    fn silent(rate: u32, seconds: u32) -> Vec<f32> {
        vec![0.0; (rate * seconds) as usize]
    }

    fn run_auto(mono: &[f32]) -> Vec<Event> {
        run_configured(
            LengthTranscriber,
            &stereo(mono),
            Some(AutoConfig::new(2_000)),
        )
    }

    fn kinds(events: &[Event]) -> Vec<&'static str> {
        events
            .iter()
            .map(|event| match event {
                Event::Preview { .. } => "preview",
                Event::UtteranceStarted => "started",
                Event::UtteranceEnded => "ended",
                Event::Final { .. } => "final",
                Event::Failed { .. } => "failed",
            })
            .collect()
    }

    #[test]
    fn auto_mode_finalizes_each_utterance_after_a_long_pause() {
        let rate = 48_000;
        let mono = [
            silent(rate, 1),
            tone(rate, 2),
            silent(rate, 3),
            tone(rate, 3),
            silent(rate, 3),
        ]
        .concat();

        let events = run_auto(&mono);

        assert_eq!(
            kinds(&events),
            ["ended", "final", "started", "ended", "final"]
        );
    }

    #[test]
    fn auto_mode_waits_without_emitting_when_nothing_more_is_said() {
        let rate = 48_000;
        let mono = [tone(rate, 2), silent(rate, 30)].concat();

        let events = run_auto(&mono);

        assert_eq!(kinds(&events), ["ended", "final"]);
    }

    #[test]
    fn auto_mode_ignores_a_click_while_waiting() {
        let rate = 48_000;
        let click = tone(rate, 1)[..(rate / 20) as usize].to_vec();
        let mono = [tone(rate, 2), silent(rate, 3), click, silent(rate, 3)].concat();

        let events = run_auto(&mono);

        assert_eq!(kinds(&events), ["ended", "final"]);
    }

    #[test]
    fn auto_mode_finalizes_a_partial_utterance_when_the_recording_ends() {
        let rate = 48_000;
        let mono = [
            tone(rate, 2),
            silent(rate, 3),
            tone(rate, 2),
            silent(rate, 1),
        ]
        .concat();

        let events = run_auto(&mono);

        assert_eq!(kinds(&events), ["ended", "final", "started", "final"]);
    }

    #[test]
    fn the_second_utterance_is_transcribed_without_the_first() {
        let rate = 48_000;
        let mono = [
            tone(rate, 2),
            silent(rate, 3),
            tone(rate, 2),
            silent(rate, 3),
        ]
        .concat();

        let events = run_auto(&mono);

        let lengths: Vec<i64> = events
            .iter()
            .filter_map(|event| match event {
                Event::Final { text } => Some(text.parse().unwrap()),
                _ => None,
            })
            .collect();
        assert_eq!(lengths.len(), 2);
        assert!(
            lengths[0] > 2 * 16_000 && lengths[0] < 3 * 16_000,
            "the pause should be trimmed, first utterance was {} samples",
            lengths[0]
        );
        assert!(
            lengths[1] > 2 * 16_000 && lengths[1] < 4 * 16_000,
            "second utterance was {} samples",
            lengths[1]
        );
    }
}

#[cfg(test)]
mod shared_transcriber_tests {
    use super::*;

    struct EchoLanguage(Option<String>);

    impl Transcriber for EchoLanguage {
        fn transcribe(&mut self, _: &[f32]) -> Result<String, TranscribeError> {
            Ok(self.0.clone().unwrap_or_else(|| "auto".to_owned()))
        }

        fn set_language(&mut self, language: Option<&str>) {
            self.0 = language.map(str::to_owned);
        }
    }

    #[test]
    fn clones_share_one_underlying_model() {
        let shared = SharedTranscriber::new(EchoLanguage(None));
        let mut first = shared.clone();
        let mut second = shared;

        assert_eq!(second.transcribe(&[]).unwrap(), "auto");
        first.set_language(Some("de"));
        assert_eq!(second.transcribe(&[]).unwrap(), "de");
    }

    #[test]
    fn a_panic_in_the_model_reports_an_error_instead_of_propagating() {
        struct Panicking;
        impl Transcriber for Panicking {
            fn transcribe(&mut self, _: &[f32]) -> Result<String, TranscribeError> {
                panic!("model crashed");
            }
        }

        let mut shared = SharedTranscriber::new(Panicking);
        let mut crashing = shared.clone();
        let _ = std::thread::spawn(move || crashing.transcribe(&[])).join();

        assert!(shared.transcribe(&[]).is_err());
    }
}
