use crate::job;
use crate::spec::DownloadSpec;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Queued,
    Downloading,
    Merging,
    Converting,
    Paused,
    Done,
    Error,
    Canceled,
}

#[derive(Clone, Copy, Debug)]
pub enum KillReason {
    Pause,
    Cancel,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub downloaded_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
    pub speed_bytes_per_sec: Option<f64>,
    pub eta_seconds: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub group_id: Option<String>,
    pub group_title: Option<String>,
    pub video_id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub url: String,
    pub spec: DownloadSpec,
    pub status: JobStatus,
    pub progress: JobProgress,
    pub error: Option<String>,
    pub output_path: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub retries_left: u32,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewJobRequest {
    pub group_id: Option<String>,
    pub group_title: Option<String>,
    pub video_id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub url: String,
    pub spec: DownloadSpec,
}

pub struct QueueManager {
    pub jobs: AsyncMutex<Vec<Job>>,
    pub running: AsyncMutex<HashMap<String, mpsc::Sender<KillReason>>>,
    pub concurrency: AsyncMutex<usize>,
    pub last_emit: AsyncMutex<HashMap<String, std::time::Instant>>,
}

impl QueueManager {
    pub fn new() -> Self {
        Self {
            jobs: AsyncMutex::new(Vec::new()),
            running: AsyncMutex::new(HashMap::new()),
            concurrency: AsyncMutex::new(3),
            last_emit: AsyncMutex::new(HashMap::new()),
        }
    }
}

fn queue_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("queue.json"))
}

pub async fn persist(app: &AppHandle) {
    let Some(path) = queue_file(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let qm = app.state::<QueueManager>();
    let jobs = qm.jobs.lock().await;
    if let Ok(json) = serde_json::to_vec_pretty(&*jobs) {
        let _ = tokio::fs::write(&path, json).await;
    }
}

pub async fn load(app: &AppHandle) {
    let Some(path) = queue_file(app) else { return };
    let Ok(bytes) = tokio::fs::read(&path).await else { return };
    let Ok(mut loaded): Result<Vec<Job>, _> = serde_json::from_slice(&bytes) else { return };
    for job in loaded.iter_mut() {
        if matches!(
            job.status,
            JobStatus::Queued | JobStatus::Downloading | JobStatus::Merging | JobStatus::Converting
        ) {
            job.status = JobStatus::Paused;
        }
    }
    *app.state::<QueueManager>().jobs.lock().await = loaded;
}

pub async fn emit_list(app: &AppHandle) {
    let jobs = app.state::<QueueManager>().jobs.lock().await.clone();
    let _ = app.emit("job://list", jobs);
}

pub async fn set_status(app: &AppHandle, job_id: &str, status: JobStatus) {
    let qm = app.state::<QueueManager>();
    let mut jobs = qm.jobs.lock().await;
    if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
        job.status = status;
    }
}

pub async fn update_progress(app: &AppHandle, job_id: &str, progress: JobProgress, status: JobStatus) {
    let qm = app.state::<QueueManager>();
    let status_changed = {
        let mut jobs = qm.jobs.lock().await;
        match jobs.iter_mut().find(|j| j.id == job_id) {
            Some(job) => {
                let changed = job.status != status;
                job.status = status.clone();
                job.progress = progress.clone();
                changed
            }
            None => return,
        }
    };

    let should_emit_progress = if status_changed {
        true
    } else {
        let mut last = qm.last_emit.lock().await;
        let now = std::time::Instant::now();
        let due = last
            .get(job_id)
            .map(|t| now.duration_since(*t).as_millis() >= 250)
            .unwrap_or(true);
        if due {
            last.insert(job_id.to_string(), now);
        }
        due
    };

    if should_emit_progress {
        let _ = app.emit(
            "job://progress",
            serde_json::json!({ "id": job_id, "status": status, "progress": progress }),
        );
    }
    if status_changed {
        emit_list(app).await;
    }
}

fn try_schedule(app: AppHandle) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(try_schedule_inner(app))
}

async fn try_schedule_inner(app: AppHandle) {
    loop {
        let next_id = {
            let qm = app.state::<QueueManager>();
            let jobs = qm.jobs.lock().await;
            let running = qm.running.lock().await;
            let concurrency = *qm.concurrency.lock().await;
            if running.len() >= concurrency {
                None
            } else {
                jobs.iter()
                    .find(|j| j.status == JobStatus::Queued && !running.contains_key(&j.id))
                    .map(|j| j.id.clone())
            }
        };
        match next_id {
            Some(id) => {
                let (tx, rx) = mpsc::channel::<KillReason>(1);
                app.state::<QueueManager>().running.lock().await.insert(id.clone(), tx);
                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    job::run(app2.clone(), id.clone(), rx).await;
                    app2.state::<QueueManager>().running.lock().await.remove(&id);

                    let should_auto_retry = {
                        let qm = app2.state::<QueueManager>();
                        let mut jobs = qm.jobs.lock().await;
                        match jobs.iter_mut().find(|j| j.id == id) {
                            Some(job) if job.status == JobStatus::Error && job.retries_left > 0 => {
                                job.retries_left -= 1;
                                job.status = JobStatus::Queued;
                                job.progress = JobProgress::default();
                                job.error = None;
                                true
                            }
                            _ => false,
                        }
                    };
                    if should_auto_retry {
                        // Brief backoff so a persistently failing job doesn't hot-loop.
                        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                    }

                    persist(&app2).await;
                    emit_list(&app2).await;
                    try_schedule(app2).await;
                });
            }
            None => break,
        }
    }
}

