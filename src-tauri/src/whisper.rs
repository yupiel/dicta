use std::{ffi::c_int, path::Path};

use whisper_rs::{WhisperContext, WhisperContextParameters, WhisperState};

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
