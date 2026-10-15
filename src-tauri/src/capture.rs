use std::{
    sync::mpsc::{self, Receiver, Sender, SyncSender},
    thread::{self, JoinHandle},
};

use cpal::{
    FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use serde::Serialize;

use crate::stream::{AudioFormat, AudioStream, StreamError, StreamSource};

const AUDIO_QUEUE_CHUNKS: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("no audio input device available")]
    NoInputDevice,
    #[error("input device '{0}' not found")]
    DeviceNotFound(String),
    #[error("{0}")]
    CpalError(#[from] cpal::Error),
    #[error("unsupported sample format {0:?}")]
    UnsupportedFormat(SampleFormat),
    #[error("capture thread stopped unexpectedly")]
    ThreadGone,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputDeviceInfo {
    pub name: String,
    pub id: String,
    pub is_default: bool,
}

pub struct Capture {
    pub recorder: CaptureRecorder,
    pub audio: Receiver<Vec<f32>>,
    pub format: AudioFormat,
}

pub struct CaptureRecorder {
    stop_tx: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

pub fn list_input_devices() -> Result<Vec<InputDeviceInfo>, cpal::Error> {
    let host = cpal::default_host();

    let default_device_id = host
        .default_input_device()
        .and_then(|device| device.id().ok());

    host.input_devices()?
        .filter(|device| device.id().is_ok())
        .map(|device| -> Result<InputDeviceInfo, cpal::Error> {
            let device_id = device.id()?;
            let device_description = device.description()?;

            Ok(InputDeviceInfo {
                name: device_description.name().into(),
                id: device_id.to_string(),
                // Don't consider this condition true if both are none
                is_default: default_device_id.is_some() && default_device_id == device.id().ok(),
            })
        })
        .collect()
}

/// `device_name == None` selects the system default input device.
pub fn start_capture(
    device_name: Option<&str>,
    queue_chunks: usize,
) -> Result<Capture, CaptureError> {
    let (audio_tx, audio_rx) = mpsc::sync_channel(queue_chunks);
    let (stop_tx, stop_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let device_name = device_name.map(str::to_owned);

    let thread = thread::spawn(move || capture_thread(device_name, audio_tx, stop_rx, ready_tx));
    let format = ready_rx.recv().map_err(|_| CaptureError::ThreadGone)??;

    Ok(Capture {
        recorder: CaptureRecorder {
            stop_tx,
            thread: Some(thread),
        },
        audio: audio_rx,
        format,
    })
}

impl Drop for CaptureRecorder {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// cpal's `Stream` is not `Send`, so it lives and dies on this thread.
fn capture_thread(
    device_name: Option<String>,
    audio_tx: SyncSender<Vec<f32>>,
    stop_rx: Receiver<()>,
    ready_tx: Sender<Result<AudioFormat, CaptureError>>,
) {
    match open_stream(device_name.as_deref(), audio_tx) {
        Ok((stream, format)) => {
            let _ = ready_tx.send(Ok(format));
            let _ = stop_rx.recv();
            drop(stream);
        }
        Err(error) => {
            let _ = ready_tx.send(Err(error));
        }
    }
}

fn find_input_device(
    host: &cpal::Host,
    device_id: Option<&str>,
) -> Result<cpal::Device, CaptureError> {
    let Some(wanted) = device_id else {
        return host
            .default_input_device()
            .ok_or(CaptureError::NoInputDevice);
    };

    if let Some(selected_device) = host.input_devices()?.find(|input_device| {
        input_device
            .id()
            .is_ok_and(|dev_id| dev_id.to_string() == wanted)
    }) {
        Ok(selected_device)
    } else {
        Err(CaptureError::DeviceNotFound(wanted.to_owned()))
    }
}

fn open_stream(
    device_name: Option<&str>,
    audio_tx: SyncSender<Vec<f32>>,
) -> Result<(Stream, AudioFormat), CaptureError> {
    let host = cpal::default_host();
    let device = find_input_device(&host, device_name)?;

    let supported = device.default_input_config()?;
    let sample_format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let format = AudioFormat {
        sample_rate: config.sample_rate,
        channels: config.channels,
    };

    let stream = match sample_format {
        SampleFormat::F32 => build_stream::<f32>(&device, &config, audio_tx)?,
        SampleFormat::I16 => build_stream::<i16>(&device, &config, audio_tx)?,
        SampleFormat::I32 => build_stream::<i32>(&device, &config, audio_tx)?,
        SampleFormat::U16 => build_stream::<u16>(&device, &config, audio_tx)?,
        other => return Err(CaptureError::UnsupportedFormat(other)),
    };
    stream.play()?;

    Ok((stream, format))
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    audio_tx: SyncSender<Vec<f32>>,
) -> Result<Stream, CaptureError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream(
        *config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let chunk: Vec<f32> = data.iter().map(|&s| f32::from_sample_(s)).collect();
            // Never block the audio callback: if the consumer lags, the chunk is dropped.
            let _ = audio_tx.try_send(chunk);
        },
        |error| eprintln!("input stream error: {error}"),
        None,
    )?;
    Ok(stream)
}

pub struct CpalSource;

impl StreamSource for CpalSource {
    fn start(&self, device_name: Option<&str>) -> Result<AudioStream, StreamError> {
        let Capture {
            recorder,
            audio,
            format,
        } = start_capture(device_name, AUDIO_QUEUE_CHUNKS).map_err(|error| error.to_string())?;
        Ok(AudioStream {
            audio,
            format,
            stop: Box::new(recorder),
        })
    }
}
