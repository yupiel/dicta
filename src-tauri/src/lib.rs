use std::sync::Arc;

use tauri::Manager;

use crate::{
    capture::CpalSource, history::HistoryStore, live_text::LiveTextStore, models::DownloadGate,
    recorder::Recorder,
};

mod auto;
mod capture;
mod commands;
mod events;
mod history;
mod live_text;
mod models;
mod pipeline;
mod recorder;
mod resample;
mod segmenter;
mod stream;
mod whisper;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    run_prelude();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let history = HistoryStore::new(app.path().app_data_dir()?.join("history"));
            app.manage(history.clone());
            app.manage(DownloadGate::default());

            let live_text = LiveTextStore::default();
            app.manage(live_text.clone());

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
