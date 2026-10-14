use crate::bin::{resolve, Tool};
use crate::probe::{friendly_error, is_cookie_client_rejection, is_permanent_failure, DISK_FULL_MESSAGE};
use crate::queue::{update_progress, JobProgress, JobStatus, KillReason, QueueManager};
use crate::runtime::resolve_runtime_env;
use crate::spec::{to_argv, Mode, SubtitleFormat};
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
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
        ..Default::default()
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

/// yt-dlp's announcement of each subtitle file — the only place a
/// `--skip-download` run reports where it wrote anything.
const SUBTITLE_LINE: &str = "[info] Writing video subtitles to: ";

/// Turns what a subtitles-only run wrote into the files the user asked for:
/// follows `--convert-subs` renames, folds the `<lang>-orig` duplicate of an
/// auto track into `<lang>`, and strips timing for plain text. Returns the
/// final paths.
async fn finish_subtitles(written: &[String], format: SubtitleFormat) -> Vec<String> {
    let target_ext = if format == SubtitleFormat::Vtt { "vtt" } else { "srt" };
    let mut converted: Vec<std::path::PathBuf> = written
        .iter()
        .map(|p| Path::new(p).with_extension(target_ext))
        .filter(|p| p.exists())
        .collect();
    converted.dedup();

    let mut finals = Vec::new();
    for path in &converted {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
        let mut path = path.clone();
        if let Some(base) = stem.strip_suffix("-orig") {
            let plain = path.with_file_name(format!("{base}.{target_ext}"));
            if converted.contains(&plain) {
                let _ = tokio::fs::remove_file(&path).await;
                continue;
            }
            if tokio::fs::rename(&path, &plain).await.is_ok() {
                path = plain;
            }
        }
        if format == SubtitleFormat::Txt {
            let Ok(srt) = tokio::fs::read_to_string(&path).await else { continue };
            let txt_path = path.with_extension("txt");
            if tokio::fs::write(&txt_path, srt_to_text(&srt)).await.is_ok() {
                let _ = tokio::fs::remove_file(&path).await;
                path = txt_path;
            }
        }
        finals.push(path.to_string_lossy().to_string());
    }
    finals
}

