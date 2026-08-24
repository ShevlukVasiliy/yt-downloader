use crate::bin::{resolve, status, Tool, ToolStatus};
use tauri::AppHandle;

/// Resolves the value for yt-dlp's `--js-runtimes` flag: `quickjs:<path-to-qjs>`.
///
/// We always use the bundled quickjs-ng rather than probing for a system
/// deno/node install. quickjs-ng is tiny (~1-2MB per platform), always present
/// in the bundle, and already verified end-to-end against the exact yt-dlp
/// version we ship — a system runtime of unknown version risks silently
/// reintroducing the "no JS runtime" bug this exists to fix (see
/// https://github.com/yt-dlp/yt-dlp/wiki/EJS for the version floors yt-dlp
/// enforces per runtime).
pub async fn js_runtime_arg(app: &AppHandle) -> Option<String> {
    let path = resolve(app, Tool::Qjs).await.ok()?;
    Some(format!("quickjs:{}", path.to_string_lossy()))
}

pub async fn js_runtime_status(app: &AppHandle) -> ToolStatus {
    status(app, Tool::Qjs).await
}

/// Resolves everything `spec::to_argv`/`probe::analyze_url` need to route
/// around YouTube's current defenses: the JS runtime and (best-effort) the
/// PO-token provider. Never fails — an unavailable POT provider just means
/// those fields stay `None`, and `runtime_args` skips the corresponding flags.
pub async fn resolve_runtime_env(app: &AppHandle) -> crate::spec::RuntimeEnv {
    let js_runtime_arg = js_runtime_arg(app).await;
    let pot = crate::pot::ensure_started(app).await;
    crate::spec::RuntimeEnv {
        js_runtime_arg,
        pot_plugin_dir: pot.as_ref().map(|p| p.plugin_dir.to_string_lossy().to_string()),
        pot_base_url: pot.as_ref().map(|p| p.base_url.clone()),
        pot_cli_path: pot.as_ref().map(|p| p.cli_path.to_string_lossy().to_string()),
    }
}
