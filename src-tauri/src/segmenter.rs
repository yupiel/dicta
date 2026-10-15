use std::collections::VecDeque;

use serde::Serialize;

use crate::whisper::WHISPER_SAMPLE_RATE;

const NOISE_FALL_PER_FRAME: f32 = 0.1;
const NOISE_RISE_PER_FRAME: f32 = 0.01;

#[derive(Debug, Clone)]
pub struct SegmenterConfig {
    pub sample_rate: u32,
    pub frame_ms: u32,
    /// Speech never counts below this RMS level (full scale = 1.0), whatever the noise floor.
    pub min_speech_rms: f32,
    /// A segment starts when a frame exceeds the noise floor by this factor.
    pub start_over_noise: f32,
    /// Once started, frames above this multiple of the noise floor still count as speech.
    pub stay_over_noise: f32,
    /// Upper bound for the noise estimate, so sustained loud sound is not mistaken for noise.
    pub max_noise_rms: f32,
    pub pause_ms: u32,
    pub max_segment_ms: u32,
    pub pre_roll_ms: u32,
    pub min_speech_ms: u32,
}

// Reasonable defaults after some trial and error and with AI recommendation
impl Default for SegmenterConfig {
    fn default() -> Self {
        Self {
            sample_rate: WHISPER_SAMPLE_RATE,
            frame_ms: 20,
            min_speech_rms: 0.002,
            start_over_noise: 4.0,
            stay_over_noise: 2.0,
            max_noise_rms: 0.02,
            pause_ms: 600,
            max_segment_ms: 10_000,
            pre_roll_ms: 300,
            min_speech_ms: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CutReason {
    Pause,
    MaxLength,
    EndOfRecording,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub samples: Vec<f32>,
    pub reason: CutReason,
}

pub struct Segmenter {
    min_speech_rms: f32,
    start_over_noise: f32,
    stay_over_noise: f32,
    max_noise_rms: f32,
    frame_len: usize,
    pause_len: usize,
    max_len: usize,
    pre_roll_len: usize,
    min_speech_len: usize,

    noise_floor: Option<f32>,
    stay_level: f32,
    partial_frame: Vec<f32>,
    pre_roll: VecDeque<f32>,
    segment: Vec<f32>,
    segment_start: usize,
    active: bool,
    speech_len: usize,
    trailing_silence: usize,
    position: usize,
    heard_speech: bool,
    quiet_len: usize,
}

pub(crate) fn ms_to_samples(ms: u32, sample_rate: u32) -> usize {
    (u64::from(sample_rate) * u64::from(ms) / 1000) as usize
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0_f32;
    }
    (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt()
}

impl Segmenter {
    pub fn new(config: SegmenterConfig) -> Self {
        let samples = |ms| ms_to_samples(ms, config.sample_rate);
        Self {
            min_speech_rms: config.min_speech_rms,
            start_over_noise: config.start_over_noise,
            stay_over_noise: config.stay_over_noise,
            max_noise_rms: config.max_noise_rms,
            frame_len: samples(config.frame_ms).max(1),
            pause_len: samples(config.pause_ms),
            max_len: samples(config.max_segment_ms),
            pre_roll_len: samples(config.pre_roll_ms),
            min_speech_len: samples(config.min_speech_ms),
            noise_floor: None,
            stay_level: 0.0,
            partial_frame: Vec::new(),
            pre_roll: VecDeque::new(),
            segment: Vec::new(),
            segment_start: 0,
            active: false,
            speech_len: 0,
            trailing_silence: 0,
            position: 0,
            heard_speech: false,
            quiet_len: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Speech accumulated in the segment being built; 0 between segments.
    pub fn speech_samples(&self) -> usize {
        if self.active { self.speech_len } else { 0 }
    }

    /// Length of the silence since the last speech, across segment cuts; `None` before any speech.
    pub fn silence_since_speech(&self) -> Option<usize> {
        self.heard_speech.then_some(self.quiet_len)
    }

    pub fn push(&mut self, mut input: &[f32]) -> Vec<Segment> {
        let mut out = Vec::new();

        while !input.is_empty() {
            let take = (self.frame_len - self.partial_frame.len()).min(input.len());

            self.partial_frame.extend_from_slice(&input[..take]);
            input = &input[take..];

            if self.partial_frame.len() == self.frame_len {
                let frame = std::mem::take(&mut self.partial_frame);
                self.handle_frame(&frame, &mut out);
                self.partial_frame = frame;
                self.partial_frame.clear();
            }
        }
        out
    }

    pub fn finish(mut self) -> Vec<Segment> {
        let mut out = Vec::new();
        if !self.partial_frame.is_empty() {
            let frame = std::mem::take(&mut self.partial_frame);
            self.handle_frame(&frame, &mut out);
        }
        if self.active {
            self.emit(CutReason::EndOfRecording, &mut out);
        }
        out
    }

    fn handle_frame(&mut self, frame: &[f32], out: &mut Vec<Segment>) {
        let level = rms(frame);

        if self.active {
            self.segment.extend_from_slice(frame);
            if level >= self.stay_level {
                self.speech_len += frame.len();
                self.trailing_silence = 0;
                self.quiet_len = 0;
            } else {
                self.trailing_silence += frame.len();
                self.quiet_len += frame.len();
            }

            if self.trailing_silence >= self.pause_len {
                self.emit(CutReason::Pause, out);
            } else if self.segment.len() >= self.max_len {
                self.emit(CutReason::MaxLength, out);
            }
        } else {
            let noise = *self
                .noise_floor
                .get_or_insert(level.min(self.max_noise_rms));
            let start_level = (noise * self.start_over_noise).max(self.min_speech_rms);

            if level >= start_level {
                self.stay_level = (noise * self.stay_over_noise).max(self.min_speech_rms * 0.5);
                self.active = true;
                self.segment_start = self.position - self.pre_roll.len();
                self.segment.extend(self.pre_roll.drain(..));
                self.segment.extend_from_slice(frame);
                self.speech_len = frame.len();
                self.trailing_silence = 0;
                self.heard_speech = true;
                self.quiet_len = 0;
            } else {
                self.quiet_len += frame.len();
                self.noise_floor = Some(self.next_noise(noise, level));
                self.pre_roll.extend(frame);
                let excess = self.pre_roll.len().saturating_sub(self.pre_roll_len);
                self.pre_roll.drain(..excess);
            }
        }

        self.position += frame.len();
    }

    // Falls quickly and rises slowly, so brief dips in speech pull the estimate
    // down but quiet speech cannot drag it up and hide itself.
    fn next_noise(&self, noise: f32, level: f32) -> f32 {
        let rate = if level < noise {
            NOISE_FALL_PER_FRAME
        } else {
            NOISE_RISE_PER_FRAME
        };
        (noise + (level - noise) * rate).min(self.max_noise_rms)
    }

    fn emit(&mut self, reason: CutReason, out: &mut Vec<Segment>) {
        let mut samples = std::mem::take(&mut self.segment);
        if reason != CutReason::MaxLength {
            let kept_tail = self.trailing_silence.min(self.pre_roll_len);
            samples.truncate(samples.len() - self.trailing_silence + kept_tail);
        }

        if self.speech_len >= self.min_speech_len {
            out.push(Segment {
                samples,
                reason,
            });
        }

        self.active = false;
        self.speech_len = 0;
        self.trailing_silence = 0;
    }
}
