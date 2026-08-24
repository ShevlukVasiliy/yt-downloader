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
    /// quickjs-ng: the JS runtime yt-dlp needs to solve YouTube's JS challenges
    /// (nsig/sig) since 2025.11.12. See src/runtime.rs.
    Qjs,
    /// PO-token provider (bgutil-ytdlp-pot-provider-rs), run as a local HTTP server.
    /// See src/pot.rs.
    BgutilPot,
}

impl Tool {
    #[cfg(windows)]
    pub fn bin_name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp.exe",
            Tool::Ffmpeg => "ffmpeg.exe",
            Tool::Ffprobe => "ffprobe.exe",
            Tool::Qjs => "qjs.exe",
            Tool::BgutilPot => "bgutil-pot.exe",
        }
    }

    #[cfg(not(windows))]
    pub fn bin_name(self) -> &'static str {
        match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ffprobe => "ffprobe",
            Tool::Qjs => "qjs",
            Tool::BgutilPot => "bgutil-pot",
        }
    }

    /// yt-dlp/qjs/bgutil-pot all use `--version`; ffmpeg/ffprobe use their own
    /// classic single-dash `-version` and reject the double-dash form outright.
    fn version_flag(self) -> &'static str {
        match self {
            Tool::Ffmpeg | Tool::Ffprobe => "-version",
            Tool::YtDlp | Tool::Qjs | Tool::BgutilPot => "--version",
        }
    }
}

#[derive(Default)]
pub struct BinCache(Mutex<HashMap<&'static str, (PathBuf, String)>>);

/// Where `update_yt_dlp` writes a freshly self-updated binary. Never write into
/// the bundled resource path directly: on macOS that path lives inside the
/// signed `.app`, and yt-dlp's `-U` rewriting it in place breaks the bundle's
/// code signature (the binary then fails to launch at all).
fn appdata_bin_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("bin"))
}

/// Candidate locations for a tool, checked in order:
/// 1. Self-updated copy in the app's data dir (see `appdata_bin_dir`)
/// 2. Bundled resource (production `.app` bundle)
/// 3. `src-tauri/binaries/` next to the crate (dev loop, before a real build)
/// 4. `$PATH`
/// 5. Well-known Homebrew / system install locations
fn candidates(app: &AppHandle, tool: Tool) -> Vec<PathBuf> {
    let name = tool.bin_name();
    let mut out = Vec::new();

    if let Some(dir) = appdata_bin_dir(app) {
        out.push(dir.join(name));
    }

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
    /// The external JS runtime yt-dlp needs to solve YouTube's challenges
    /// (bundled quickjs-ng, or a faster system deno if one's on PATH). Blocking:
    /// without it YouTube extraction silently loses formats and misfires as a
    /// bot-check.
    pub js_runtime: ToolStatus,
    /// PO-token provider (bgutil-pot). Non-blocking: some formats/clients work
    /// without it, so its absence shouldn't gate `all_ready`.
    pub pot_provider: ToolStatus,
    pub all_ready: bool,
}

#[tauri::command]
pub async fn check_deps(app: AppHandle) -> DepsStatus {
    let (yt_dlp, ffmpeg, ffprobe, pot_provider) = tokio::join!(
        status(&app, Tool::YtDlp),
        status(&app, Tool::Ffmpeg),
        status(&app, Tool::Ffprobe),
        status(&app, Tool::BgutilPot),
    );
    let js_runtime = crate::runtime::js_runtime_status(&app).await;
    let all_ready = yt_dlp.found && ffmpeg.found && ffprobe.found && js_runtime.found;
    eprintln!(
        "[deps] yt-dlp found={} ({:?}) ffmpeg found={} ({:?}) ffprobe found={} ({:?}) js_runtime found={} ({:?}) pot found={} ({:?}) all_ready={}",
        yt_dlp.found, yt_dlp.version, ffmpeg.found, ffmpeg.version, ffprobe.found, ffprobe.version,
        js_runtime.found, js_runtime.version, pot_provider.found, pot_provider.version, all_ready
    );
    DepsStatus {
        yt_dlp,
        ffmpeg,
        ffprobe,
        js_runtime,
        pot_provider,
        all_ready,
    }
}

#[tauri::command]
pub async fn update_yt_dlp(app: AppHandle) -> Result<String, String> {
    let current = resolve(&app, Tool::YtDlp).await?;
    let appdata_dir = appdata_bin_dir(&app).ok_or_else(|| "Не удалось определить каталог данных приложения".to_string())?;
    tokio::fs::create_dir_all(&appdata_dir)
        .await
        .map_err(|e| e.to_string())?;
    let target = appdata_dir.join(Tool::YtDlp.bin_name());

    if target != current {
        tokio::fs::copy(&current, &target).await.map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&target)
                .await
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            tokio::fs::set_permissions(&target, perms)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    let output = Command::new(&target)
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
