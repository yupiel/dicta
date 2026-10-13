mod capture;
mod commands;
mod recorder;
mod stream;

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
        .invoke_handler(tauri::generate_handler![
            commands::list_input_devices,
            commands::start_recording,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
