use std::{
    cmp::Reverse,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

const FILE_EXTENSION: &str = "txt";
const TEMP_EXTENSION: &str = "tmp";

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub created_at_ms: u64,
    pub text: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("history storage failed: {0}")]
    Io(io::Error),
    #[error("invalid history id: {0}")]
    InvalidId(String),
}

impl From<io::Error> for HistoryError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

// One plain-text file per transcript, named `<unix-ms>-<session>.txt`: the name carries the
// metadata, so there is no format to migrate and a user can read or delete files by hand.
#[derive(Debug, Clone)]
pub struct HistoryStore {
    dir: PathBuf,
}

impl HistoryStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn add(&self, session: u64, text: &str) -> Result<Option<HistoryEntry>, HistoryError> {
        self.add_at(now_ms(), session, text)
    }

    pub fn add_at(
        &self,
        created_at_ms: u64,
        session: u64,
        text: &str,
    ) -> Result<Option<HistoryEntry>, HistoryError> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(None);
        }

        fs::create_dir_all(&self.dir)?;

        let id = format!("{created_at_ms}-{session}");
        let path = self.path_for(&id);
        let temp = path.with_extension(TEMP_EXTENSION);

        fs::write(&temp, text)?;
        fs::rename(&temp, &path)?;

        Ok(Some(HistoryEntry {
            id,
            created_at_ms,
            text: text.to_owned(),
        }))
    }

    pub fn list(&self) -> Result<Vec<HistoryEntry>, HistoryError> {
        let reader = match fs::read_dir(&self.dir) {
            Ok(reader) => reader,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };

        let mut entries = Vec::new();
        for item in reader {
            if let Some(entry) = read_entry(&item?.path()) {
                entries.push(entry);
            }
        }

        entries.sort_by_key(|entry| Reverse(parse_id(&entry.id)));

        Ok(entries)
    }

    pub fn locate(&self, id: &str) -> Result<PathBuf, HistoryError> {
        if parse_id(id).is_none() {
            return Err(HistoryError::InvalidId(id.to_owned()));
        }

        let path = self.path_for(id);
        if path.is_file() {
            Ok(path)
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound).into())
        }
    }

    pub fn delete(&self, id: &str) -> Result<(), HistoryError> {
        if parse_id(id).is_none() {
            return Err(HistoryError::InvalidId(id.to_owned()));
        }

        match fs::remove_file(self.path_for(id)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        }
    }

    pub fn clear(&self) -> Result<(), HistoryError> {
        for entry in self.list()? {
            self.delete(&entry.id)?;
        }

        Ok(())
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.{FILE_EXTENSION}"))
    }
}

fn now_ms() -> u64 {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();

    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

// Ids are only digits and one dash, which is also what keeps `delete` from escaping the directory.
fn parse_id(id: &str) -> Option<(u64, u64)> {
    let (created_at_ms, session) = id.split_once('-')?;

    let all_digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if !all_digits(created_at_ms) || !all_digits(session) {
        return None;
    }

    Some((created_at_ms.parse().ok()?, session.parse().ok()?))
}

fn read_entry(path: &Path) -> Option<HistoryEntry> {
    if path.extension()?.to_str()? != FILE_EXTENSION {
        return None;
    }

    let id = path.file_stem()?.to_str()?.to_owned();
    let (created_at_ms, _) = parse_id(&id)?;
    let text = fs::read_to_string(path).ok()?;

    Some(HistoryEntry {
        id,
        created_at_ms,
        text,
    })
}
