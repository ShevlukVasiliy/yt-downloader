use crate::bin::{resolve, Tool};
use crate::probe::friendly_error;
use crate::queue::{update_progress, JobProgress, JobStatus, KillReason, QueueManager};
use crate::spec::to_argv;
use std::sync::Arc;
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

async fn cleanup_partial(filename: &Option<String>) {
    let Some(f) = filename else { return };
    for suffix in [".part", ".ytdl", ""] {
        let _ = tokio::fs::remove_file(format!("{f}{suffix}")).await;
    }
}

fn handle_download_line(line: &str, current_filename: &mut Option<String>) -> Option<JobProgress> {
    let rest = line.strip_prefix("[DLPROG]")?;
    let parts: Vec<&str> = rest.splitn(7, '|').collect();
    if parts.len() != 7 {
        return None;
    }
    let downloaded_bytes = parts[1].parse::<u64>().ok();
    let total_bytes = parts[2]
        .parse::<u64>()
        .ok()
        .or_else(|| parts[3].parse::<u64>().ok());
    let speed_bytes_per_sec = parts[4].parse::<f64>().ok();
    let eta_seconds = parts[5].parse::<u64>().ok();
    if !parts[6].is_empty() && parts[6] != "NA" {
        *current_filename = Some(parts[6].to_string());
    }
    Some(JobProgress {
        downloaded_bytes,
        total_bytes,
        speed_bytes_per_sec,
        eta_seconds,
    })
}

fn handle_postprocess_line(line: &str, current_filename: &mut Option<String>) -> Option<JobStatus> {
    let rest = line.strip_prefix("[PPPROG]")?;
    let parts: Vec<&str> = rest.splitn(3, '|').collect();
    if parts.len() != 3 {
        return None;
    }
    if !parts[2].is_empty() && parts[2] != "NA" {
        *current_filename = Some(parts[2].to_string());
    }
    let postprocessor = parts[1].to_lowercase();
    Some(if postprocessor.contains("merg") {
        JobStatus::Merging
    } else {
        JobStatus::Converting
    })
}

pub async fn run(app: AppHandle, job_id: String, mut kill_rx: mpsc::Receiver<KillReason>) {
    let (url, mut spec) = {
        let qm = app.state::<QueueManager>();
        let jobs = qm.jobs.lock().await;
        match jobs.iter().find(|j| j.id == job_id) {
            Some(j) => (j.url.clone(), j.spec.clone()),
            None => return,
        }
    };

    // The configured rate limit is a total budget shared across every
    // simultaneous download, not a per-process cap — otherwise N parallel
    // downloads could together use up to N times the intended bandwidth,
    // and whichever one grabs the connection first starves the others.
    // yt-dlp has no live "adjust rate mid-download" control, so this splits
    // evenly by the concurrency setting once, at job start.
    let concurrency = *app.state::<QueueManager>().concurrency.lock().await;
    spec.rate_limit_kbps = split_rate_limit(spec.rate_limit_kbps, concurrency);

    let ytdlp = match resolve(&app, Tool::YtDlp).await {
        Ok(p) => p,
        Err(e) => {
            finalize_error(&app, &job_id, e).await;
            return;
        }
    };

    let argv = to_argv(&spec, &url);

    let mut child = match Command::new(&ytdlp)
        .args(&argv)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            finalize_error(&app, &job_id, format!("Не удалось запустить yt-dlp: {e}")).await;
            return;
        }
    };

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let stderr_buf = Arc::new(AsyncMutex::new(Vec::<String>::new()));
    let stderr_buf2 = stderr_buf.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut buf = stderr_buf2.lock().await;
            if buf.len() >= 50 {
                buf.remove(0);
            }
            buf.push(line);
        }
    });

    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut current_filename: Option<String> = None;
    let mut kill_reason: Option<KillReason> = None;

    loop {
        tokio::select! {
            line = stdout_lines.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some(progress) = handle_download_line(&l, &mut current_filename) {
                            update_progress(&app, &job_id, progress, JobStatus::Downloading).await;
                        } else if let Some(status) = handle_postprocess_line(&l, &mut current_filename) {
                            update_progress(&app, &job_id, JobProgress::default(), status).await;
                        }
                    }
                    _ => break,
                }
            }
            reason = kill_rx.recv() => {
                if let Some(r) = reason {
                    let _ = child.start_kill();
                    kill_reason = Some(r);
                }
                break;
            }
        }
    }

    let exit_status = child.wait().await;
    let _ = stderr_task.await;

    match kill_reason {
        Some(KillReason::Pause) => {
            finalize(&app, &job_id, JobStatus::Paused, None, None).await;
        }
        Some(KillReason::Cancel) => {
            cleanup_partial(&current_filename).await;
            finalize(&app, &job_id, JobStatus::Canceled, None, None).await;
        }
        None => match exit_status {
            Ok(status) if status.success() => {
                finalize(&app, &job_id, JobStatus::Done, current_filename, None).await;
            }
            _ => {
                let lines = stderr_buf.lock().await.clone();
                let message = friendly_error(&lines.join("\n"));
                finalize_error(&app, &job_id, message).await;
            }
        },
    }
}

async fn finalize(
    app: &AppHandle,
    job_id: &str,
    status: JobStatus,
    output_path: Option<String>,
    error: Option<String>,
) {
    let qm = app.state::<QueueManager>();
    let mut jobs = qm.jobs.lock().await;
    if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
        job.status = status;
        if output_path.is_some() {
            job.output_path = output_path;
        }
        job.error = error;
    }
}

async fn finalize_error(app: &AppHandle, job_id: &str, message: String) {
    finalize(app, job_id, JobStatus::Error, None, Some(message)).await;
}

/// Divides a total bandwidth budget evenly across concurrent download slots.
/// `None` (no limit configured) stays `None`; a configured limit never drops
/// below 1 KB/s even at high concurrency, so it stays a real (if slow) cap
/// rather than silently becoming unlimited via integer division to zero.
fn split_rate_limit(total_kbps: Option<u32>, concurrency: usize) -> Option<u32> {
    total_kbps.map(|total| (total / concurrency.max(1) as u32).max(1))
}

#[cfg(test)]
mod split_rate_limit_tests {
    use super::*;

    #[test]
    fn no_limit_stays_unlimited() {
        assert_eq!(split_rate_limit(None, 3), None);
    }

    #[test]
    fn splits_evenly_across_concurrency() {
        assert_eq!(split_rate_limit(Some(3000), 3), Some(1000));
        assert_eq!(split_rate_limit(Some(1000), 1), Some(1000));
    }

    #[test]
    fn rounds_down_but_never_below_one_kbps() {
        assert_eq!(split_rate_limit(Some(2), 5), Some(1));
    }

    #[test]
    fn zero_concurrency_treated_as_one() {
        assert_eq!(split_rate_limit(Some(500), 0), Some(500));
    }
}
