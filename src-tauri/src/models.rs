use std::{
    fs::{self, File}, io::{self, Read, Write}, path::{Path, PathBuf}, sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    }, time::Duration,
};

use serde::Serialize;

const READ_BUFFER_BYTES: usize = 64 * 1024;
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

pub fn download(
    spec: &ModelSpec,
    dir: &Path,
    on_progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, ModelError> {
    let destination = spec.path_in(dir);
    if destination.is_file() {
        return Ok(destination);
    }
    fs::create_dir_all(dir)?;

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(CONNECT_TIMEOUT)
        .timeout_read(READ_TIMEOUT)
        .build();
    let response = agent
        .get(spec.url)
        .call()
        .map_err(|error| ModelError::Http(error.to_string()))?;
    let expected = response
        .header("Content-Length")
        .and_then(|value| value.parse().ok());

    write_atomically(response.into_reader(), expected, &destination, on_progress)?;
    Ok(destination)
}

// Written to `<name>.part` and renamed at the end, so an interrupted download can never be
// mistaken for an installed model.
fn write_atomically(
    reader: impl Read,
    expected: Option<u64>,
    destination: &Path,
    on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), ModelError> {
    let mut part_name = destination.as_os_str().to_owned();
    part_name.push(PART_SUFFIX);
    let part = PathBuf::from(part_name);

    let result = copy_checked(reader, expected, &part, on_progress)
        .and_then(|()| fs::rename(&part, destination).map_err(ModelError::from));
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

fn copy_checked(
    mut reader: impl Read,
    expected: Option<u64>,
    path: &Path,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), ModelError> {
    let mut file = File::create(path)?;
    let mut buffer = vec![0u8; READ_BUFFER_BYTES];
    let mut received: u64 = 0;
    let mut last_reported: u64 = 0;

    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(ModelError::Http(error.to_string())),
        };
        file.write_all(&buffer[..read])?;
        received += read as u64;
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
    Ok(())
}
