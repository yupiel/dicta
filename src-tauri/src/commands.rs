use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    capture::{self, InputDeviceInfo},
    data::data_subdir,
    events::{UiEvent, emit_ui_event},
    models::{self, DownloadGate, ModelError, ModelInfo},
    recorder::StartOptions,
};

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Background task failed: {0}")]
    BlockingFailed(#[from] tauri::Error),
    #[error("{0}")]
    CaptureError(cpal::Error),
    #[error("{0}")]
    ModelError(models::ModelError),
}

impl From<ModelError> for CommandError {
    fn from(value: ModelError) -> Self {
        Self::ModelError(value)
    }
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

#[tauri::command]
pub async fn start_recording(app: AppHandle, options: StartOptions) -> Result<(), CommandError> {
    println!("Device ID: {:?}", options.device_id);

    Ok(())
}

#[tauri::command]
pub async fn list_models(app: AppHandle) -> Result<Vec<ModelInfo>, CommandError> {
    let appdata_dir = data_subdir(&app, "models")?;

    Ok(models::MODELS
        .iter()
        .map(|spec| ModelInfo {
            id: spec.id,
            label: spec.label,
            size_mb: spec.size_mb,
            installed: spec.is_installed(&appdata_dir),
            path: spec.path_in(&appdata_dir).display().to_string(),
        })
        .collect())
}

#[tauri::command]
pub async fn download_model(
    app: AppHandle,
    gate: State<'_, DownloadGate>,
    model_id: String,
) -> Result<String, CommandError> {
    let spec = models::find(&model_id).ok_or_else(|| ModelError::UnknownModel(model_id.clone()))?;
    let permit = gate.try_acquire().ok_or(ModelError::Http(
        "A model download is already running".into(),
    ))?;
    let dir = data_subdir(&app, "models")?;

    Ok(tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        models::download(spec, dir, move |downloaded, total| {
            emit_ui_event(
                &app,
                UiEvent::DownloadProgress {
                    model_id: model_id.clone(),
                    downloaded,
                    total,
                },
            )
        })
    })
    .await
    .map_err(CommandError::BlockingFailed)?
    .await
    .map_err(CommandError::ModelError)?
    .to_string_lossy()
    .to_string())
}
