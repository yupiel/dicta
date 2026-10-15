use std::{ffi::c_int, path::Path};

use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::pipeline::Transcriber;

// Default whisper sample rate
pub const WHISPER_SAMPLE_RATE: u32 = 16000;

// whisper.cpp ggml decoder seems to have issues with audio segments under 1 second https://github.com/ggml-org/whisper.cpp/issues/2065
const MIN_INPUT_SAMPLES: usize = WHISPER_SAMPLE_RATE as usize * 11 / 10;

// MAYBE: possibly use thiserror type if multiple different errors are useful in frontend
pub type TranscribeError = Box<dyn std::error::Error + Send + Sync>;

pub struct WhisperTranscriber {
    _context: WhisperContext,
    state: WhisperState,
    language: Option<String>,
    threads: c_int,
}

impl WhisperTranscriber {
    /// `language` is a Whisper language code such as "de" or "en"; `None` auto-detects.
    pub fn load(
        model_path: impl AsRef<Path>,
        language: Option<&str>,
    ) -> Result<Self, TranscribeError> {
        let context =
            WhisperContext::new_with_params(model_path, WhisperContextParameters::default())?;
        let state = context.create_state()?;
        // MAYBE: adjustable from frontend?
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as c_int;

        Ok(Self {
            _context: context,
            state,
            language: language.map(str::to_owned),
            threads,
        })
    }
}

impl Transcriber for WhisperTranscriber {
    fn set_language(&mut self, language: Option<&str>) {
        self.language = language.map(str::to_owned);
    }

    fn transcribe(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
        self.run(samples, Speed::Accurate)
    }

    fn transcribe_preview(&mut self, samples: &[f32]) -> Result<String, TranscribeError> {
        self.run(samples, Speed::Fast)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Speed {
    Accurate,
    Fast,
}

impl WhisperTranscriber {
    fn run(&mut self, samples: &[f32], speed: Speed) -> Result<String, TranscribeError> {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(self.language.as_deref());
        params.set_n_threads(self.threads);
        params.set_translate(false);
        // Defaults to false, which would feed earlier previews into the next
        // call on this shared state as decoder context.
        params.set_no_context(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        if speed == Speed::Fast {
            // No retries at higher temperatures when a decode looks bad, and no timestamp
            // decoding. Both only apply to clips that fit one 30 s window, which previews do.
            params.set_temperature_inc(0.0);
            params.set_no_timestamps(true);
            params.set_single_segment(true);
        }

        let mut padded = Vec::new();
        let input = if samples.len() < MIN_INPUT_SAMPLES {
            padded.extend_from_slice(samples);
            padded.resize(MIN_INPUT_SAMPLES, 0.0);
            padded.as_slice()
        } else {
            samples
        };

        self.state.full(params, input)?;

        let mut text = String::new();
        for index in 0..self.state.full_n_segments() {
            if let Some(segment) = self.state.get_segment(index) {
                text.push_str(&segment.to_str_lossy()?);
            }
        }
        Ok(text.trim().to_owned())
    }
}
