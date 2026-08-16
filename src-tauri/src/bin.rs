use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
    YtDlp,
    Ffmpeg,
    Ffprobe,
}

impl Tool {
    #[cfg(windows)]
    pub fn bin_name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp.exe",
            Tool::Ffmpeg => "ffmpeg.exe",
            Tool::Ffprobe => "ffprobe.exe",
        }
    }

    #[cfg(not(windows))]
    pub fn bin_name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ffprobe => "ffprobe",
        }
    }

    /// yt-dlp uses Python-argparse-style `--version`; ffmpeg/ffprobe use their own
    /// classic single-dash `-version` and reject the double-dash form outright.
    fn version_flag(self) -> &'static str {
        match self {
            Tool::YtDlp => "--version",
            Tool::Ffmpeg | Tool::Ffprobe => "-version",
        }
    }
}

#[derive(Default)]
pub struct BinCache(Mutex<HashMap<&'static str, (PathBuf, String)>>);

/// Candidate locations for a tool, checked in order:
/// 1. Bundled resource (production `.app` bundle)
/// 2. `src-tauri/binaries/` next to the crate (dev loop, before a real build)
/// 3. `$PATH`
/// 4. Well-known Homebrew / system install locations
fn candidates(app: &AppHandle, tool: Tool) -> Vec<PathBuf> {
    let name = tool.bin_name();
    let mut out = Vec::new();

    if let Ok(resource) = app
        .path()
        .resolve(format!("binaries/{name}"), BaseDirectory::Resource)
    {
        out.push(resource);
    }

    out.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries").join(name));

    out.push(PathBuf::from(name)); // resolved against $PATH by tokio::process::Command

    #[cfg(target_os = "macos")]
    for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        out.push(PathBuf::from(dir).join(name));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    for dir in ["/usr/local/bin", "/usr/bin", "/snap/bin"] {
        out.push(PathBuf::from(dir).join(name));
    }

    out
}

async fn probe(path: &PathBuf, tool: Tool) -> Option<String> {
    // A freshly downloaded/copied binary's first launch can hang on a Gatekeeper/
    // notarization check (observed ~20s wall time despite <1s CPU time), so probing
    // must not be allowed to block forever — and must not be worth repeating.
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(25),
        Command::new(path).arg(tool.version_flag()).output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    // ffmpeg/ffprobe print their version banner to stderr, not stdout.
    let text = if !output.stdout.is_empty() { &output.stdout } else { &output.stderr };
    Some(String::from_utf8_lossy(text).lines().next().unwrap_or_default().trim().to_string())
}

/// Resolves the runnable path for a tool, actually invoking `--version` to confirm
/// it works rather than trusting the file merely exists. Successful lookups (path +
/// version) are cached so later calls never re-exec the binary.
async fn resolve_verified(app: &AppHandle, tool: Tool) -> Result<(PathBuf, String), String> {
    if let Some(cached) = app.state::<BinCache>().0.lock().unwrap().get(tool.bin_name()).cloned() {
        return Ok(cached);
    }

    for candidate in candidates(app, tool) {
        if let Some(version) = probe(&candidate, tool).await {
            app.state::<BinCache>()
                .0
                .lock()
                .unwrap()
                .insert(tool.bin_name(), (candidate.clone(), version.clone()));
            return Ok((candidate, version));
        }
    }

    Err(format!("{} не найден", tool.bin_name()))
}

pub async fn resolve(app: &AppHandle, tool: Tool) -> Result<PathBuf, String> {
    resolve_verified(app, tool).await.map(|(path, _)| path)
}

pub fn invalidate(app: &AppHandle, tool: Tool) {
    app.state::<BinCache>().0.lock().unwrap().remove(tool.bin_name());
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub name: String,
    pub found: bool,
    pub version: Option<String>,
    pub bundled: bool,
    pub path: Option<String>,
}

pub async fn status(app: &AppHandle, tool: Tool) -> ToolStatus {
    match resolve_verified(app, tool).await {
        Ok((path, version)) => {
            let version = Some(version);
            let bundled = path.starts_with(
                app.path()
                    .resolve("binaries", BaseDirectory::Resource)
                    .unwrap_or_default(),
            ) || path.starts_with(env!("CARGO_MANIFEST_DIR"));
            ToolStatus {
                name: tool.bin_name().to_string(),
                found: true,
                version,
                bundled,
                path: Some(path.to_string_lossy().to_string()),
            }
        }
        Err(_) => ToolStatus {
            name: tool.bin_name().to_string(),
            found: false,
            version: None,
            bundled: false,
            path: None,
        },
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DepsStatus {
    pub yt_dlp: ToolStatus,
    pub ffmpeg: ToolStatus,
    pub ffprobe: ToolStatus,
    pub all_ready: bool,
}

#[tauri::command]
pub async fn check_deps(app: AppHandle) -> DepsStatus {
    let (yt_dlp, ffmpeg, ffprobe) = tokio::join!(
        status(&app, Tool::YtDlp),
        status(&app, Tool::Ffmpeg),
        status(&app, Tool::Ffprobe),
    );
    let all_ready = yt_dlp.found && ffmpeg.found && ffprobe.found;
    eprintln!(
        "[deps] yt-dlp found={} ({:?}) ffmpeg found={} ({:?}) ffprobe found={} ({:?}) all_ready={}",
        yt_dlp.found, yt_dlp.version, ffmpeg.found, ffmpeg.version, ffprobe.found, ffprobe.version, all_ready
    );
    DepsStatus {
        yt_dlp,
        ffmpeg,
        ffprobe,
        all_ready,
    }
}

#[tauri::command]
pub async fn update_yt_dlp(app: AppHandle) -> Result<String, String> {
    let path = resolve(&app, Tool::YtDlp).await?;
    let output = Command::new(&path)
        .arg("-U")
        .output()
        .await
        .map_err(|e| e.to_string())?;
    invalidate(&app, Tool::YtDlp);
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
