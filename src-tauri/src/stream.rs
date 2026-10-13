use std::{error::Error, sync::mpsc::Receiver};

// TODO: specify with thiserror later if i need more error types?
pub type StreamError = Box<dyn Error + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct AudioStream {
    pub audio: Receiver<Vec<f32>>,
    pub format: AudioFormat,
    /// Dropping this stops the capture, which closes `audio`.
    pub stop: Box<dyn Send>,
}

pub trait StreamSource: Send + Sync + 'static {
    fn start(&self, device_id: Option<&str>) -> Result<AudioStream, StreamError>;
}