#[tauri::command]
pub async fn get_jobs(app: AppHandle) -> Vec<Job> {
    app.state::<QueueManager>().jobs.lock().await.clone()
}

#[tauri::command]
pub async fn enqueue_jobs(app: AppHandle, requests: Vec<NewJobRequest>) -> Result<Vec<Job>, String> {
    let ffmpeg_dir = crate::bin::resolve(&app, crate::bin::Tool::Ffmpeg)
        .await
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_string_lossy().to_string()));
    let auto_retry_attempts = crate::settings::get_settings(app.clone())
        .await
        .map(|s| s.auto_retry_attempts)
        .unwrap_or(2);

    let mut created = Vec::with_capacity(requests.len());
    {
        let qm = app.state::<QueueManager>();
        let mut jobs = qm.jobs.lock().await;
        for req in requests {
            let mut spec = req.spec;
            if spec.ffmpeg_dir.is_none() {
                spec.ffmpeg_dir = ffmpeg_dir.clone();
            }
            if spec.download_archive_path.is_none() {
                spec.download_archive_path = Some(format!(
                    "{}/.yt-downloader-archive.txt",
                    spec.output_dir.trim_end_matches('/')
                ));
            }
            let job = Job {
                id: uuid::Uuid::new_v4().to_string(),
                group_id: req.group_id,
                group_title: req.group_title,
                video_id: req.video_id,
                title: req.title,
                thumbnail: req.thumbnail,
                url: req.url,
                spec,
                status: JobStatus::Queued,
                progress: JobProgress::default(),
                error: None,
                output_path: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                retries_left: auto_retry_attempts,
            };
            created.push(job.clone());
            jobs.push(job);
        }
    }
    persist(&app).await;
    emit_list(&app).await;
    tauri::async_runtime::spawn(try_schedule(app.clone()));
    Ok(created)
}

#[tauri::command]
pub async fn pause_job(app: AppHandle, id: String) -> Result<(), String> {
    let qm = app.state::<QueueManager>();
    let sender = qm.running.lock().await.get(&id).cloned();
    match sender {
        Some(tx) => {
            let _ = tx.send(KillReason::Pause).await;
        }
        None => {
            set_status(&app, &id, JobStatus::Paused).await;
            persist(&app).await;
            emit_list(&app).await;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_job(app: AppHandle, id: String) -> Result<(), String> {
    let qm = app.state::<QueueManager>();
    let sender = qm.running.lock().await.get(&id).cloned();
    match sender {
        Some(tx) => {
            let _ = tx.send(KillReason::Cancel).await;
        }
        None => {
            set_status(&app, &id, JobStatus::Canceled).await;
            persist(&app).await;
            emit_list(&app).await;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn retry_job(app: AppHandle, id: String) -> Result<(), String> {
    let auto_retry_attempts = crate::settings::get_settings(app.clone())
        .await
        .map(|s| s.auto_retry_attempts)
        .unwrap_or(2);
    {
        let qm = app.state::<QueueManager>();
        let mut jobs = qm.jobs.lock().await;
        if let Some(job) = jobs.iter_mut().find(|j| j.id == id) {
            job.status = JobStatus::Queued;
            job.progress = JobProgress::default();
            job.error = None;
            job.retries_left = auto_retry_attempts;
        }
    }
    persist(&app).await;
    emit_list(&app).await;
    tauri::async_runtime::spawn(try_schedule(app.clone()));
    Ok(())
}

#[tauri::command]
pub async fn remove_job(app: AppHandle, id: String) -> Result<(), String> {
    // Removing a job that's still running left its yt-dlp process orphaned
    // (downloading to disk with no queue entry tracking it) — stop it first.
    let sender = app.state::<QueueManager>().running.lock().await.get(&id).cloned();
    if let Some(tx) = sender {
        let _ = tx.send(KillReason::Cancel).await;
    }
    {
        let qm = app.state::<QueueManager>();
        let mut jobs = qm.jobs.lock().await;
        jobs.retain(|j| j.id != id);
    }
    persist(&app).await;
    emit_list(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn set_concurrency(app: AppHandle, value: usize) -> Result<(), String> {
    *app.state::<QueueManager>().concurrency.lock().await = value.clamp(1, 5);
    tauri::async_runtime::spawn(try_schedule(app.clone()));
    Ok(())
}

#[tauri::command]
pub async fn reveal_in_folder(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .reveal_item_in_dir(PathBuf::from(path))
        .map_err(|e| e.to_string())
}