/// SRT → plain text: drops cue numbers, timings and markup, and collapses
/// the rolling repeats auto-captions are made of.
fn srt_to_text(srt: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for raw in srt.lines() {
        let line = raw.trim();
        if line.is_empty() || line.contains("-->") || line.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let mut clean = String::new();
        let mut in_tag = false;
        for c in line.chars() {
            match c {
                '<' => in_tag = true,
                '>' => in_tag = false,
                _ if !in_tag => clean.push(c),
                _ => {}
            }
        }
        let clean = clean.trim().to_string();
        if !clean.is_empty() && lines.last() != Some(&clean) {
            lines.push(clean);
        }
    }
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

/// Below this much free space a download isn't even started.
const MIN_FREE_BYTES: u64 = 200 * 1024 * 1024;
/// Head-room kept on top of the estimate when checking a stream's size.
const FREE_MARGIN_BYTES: u64 = 50 * 1024 * 1024;

/// Free bytes on the volume holding `dir` — walks up to the nearest existing
/// ancestor since yt-dlp creates the folder (e.g. a playlist subfolder) itself.
pub fn free_space(dir: &str) -> Option<u64> {
    Path::new(dir)
        .ancestors()
        .find(|p| p.exists())
        .and_then(|p| fs4::available_space(p).ok())
}

/// Space a stream still needs: what's left to download, plus a full copy of
/// it when ffmpeg will later merge the streams into a new file next to them.
fn space_needed(progress: &JobProgress, merges: bool) -> Option<u64> {
    let total = progress.total_bytes?;
    let remaining = total.saturating_sub(progress.downloaded_bytes.unwrap_or(0));
    Some(remaining + if merges { total } else { 0 })
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DiskFullEvent {
    dir: String,
    free_bytes: Option<u64>,
    needed_bytes: Option<u64>,
}

enum Attempt {
    Done,
    /// Stopped before it filled the disk: `needed` bytes wouldn't fit.
    NoSpace(u64),
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
    subtitle_files: &mut Vec<String>,
    space_check: Option<(&str, bool)>,
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
    let mut no_space: Option<u64> = None;
    // Each stream (video, then audio) is checked once, as soon as yt-dlp
    // reports its size.
    let mut checked_stream: Option<String> = None;
    let mut streams_seen: Vec<String> = Vec::new();
    let merges = space_check.is_some_and(|(_, m)| m);

    loop {
        tokio::select! {
            line = stdout_lines.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some(path) = l.strip_prefix(SUBTITLE_LINE) {
                            subtitle_files.push(path.trim().to_string());
                        } else if let Some(mut progress) = handle_download_line(&l, current_filename) {
                            if let Some(f) = current_filename.as_ref() {
                                if !streams_seen.contains(f) {
                                    streams_seen.push(f.clone());
                                }
                            }
                            if merges {
                                let part = streams_seen.len().max(1) as u32;
                                progress.part = Some(part);
                                progress.parts = Some(part.max(2));
                            }
                            if let Some((dir, merges)) = space_check {
                                if progress.total_bytes.is_some() && checked_stream != *current_filename {
                                    checked_stream = current_filename.clone();
                                    let needed = space_needed(&progress, merges);
                                    if let (Some(needed), Some(free)) = (needed, free_space(dir)) {
                                        if free < needed + FREE_MARGIN_BYTES {
                                            let _ = child.start_kill();
                                            no_space = Some(needed);
                                            break;
                                        }
                                    }
                                }
                            }
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
    if let Some(needed) = no_space {
        return Attempt::NoSpace(needed);
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

    let space_check = (spec.mode != Mode::Subtitles)
        .then(|| (spec.output_dir.as_str(), spec.mode == Mode::Video && !spec.video_only));
    if space_check.is_some() {
        if let Some(free) = free_space(&spec.output_dir) {
            if free < MIN_FREE_BYTES {
                fail_disk_full(&app, &job_id, &spec.output_dir, None, None).await;
                return;
            }
        }
    }

    let env = resolve_runtime_env(&app).await;
    let mut current_filename: Option<String> = None;
    let mut subtitle_files: Vec<String> = Vec::new();

    let mut outcome = attempt(
        &app,
        &job_id,
        &ytdlp,
        &to_argv(&spec, &url, &env),
        &mut kill_rx,
        &mut current_filename,
        &mut subtitle_files,
        space_check,
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
                &mut subtitle_files,
                space_check,
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
        Attempt::Done if spec.mode == Mode::Subtitles => {
            let files = finish_subtitles(&subtitle_files, spec.subtitle_format).await;
            match files.into_iter().next() {
                Some(first) => finalize(&app, &job_id, JobStatus::Done, Some(first), None, None).await,
                None => {
                    let message = if spec.extras.subtitles.auto {
                        "У этого видео нет субтитров на выбранных языках"
                    } else {
                        "Обычных субтитров нет — включите «Автоматические субтитры» и повторите"
                    };
                    finalize_error(&app, &job_id, message.into(), None).await
                }
            }
        }
        Attempt::Done => {
            finalize(&app, &job_id, JobStatus::Done, current_filename, None, None).await;
        }
        Attempt::NoSpace(needed) => {
            fail_disk_full(&app, &job_id, &spec.output_dir, Some(needed), None).await;
        }
        Attempt::SpawnFailed(message) => {
            finalize_error(&app, &job_id, message, None).await;
        }
        Attempt::Failed(stderr) if crate::probe::is_disk_full(&stderr) => {
            // The half-written file only takes up the space the user is
            // about to free; a retry starts it over anyway.
            cleanup_partial(&current_filename).await;
            fail_disk_full(&app, &job_id, &spec.output_dir, None, Some(stderr)).await;
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
    let kind = detail.as_deref().filter(|d| is_permanent_failure(d)).map(|_| "permanent");
    finalize(app, job_id, JobStatus::Error, None, Some(message), detail).await;
    set_error_kind(app, job_id, kind).await;
}

async fn set_error_kind(app: &AppHandle, job_id: &str, kind: Option<&str>) {
    let qm = app.state::<QueueManager>();
    let mut jobs = qm.jobs.lock().await;
    if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
        job.error_kind = kind.map(str::to_string);
    }
}

/// Fails the job as "disk full" and tells the UI to show the free-up-space
/// dialog. `needed` is set when the check caught it before the disk filled.
async fn fail_disk_full(app: &AppHandle, job_id: &str, dir: &str, needed: Option<u64>, detail: Option<String>) {
    let free = free_space(dir);
    let detail = detail.or_else(|| {
        Some(format!(
            "Папка: {dir}\nСвободно: {} МБ{}",
            free.map(|f| f / 1024 / 1024).unwrap_or(0),
            needed.map(|n| format!("\nНужно: ~{} МБ", n / 1024 / 1024)).unwrap_or_default()
        ))
    });
    finalize(app, job_id, JobStatus::Error, None, Some(DISK_FULL_MESSAGE.into()), detail).await;
    set_error_kind(app, job_id, Some("diskFull")).await;
    let _ = app.emit(
        "job://disk-full",
        DiskFullEvent { dir: dir.to_string(), free_bytes: free, needed_bytes: needed },
    );
}

/// Divides a total bandwidth budget evenly across concurrent download slots.
/// `None` (no limit configured) stays `None`; a configured limit never drops
/// below 1 KB/s even at high concurrency, so it stays a real (if slow) cap
/// rather than silently becoming unlimited via integer division to zero.
fn split_rate_limit(total_kbps: Option<u32>, concurrency: usize) -> Option<u32> {
    total_kbps.map(|total| (total / concurrency.max(1) as u32).max(1))
}

#[cfg(test)]
mod subtitle_tests {
    use super::*;

    #[test]
    fn srt_becomes_plain_text_without_rolling_repeats() {
        let srt = "1\n00:00:00,000 --> 00:00:02,000\nhello <c>there</c>\n\n2\n00:00:02,000 --> 00:00:04,000\nhello there\nnext line\n";
        assert_eq!(srt_to_text(srt), "hello there\nnext line\n");
    }

    #[tokio::test]
    async fn orig_track_is_folded_into_plain_language_and_txt_written() {
        let dir = std::env::temp_dir().join(format!("subs-test-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let srt = "1\n00:00:00,000 --> 00:00:01,000\nhi\n";
        tokio::fs::write(dir.join("v.en-orig.srt"), srt).await.unwrap();
        tokio::fs::write(dir.join("v.ru.srt"), srt).await.unwrap();
        let written = vec![
            dir.join("v.en-orig.vtt").to_string_lossy().to_string(),
            dir.join("v.ru.vtt").to_string_lossy().to_string(),
        ];
        let mut out = finish_subtitles(&written, SubtitleFormat::Txt).await;
        out.sort();
        assert_eq!(
            out,
            vec![
                dir.join("v.en.txt").to_string_lossy().to_string(),
                dir.join("v.ru.txt").to_string_lossy().to_string()
            ]
        );
        assert_eq!(tokio::fs::read_to_string(dir.join("v.en.txt")).await.unwrap(), "hi\n");
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
}

#[cfg(test)]
mod space_tests {
    use super::*;

    #[test]
    fn merge_needs_room_for_the_merged_copy() {
        let p = JobProgress { downloaded_bytes: Some(100), total_bytes: Some(1000), ..Default::default() };
        assert_eq!(space_needed(&p, false), Some(900));
        assert_eq!(space_needed(&p, true), Some(1900));
        assert_eq!(space_needed(&JobProgress::default(), true), None);
    }

    #[test]
    fn free_space_resolves_missing_subfolder() {
        let dir = std::env::temp_dir().join("no-such-dir-yet/nested");
        assert!(free_space(dir.to_str().unwrap()).is_some());
    }
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
