use std::path::{Path, PathBuf};

use opener::OpenError;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::{
    capture::{self, InputDeviceInfo},
    events::{UiEvent, emit_ui_event},
    history::{self, HistoryEntry, HistoryError, HistoryStore},
    live_text::LiveTextStore,
    models::{self, DownloadGate, ModelError, ModelInfo},
    recorder::{self, Phase, Recorder, RecorderError, StartOptions},
    whisper::WhisperTranscriber,
};

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("Background task failed: {0}")]
    BlockingFailed(#[from] tauri::Error),
    #[error("{0}")]
    CaptureError(cpal::Error),
    #[error("{0}")]
    ModelError(models::ModelError),
    #[error("{0}")]
    RecorderError(recorder::RecorderError),
    #[error("{0}")]
    HistoryError(history::HistoryError),
    #[error("{0}")]
    OpenError(opener::OpenError),
    #[error("{0}")]
    WhisperError(String),
}

impl From<ModelError> for CommandError {
    fn from(value: ModelError) -> Self {
        Self::ModelError(value)
    }
}
impl From<RecorderError> for CommandError {
    fn from(value: RecorderError) -> Self {
        Self::RecorderError(value)
    }
}
impl From<HistoryError> for CommandError {
    fn from(value: HistoryError) -> Self {
        Self::HistoryError(value)
    }
}
impl From<OpenError> for CommandError {
    fn from(value: OpenError) -> Self {
        Self::OpenError(value)
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictaStatus {
    phase: Phase,
    model_loaded: bool,
}

fn data_subdir(app: &AppHandle, name: &str) -> Result<PathBuf, tauri::Error> {
    app.path().app_data_dir().map(|dir| dir.join(name))
}

#[tauri::command]
pub async fn list_input_devices() -> Result<Vec<InputDeviceInfo>, CommandError> {
    let input_devices = tauri::async_runtime::spawn_blocking(capture::list_input_devices)
        .await?
        .map_err(|err| CommandError::CaptureError(err))?;

    Ok(input_devices)
}

#[tauri::command]
pub async fn start_recording(
    recorder: State<'_, Recorder>,
    options: StartOptions,
) -> Result<u64, CommandError> {
    let recorder = recorder.inner().clone();

    Ok(
        tauri::async_runtime::spawn_blocking(move || recorder.start(options))
            .await?
            .map_err(|err| CommandError::RecorderError(err))?,
    )
}

#[tauri::command]
pub async fn stop_recording(recorder: State<'_, Recorder>) -> Result<(), CommandError> {
    let recorder = recorder.inner().clone();

    Ok(
        tauri::async_runtime::spawn_blocking(move || recorder.stop())
            .await?
            .map_err(|err| CommandError::RecorderError(err))?,
    )
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
        models::download(spec, &dir, move |downloaded, total| {
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
    .map_err(CommandError::ModelError)?
    .to_string_lossy()
    .to_string())
}

#[tauri::command]
pub async fn load_model(recorder: State<'_, Recorder>, path: String) -> Result<(), CommandError> {
    let recorder = recorder.inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        if !Path::new(&path).is_file() {
            return Err(format!("model file not found: {path}"));
        }
        
        let transcriber = WhisperTranscriber::load(&path, None)
            .map_err(|error| format!("cannot load model {path}: {error}"))?;
        recorder.set_model(transcriber, path);

        Ok(())
    })
    .await?
    .map_err(CommandError::WhisperError)
}

#[tauri::command]
pub async fn get_status(recorder: State<'_, Recorder>) -> Result<DictaStatus, String> {
    Ok(DictaStatus {
        phase: recorder.phase().into(),
        model_loaded: recorder.has_model(),
    })
}

#[tauri::command]
pub async fn reset_transcript(live_text: State<'_, LiveTextStore>) -> Result<(), CommandError> {
    live_text.clear();
    Ok(())
}

#[tauri::command]
pub async fn list_history(
    history: State<'_, HistoryStore>,
) -> Result<Vec<HistoryEntry>, CommandError> {
    let history = history.inner().clone();

    Ok(tauri::async_runtime::spawn_blocking(move || history.list())
        .await?
        .map(|entries| entries.into_iter().map(HistoryEntry::from).collect())?)
}

#[tauri::command]
pub async fn delete_history(
    history: State<'_, HistoryStore>,
    id: String,
) -> Result<(), CommandError> {
    let history = history.inner().clone();

    tauri::async_runtime::spawn_blocking(move || history.delete(&id))
        .await?
        .map_err(CommandError::HistoryError)
}

#[tauri::command]
pub async fn reveal_history_entry(
    history: State<'_, HistoryStore>,
    id: String,
) -> Result<(), CommandError> {
    let history = history.inner().clone();

    tauri::async_runtime::spawn_blocking(move || -> Result<(), CommandError> {
        let path = history.locate(&id)?;
        Ok(opener::reveal(path)?)
    })
    .await?
}

#[tauri::command]
pub async fn clear_history(history: State<'_, HistoryStore>) -> Result<(), CommandError> {
    let history = history.inner().clone();

    tauri::async_runtime::spawn_blocking(move || history.clear())
        .await?
        .map_err(CommandError::HistoryError)
}
