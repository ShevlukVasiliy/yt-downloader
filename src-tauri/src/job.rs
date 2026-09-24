use crate::bin::{resolve, Tool};
use crate::probe::{friendly_error, is_cookie_client_rejection};
use crate::queue::{update_progress, JobProgress, JobStatus, KillReason, QueueManager};
use crate::runtime::resolve_runtime_env;
use crate::spec::to_argv;
use std::path::Path;
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

enum Attempt {
    Done,
    Killed(KillReason),
    /// Joined tail of yt-dlp's stderr.
    Failed(String),
    SpawnFailed(String),
}

/// One yt-dlp invocation: streams its progress into the job and stops it on a
/// pause/cancel request. `current_filename` outlives the attempt so a retry
/// keeps knowing which partial file belongs to this job.
async fn attempt(
    app: &AppHandle,
    job_id: &str,
    ytdlp: &Path,
    argv: &[String],
    kill_rx: &mut mpsc::Receiver<KillReason>,
    current_filename: &mut Option<String>,
) -> Attempt {
    let mut child = match Command::new(ytdlp)
        .args(argv)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return Attempt::SpawnFailed(format!("Не удалось запустить yt-dlp: {e}")),
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
    let mut kill_reason: Option<KillReason> = None;

    loop {
        tokio::select! {
            line = stdout_lines.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some(progress) = handle_download_line(&l, current_filename) {
                            update_progress(app, job_id, progress, JobStatus::Downloading).await;
                        } else if let Some(status) = handle_postprocess_line(&l, current_filename) {
                            update_progress(app, job_id, JobProgress::default(), status).await;
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

    if let Some(r) = kill_reason {
        return Attempt::Killed(r);
    }
    match exit_status {
        Ok(status) if status.success() => Attempt::Done,
        _ => Attempt::Failed(stderr_buf.lock().await.join("\n")),
    }
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
            finalize_error(&app, &job_id, e, None).await;
            return;
        }
    };

    let env = resolve_runtime_env(&app).await;
    let mut current_filename: Option<String> = None;

    let mut outcome = attempt(
        &app,
        &job_id,
        &ytdlp,
        &to_argv(&spec, &url, &env),
        &mut kill_rx,
        &mut current_filename,
    )
    .await;

    if let Attempt::Failed(first_stderr) = &outcome {
        if spec.network_opts().uses_cookies() && is_cookie_client_rejection(first_stderr) {
            // `--continue` in the argv resumes whatever the first attempt
            // already wrote to disk.
            let retry = attempt(
                &app,
                &job_id,
                &ytdlp,
                &to_argv(&spec.without_cookies(), &url, &env),
                &mut kill_rx,
                &mut current_filename,
            )
            .await;
            outcome = match retry {
                // A video that genuinely needs the account (private, age-gated)
                // still fails without cookies. Report the original error — the
                // no-cookies one would tell the user to enable cookies they
                // already have — but keep both stderr tails in the details.
                Attempt::Failed(retry_stderr) => {
                    let message = friendly_error(first_stderr);
                    let detail = format!("{first_stderr}\n\n--- повтор без cookies ---\n{retry_stderr}");
                    finalize_error(&app, &job_id, message, Some(detail)).await;
                    return;
                }
                other => other,
            };
        }
    }

    match outcome {
        Attempt::Killed(KillReason::Pause) => {
            finalize(&app, &job_id, JobStatus::Paused, None, None, None).await;
        }
        Attempt::Killed(KillReason::Cancel) => {
            cleanup_partial(&current_filename).await;
            finalize(&app, &job_id, JobStatus::Canceled, None, None, None).await;
        }
        Attempt::Done => {
            finalize(&app, &job_id, JobStatus::Done, current_filename, None, None).await;
        }
        Attempt::SpawnFailed(message) => {
            finalize_error(&app, &job_id, message, None).await;
        }
        Attempt::Failed(stderr) => {
            let message = friendly_error(&stderr);
            finalize_error(&app, &job_id, message, Some(stderr)).await;
        }
    }
}

async fn finalize(
    app: &AppHandle,
    job_id: &str,
    status: JobStatus,
    output_path: Option<String>,
    error: Option<String>,
    error_detail: Option<String>,
) {
    let qm = app.state::<QueueManager>();
    let mut jobs = qm.jobs.lock().await;
    if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
        job.status = status;
        if output_path.is_some() {
            job.output_path = output_path;
        }
        job.error = error;
        job.error_detail = error_detail.filter(|d| !d.trim().is_empty());
    }
}

async fn finalize_error(app: &AppHandle, job_id: &str, message: String, detail: Option<String>) {
    finalize(app, job_id, JobStatus::Error, None, Some(message), detail).await;
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
