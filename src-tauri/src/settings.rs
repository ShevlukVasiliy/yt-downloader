use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

fn default_network_retries() -> u32 {
    10
}

fn default_auto_retry_attempts() -> u32 {
    2
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: String,
    pub filename_template: String,
    pub concurrency: usize,
    pub rate_limit_kbps: Option<u32>,
    pub proxy: Option<String>,
    pub cookies_from_browser: Option<String>,
    /// A `cookies.txt` file, preferred over `cookies_from_browser` when both are
    /// set (see `spec::network_args`).
    #[serde(default)]
    pub cookies_file: Option<String>,
    pub theme: String,
    pub locale: String,
    /// Passed to yt-dlp's own --retries/--fragment-retries for transient network hiccups
    /// within a single attempt.
    #[serde(default = "default_network_retries")]
    pub network_retries: u32,
    /// How many times the queue automatically re-queues a job that ended in Error,
    /// before leaving it for the user to retry manually.
    #[serde(default = "default_auto_retry_attempts")]
    pub auto_retry_attempts: u32,
}

impl Settings {
    fn with_default_dir(app: &AppHandle) -> Self {
        let download_dir = app
            .path()
            .download_dir()
            .map(|d| d.join("YouTube").to_string_lossy().to_string())
            .unwrap_or_default();
        Self {
            download_dir,
            filename_template: "%(title)s.%(ext)s".into(),
            concurrency: 3,
            rate_limit_kbps: None,
            proxy: None,
            cookies_from_browser: None,
            cookies_file: None,
            theme: "dark".into(),
            locale: "ru".into(),
            network_retries: default_network_retries(),
            auto_retry_attempts: default_auto_retry_attempts(),
        }
    }
}

const STORE_FILE: &str = "settings.json";
const KEY: &str = "settings";

#[tauri::command]
pub async fn get_settings(app: AppHandle) -> Result<Settings, String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    match store.get(KEY) {
        Some(v) => serde_json::from_value(v.clone()).or_else(|_| Ok(Settings::with_default_dir(&app))),
        None => Ok(Settings::with_default_dir(&app)),
    }
}

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    store.set(
        KEY.to_string(),
        serde_json::to_value(&settings).map_err(|e| e.to_string())?,
    );
    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn pick_download_dir(app: AppHandle, default_path: Option<String>) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let mut builder = app.dialog().file();
    if let Some(p) = default_path {
        builder = builder.set_directory(p);
    }
    let (tx, rx) = tokio::sync::oneshot::channel();
    builder.pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let result = rx.await.map_err(|e| e.to_string())?;
    Ok(result.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn pick_cookies_file(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let builder = app.dialog().file().add_filter("cookies.txt", &["txt"]);
    let (tx, rx) = tokio::sync::oneshot::channel();
    builder.pick_file(move |file| {
        let _ = tx.send(file);
    });
    let result = rx.await.map_err(|e| e.to_string())?;
    Ok(result.map(|p| p.to_string()))
}
