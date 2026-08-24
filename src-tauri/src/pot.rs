use crate::bin::{resolve, Tool};
use std::path::PathBuf;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Info needed to point yt-dlp at the running PO-token provider.
#[derive(Clone)]
pub struct PotInfo {
    pub base_url: String,
    pub plugin_dir: PathBuf,
    pub cli_path: PathBuf,
}

struct Running {
    _child: Child,
    info: PotInfo,
}

/// Lazily-started PO-token provider server, shared by every analyze/download
/// call for the app's lifetime. `kill_on_drop` on the child means it dies with
/// the app; there's no explicit shutdown command because the app never needs
/// to stop it independently.
#[derive(Default)]
pub struct PotServer(Mutex<Option<Running>>);

/// Locates the bundled plugin package directory. yt-dlp's `--plugin-dirs X`
/// lists the *entries inside* X and, for each, checks whether it contains a
/// `yt_dlp_plugins/` folder — so the resource layout must be
/// `pot-plugin/bgutil/yt_dlp_plugins/...` and we pass `--plugin-dirs
/// <resource>/pot-plugin` (the parent), not the `bgutil` folder itself.
fn plugin_root(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .resolve("pot-plugin", BaseDirectory::Resource)
        .ok()
        .filter(|p| p.is_dir())
        .or_else(|| {
            let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("pot-plugin");
            dev.is_dir().then_some(dev)
        })
}

/// Binds an ephemeral local port and immediately releases it, so the server
/// process can bind the same number. Small TOCTOU race, acceptable here: this
/// is a single local helper process, not a security boundary.
fn free_port() -> Option<u16> {
    std::net::TcpListener::bind("127.0.0.1:0")
        .ok()?
        .local_addr()
        .ok()
        .map(|a| a.port())
}

async fn start(app: &AppHandle) -> Option<Running> {
    let cli_path = resolve(app, Tool::BgutilPot).await.ok()?;
    let plugin_dir = plugin_root(app)?;
    let port = free_port()?;

    let child = Command::new(&cli_path)
        .args(["server", "--host", "127.0.0.1", "--port", &port.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;

    // No HTTP client bundled to poll readiness; the server binds its listener
    // near-instantly (observed well under 100ms), yt-dlp itself also retries a
    // POT request that arrives before the server is ready. This provider is
    // explicitly non-blocking for downloads (see common_args in spec.rs), so a
    // still-cold server just means the first request skips PO tokens rather
    // than failing the job.
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;

    Some(Running {
        _child: child,
        info: PotInfo {
            base_url: format!("http://127.0.0.1:{port}"),
            plugin_dir,
            cli_path,
        },
    })
}

/// Returns the running provider's info, starting it on first use. Returns
/// `None` if the binary/plugin/port aren't available — callers must treat
/// that as "proceed without a PO-token provider", never as a hard error.
pub async fn ensure_started(app: &AppHandle) -> Option<PotInfo> {
    let state = app.state::<PotServer>();
    let mut guard = state.0.lock().await;
    if guard.is_none() {
        *guard = start(app).await;
    }
    guard.as_ref().map(|r| r.info.clone())
}
