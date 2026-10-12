use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::capture::{self, InputDeviceInfo};

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Background task failed: {0}")]
    BlockingFailed(#[from] tauri::Error),
    #[error("{0}")]
    CaptureError(cpal::Error),
}

impl Serialize for CommandError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[tauri::command]
pub async fn list_input_devices() -> Result<Vec<InputDeviceInfo>, CommandError> {
    let input_devices = tauri::async_runtime::spawn_blocking(capture::list_input_devices)
        .await?
        .map_err(|err| CommandError::CaptureError(err))?;

    Ok(input_devices)
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StartOptions {
    device_id: Option<String>,
}

#[tauri::command]
pub async fn start_recording(app: AppHandle, options: StartOptions) -> Result<(), CommandError> {
    println!("Device ID: {:?}", options.device_id);

    Ok(())
}