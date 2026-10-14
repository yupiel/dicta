use std::path::PathBuf;

use tauri::{AppHandle, Manager};

pub fn data_subdir(app: &AppHandle, name: &str) -> Result<PathBuf, tauri::Error> {
    app.path().app_data_dir().map(|dir| dir.join(name))
}
