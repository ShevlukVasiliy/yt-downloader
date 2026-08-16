mod bin;
mod job;
mod probe;
mod queue;
mod settings;
mod spec;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(bin::BinCache::default())
        .manage(queue::QueueManager::new())
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                queue::load(&handle).await;
                if let Ok(s) = settings::get_settings(handle.clone()).await {
                    *handle.state::<queue::QueueManager>().concurrency.lock().await = s.concurrency.clamp(1, 5);
                }
                queue::emit_list(&handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bin::check_deps,
            bin::update_yt_dlp,
            probe::analyze_url,
            queue::get_jobs,
            queue::enqueue_jobs,
            queue::pause_job,
            queue::cancel_job,
            queue::retry_job,
            queue::remove_job,
            queue::set_concurrency,
            queue::reveal_in_folder,
            settings::get_settings,
            settings::save_settings,
            settings::pick_download_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
