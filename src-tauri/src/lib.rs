use std::sync::Arc;

use tauri::Manager;

use crate::{
    capture::CpalSource, history::HistoryStore, hotkey::InsertShortcut, live_text::LiveTextStore,
    models::DownloadGate, recorder::Recorder,
};

mod auto;
mod capture;
mod commands;
mod events;
mod history;
mod hotkey;
mod insert;
mod live_text;
mod models;
mod pipeline;
mod recorder;
mod resample;
mod segmenter;
mod stream;
mod whisper;

#[cfg(test)]
mod test_support;

fn run_prelude() {
    #[cfg(target_os = "linux")]
    // Technically this issue is NVidia only but that's most GPUs
    // See: https://github.com/tauri-apps/tauri/issues/9394
    if std::env::var("DICTA_HWACCEL").is_err() {
        // Unsafe here is only required since version 2024
        // Technically I don't need linux support so this is for debugging purposes
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
}

const INSERT_SHORTCUT_FILE: &str = "insert-shortcut.txt";

fn init_shortcut_plugin(app: &tauri::App) -> Result<(), String> {
    use tauri_plugin_global_shortcut::{Builder, ShortcutState};

    app.handle()
        .plugin(
            Builder::new()
                .with_handler(|app, _, event| {
                    if event.state() == ShortcutState::Pressed {
                        hotkey::insert_current_text(app);
                    }
                })
                .build(),
        )
        .map_err(|error| format!("global shortcuts are unavailable: {error}"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    run_prelude();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let history = HistoryStore::new(app.path().app_data_dir()?.join("history"));
            app.manage(history.clone());
            app.manage(DownloadGate::default());

            let live_text = LiveTextStore::default();
            app.manage(live_text.clone());

            let insert_shortcut =
                InsertShortcut::load(app.path().app_data_dir()?.join(INSERT_SHORTCUT_FILE));
            match init_shortcut_plugin(app) {
                Ok(()) => insert_shortcut.register_saved(app.handle()),
                Err(message) => insert_shortcut.fail(message),
            }
            app.manage(insert_shortcut);

            let handle = app.handle().clone();
            let recorder = Recorder::new(
                Arc::new(move |event| {
                    events::handle_recorder_event(&handle, &history, &live_text, event)
                }),
                Arc::new(CpalSource),
            );
            app.manage(recorder);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_input_devices,
            commands::load_model,
            commands::start_recording,
            commands::stop_recording,
            commands::get_status,
            commands::reset_transcript,
            commands::list_history,
            commands::delete_history,
            commands::clear_history,
            commands::reveal_history_entry,
            commands::list_models,
            commands::download_model,
            commands::set_insert_shortcut,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
