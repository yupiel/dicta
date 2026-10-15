use crate::segmenter::{ms_to_samples, Segmenter, SegmenterConfig};

pub const DEFAULT_RESUME_RMS: f32 = 0.01;
pub const DEFAULT_RESUME_SPEECH_MS: u32 = 300;
const WAITING_BUFFER_MS: u32 = 1500;

#[derive(Debug, Clone)]
pub struct AutoConfig {
    /// Silence after speech that ends an utterance.
    pub pause_ms: u32,
    /// Speech below this RMS level (full scale = 1.0) never counts as the start of a new utterance.
    pub resume_rms: f32,
    /// Speech that must accumulate before the old text is replaced, so clicks and coughs do not.
    pub resume_speech_ms: u32,
}

impl AutoConfig {
    pub fn new(pause_ms: u32) -> Self {
        Self {
            pause_ms,
            resume_rms: DEFAULT_RESUME_RMS,
            resume_speech_ms: DEFAULT_RESUME_SPEECH_MS,
        }
    }
}

/// Listens between two utterances and hands back the audio of the next one once real speech is
/// confirmed. Audio from shortly before the confirmation is kept so the first words are not cut.
pub struct Waiter {
    detector: Segmenter,
    buffer: Vec<f32>,
    keep_len: usize,
    resume_len: usize,
}

impl Waiter {
    pub fn new(config: &AutoConfig, segmenter: &SegmenterConfig) -> Self {
        let detector = Segmenter::new(SegmenterConfig {
            min_speech_rms: config.resume_rms,
            ..segmenter.clone()
        });
        Self {
            detector,
            buffer: Vec::new(),
            keep_len: ms_to_samples(WAITING_BUFFER_MS, segmenter.sample_rate),
            resume_len: ms_to_samples(config.resume_speech_ms, segmenter.sample_rate),
        }
    }

    pub fn push(&mut self, samples: &[f32]) -> Option<Vec<f32>> {
        self.buffer.extend_from_slice(samples);
        // The detector's own segments are irrelevant; only its running speech count matters.
        let _ = self.detector.push(samples);

        if self.detector.speech_samples() >= self.resume_len {
            return Some(std::mem::take(&mut self.buffer));
        }
        if !self.detector.is_active() {
            let excess = self.buffer.len().saturating_sub(self.keep_len);
            self.buffer.drain(..excess);
        }
        None
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::whisper::WHISPER_SAMPLE_RATE;

    const RATE: usize = WHISPER_SAMPLE_RATE as usize;

    fn tone(amplitude: f32, seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize)
            .map(|i| {
                amplitude * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / RATE as f32).sin()
            })
            .collect()
    }

    fn silence(seconds: f32) -> Vec<f32> {
        vec![0.0; (RATE as f32 * seconds) as usize]
    }

    fn waiter() -> Waiter {
        Waiter::new(&AutoConfig::new(2_000), &SegmenterConfig::default())
    }

    fn feed(waiter: &mut Waiter, signal: &[f32]) -> Option<Vec<f32>> {
        signal
            .chunks(RATE / 50)
            .find_map(|chunk| waiter.push(chunk))
    }

    #[test]
    fn silence_never_starts_an_utterance() {
        assert!(feed(&mut waiter(), &silence(30.0)).is_none());
    }

    #[test]
    fn speech_below_the_resume_level_is_ignored() {
        let quiet = tone(0.004, 5.0);
        assert!(feed(&mut waiter(), &quiet).is_none());
    }

    #[test]
    fn a_short_click_is_ignored() {
        let signal = [silence(1.0), tone(0.3, 0.05), silence(2.0)].concat();
        assert!(feed(&mut waiter(), &signal).is_none());
    }

    #[test]
    fn real_speech_starts_an_utterance_and_keeps_the_lead_in() {
        let signal = [silence(3.0), tone(0.3, 1.0)].concat();
        let audio = feed(&mut waiter(), &signal).expect("speech should be detected");

        let lead_in = audio
            .iter()
            .take_while(|sample| sample.abs() < 1e-6)
            .count();
        assert!(lead_in >= RATE * 3 / 10, "lead-in was {lead_in} samples");
        assert!(
            audio.len() <= RATE * 2,
            "buffer was {} samples",
            audio.len()
        );
    }

    #[test]
    fn the_buffer_stays_bounded_during_long_silence() {
        let mut waiter = waiter();
        feed(&mut waiter, &silence(60.0));
        assert!(waiter.buffer.len() <= RATE * 2);
    }
}
