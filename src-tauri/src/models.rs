use std::{
    fs::{self, File}, io::{self, Write}, path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration,
};

use serde::Serialize;
use tauri_plugin_http::reqwest;

const PROGRESS_STEP_BYTES: u64 = 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const PART_SUFFIX: &str = ".part";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    pub size_mb: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub size_mb: u32,
    pub installed: bool,
    pub path: String,
}

pub const MODELS: [ModelSpec; 1] = [ModelSpec {
    id: "large-v3-turbo-q5_0",
    label: "Large v3 turbo, quantized",
    file_name: "ggml-large-v3-turbo-q5_0.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    size_mb: 574,
}];

pub fn find(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|spec| spec.id == id)
}

impl ModelSpec {
    pub fn path_in(&self, dir: &Path) -> PathBuf {
        dir.join(self.file_name)
    }

    pub fn is_installed(&self, dir: &Path) -> bool {
        self.path_in(dir).is_file()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("unknown model: {0}")]
    UnknownModel(String),
    #[error("cannot write model file: {0}")]
    Io(std::io::Error),
    #[error("download failed: {0}")]
    Http(String),
    #[error("download incomplete: got {expected} of {received} bytes")]
    Incomplete { expected: u64, received: u64 },
    #[error("download returned no data")]
    Empty,
}

impl From<io::Error> for ModelError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

// I'd prefer an OwnedMutex here but tauri doesn't re-export that from tokio
#[derive(Clone, Default)]
pub struct DownloadGate(Arc<AtomicBool>);

pub struct DownloadPermit(Arc<AtomicBool>);

impl DownloadGate {
    pub fn try_acquire(&self) -> Option<DownloadPermit> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| DownloadPermit(Arc::clone(&self.0)))
    }
}

impl Drop for DownloadPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub async fn download(
    spec: &ModelSpec,
    dir: PathBuf,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, ModelError> {
    let destination = spec.path_in(&dir);

    if destination.is_file() {
        return Ok(destination);
    }

    fs::create_dir_all(dir)?;

    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()
        .map_err(|e| ModelError::Http(e.to_string()))?;

    let mut response = client
        .get(spec.url)
        .send()
        .await
        .map_err(|e| ModelError::Http(e.to_string()))?
        .error_for_status()
        .map_err(|e| ModelError::Http(e.to_string()))?;

    let expected = response.content_length();

    let mut part_name = destination.as_os_str().to_owned();
    part_name.push(PART_SUFFIX);
    let part = PathBuf::from(part_name);

    let result = async {
        let mut file = File::create(&part)?;
        let mut received = 0_u64;
        let mut last_reported = 0_u64;

        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| ModelError::Http(e.to_string()))?
        {
            file.write_all(&chunk)?;

            received += chunk.len() as u64;

            if received - last_reported >= PROGRESS_STEP_BYTES {
                last_reported = received;
                on_progress(received, expected);
            }
        }

        file.flush()?;

        if received == 0 {
            return Err(ModelError::Empty);
        }

        if let Some(expected) = expected {
            if received != expected {
                return Err(ModelError::Incomplete { expected, received });
            }
        }

        on_progress(received, expected);

        fs::rename(&part, &destination)?;
        Ok(())
    }
    .await;

    if result.is_err() {
        let _ = fs::remove_file(&part);
    }

    result.map(|()| destination)
}
