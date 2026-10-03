//! Subscriptions inbox: a list of channels the user follows and the recent
//! uploads fetched from them, so new videos can be picked without opening the
//! YouTube feed.
//!
//! Uploads come from yt-dlp's flat listing of each channel's `/videos` tab
//! rather than the channel RSS feeds: since autumn 2026 `feeds/videos.xml`
//! answers 404 for every channel, while the tab listing keeps working, carries
//! durations, and leaves out Shorts and streams on its own.

use crate::bin::{resolve, Tool};
use crate::probe::friendly_error;
use crate::spec::{network_args, runtime_args, NetworkOpts, RuntimeEnv};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::process::Command;
use tokio::sync::{Mutex as AsyncMutex, Semaphore};

/// How many recent uploads to list per channel on each refresh.
const ITEMS_PER_CHANNEL: u32 = 15;
/// Parallel yt-dlp processes during a refresh — each one is a separate
/// Python start-up plus a page fetch, so more than this mostly invites
/// YouTube's rate limiting.
const REFRESH_PARALLELISM: usize = 4;
/// On a channel's first fetch, only uploads newer than this land in the inbox;
/// older ones are marked seen so adding a channel doesn't flood it.
const FIRST_FETCH_WINDOW_SECS: i64 = 7 * 24 * 3600;
/// Handled items that dropped out of a channel's listing are forgotten after
/// this — they can't come back, since the listing only ever shows newer ones.
const PRUNE_AFTER_SECS: i64 = 90 * 24 * 3600;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AutoDownload {
    Video,
    Audio,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub enabled: bool,
    /// Enqueue every new upload of this channel automatically, in this mode.
    pub auto_download: Option<AutoDownload>,
    pub added_at: i64,
    pub last_checked: Option<i64>,
    pub last_error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ItemState {
    New,
    Seen,
    Queued,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FeedItem {
    pub video_id: String,
    pub channel_id: String,
    pub channel_title: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration: Option<f64>,
    /// Unix seconds. Day precision: yt-dlp derives it from "3 days ago".
    pub published: Option<i64>,
    pub url: String,
    pub state: ItemState,
    pub first_seen: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubsState {
    pub channels: Vec<Channel>,
    pub items: Vec<FeedItem>,
    pub last_refresh: Option<i64>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub new_items: Vec<FeedItem>,
    pub failed_channels: usize,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RefreshProgress {
    pub done: usize,
    pub total: usize,
}

#[derive(Default)]
pub struct SubsManager {
    state: AsyncMutex<Option<SubsState>>,
    refreshing: AtomicBool,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn subs_file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("subscriptions.json"))
}

async fn load_from_disk(app: &AppHandle) -> SubsState {
    let Some(path) = subs_file(app) else { return SubsState::default() };
    let Ok(bytes) = tokio::fs::read(&path).await else { return SubsState::default() };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

async fn persist(app: &AppHandle, state: &SubsState) {
    let Some(path) = subs_file(app) else { return };
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    if let Ok(json) = serde_json::to_vec_pretty(state) {
        let _ = tokio::fs::write(&path, json).await;
    }
}

/// Runs `f` against the loaded state, then saves it and tells the UI.
async fn mutate<T>(app: &AppHandle, f: impl FnOnce(&mut SubsState) -> T) -> T {
    let manager = app.state::<SubsManager>();
    let mut guard = manager.state.lock().await;
    if guard.is_none() {
        *guard = Some(load_from_disk(app).await);
    }
    let state = guard.as_mut().expect("loaded above");
    let result = f(state);
    persist(app, state).await;
    let _ = app.emit("subs://state", state.clone());
    result
}

async fn snapshot(app: &AppHandle) -> SubsState {
    let manager = app.state::<SubsManager>();
    let mut guard = manager.state.lock().await;
    if guard.is_none() {
        *guard = Some(load_from_disk(app).await);
    }
    guard.clone().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// yt-dlp

#[derive(Deserialize, Debug, Default)]
struct RawThumb {
    url: Option<String>,
    id: Option<String>,
    width: Option<u32>,
}

#[derive(Deserialize, Debug, Default)]
struct RawEntry {
    id: Option<String>,
    title: Option<String>,
    channel: Option<String>,
    channel_id: Option<String>,
    uploader: Option<String>,
    url: Option<String>,
    duration: Option<f64>,
    timestamp: Option<i64>,
    live_status: Option<String>,
    thumbnails: Option<Vec<RawThumb>>,
}

#[derive(Deserialize, Debug, Default)]
struct RawTab {
    id: Option<String>,
    channel: Option<String>,
    channel_id: Option<String>,
    uploader: Option<String>,
    thumbnails: Option<Vec<RawThumb>>,
    entries: Option<Vec<RawEntry>>,
}

struct Ytdlp {
    bin: PathBuf,
    env: RuntimeEnv,
    net: NetworkOpts,
}

impl Ytdlp {
    async fn new(app: &AppHandle) -> Result<Self, String> {
        Ok(Self {
            bin: resolve(app, Tool::YtDlp).await?,
            env: crate::runtime::resolve_runtime_env(app).await,
            net: crate::probe::settings_network_opts(app).await,
        })
    }

    async fn flat_json(&self, url: &str, items: Option<&str>, with_cookies: bool) -> Result<RawTab, String> {
        let mut args: Vec<String> = vec![
            "--dump-single-json".into(),
            "--flat-playlist".into(),
            "--no-check-certificate".into(),
            "--no-update".into(),
            // Flat listings only say "3 days ago"; this turns it into a timestamp.
            "--extractor-args".into(),
            "youtubetab:approximate_date".into(),
        ];
        if let Some(range) = items {
            args.push("--playlist-items".into());
            args.push(range.into());
        }
        args.extend(runtime_args(&self.env));
        let net = if with_cookies { self.net.clone() } else { self.net.without_cookies() };
        args.extend(network_args(&net));
        args.push("--".into());
        args.push(url.into());

        let output = Command::new(&self.bin)
            .args(&args)
            .output()
            .await
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(friendly_error(&String::from_utf8_lossy(&output.stderr)));
        }
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Некорректный ответ yt-dlp: {e}"))
    }
}

fn avatar(thumbs: &Option<Vec<RawThumb>>) -> Option<String> {
    let thumbs = thumbs.as_ref()?;
    // A channel tab tags its avatar ("avatar_uncropped"); the subscriptions
    // list gives untagged, protocol-relative avatar URLs only.
    let url = thumbs
        .iter()
        .find(|t| t.id.as_deref().is_some_and(|id| id.starts_with("avatar")))
        .or_else(|| thumbs.iter().find(|t| t.id.is_none()))
        .and_then(|t| t.url.as_deref())?;
    Some(small_avatar(url))
}

/// yt3 avatar URLs end in a size spec (`=s0` is the full original, often a
/// 400 KB+ PNG); ask for a small square instead.
fn small_avatar(url: &str) -> String {
    let url = if url.starts_with("//") { format!("https:{url}") } else { url.to_string() };
    match url.rfind('=') {
        Some(i) if url.contains("googleusercontent.com") && url[i + 1..].starts_with('s') => {
            format!("{}=s88-c-k-c0x00ffffff-no-rj", &url[..i])
        }
        _ => url,
    }
}

fn video_thumbnail(id: &str, thumbs: &Option<Vec<RawThumb>>) -> Option<String> {
    // The listing's own thumbnails are signed hq720 URLs; the smallest one
    // that's still crisp at card size beats a 1280px download per row.
    thumbs
        .as_ref()
        .and_then(|ts| {
            ts.iter()
                .filter(|t| t.width.unwrap_or(0) >= 320)
                .min_by_key(|t| t.width.unwrap_or(u32::MAX))
                .and_then(|t| t.url.clone())
        })
        .or_else(|| Some(format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg")))
}

fn channel_id_from_url(url: &str) -> Option<String> {
    let idx = url.find("/channel/")?;
    let rest = &url[idx + "/channel/".len()..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
    id.starts_with("UC").then_some(id)
}

fn videos_tab_url(channel_id: &str) -> String {
    format!("https://www.youtube.com/channel/{channel_id}/videos")
}

struct FetchedChannel {
    title: Option<String>,
    thumbnail: Option<String>,
    entries: Vec<RawEntry>,
}

async fn fetch_channel(ytdlp: &Ytdlp, channel_id: &str) -> Result<FetchedChannel, String> {
    let range = format!("1:{ITEMS_PER_CHANNEL}");
    // Public listings don't need the account; leaving cookies out also steers
    // clear of the cookie-client rejection (see `probe::is_cookie_client_rejection`).
    let tab = match ytdlp.flat_json(&videos_tab_url(channel_id), Some(&range), false).await {
        Ok(tab) => tab,
        Err(_) if ytdlp.net.uses_cookies() => {
            ytdlp.flat_json(&videos_tab_url(channel_id), Some(&range), true).await?
        }
        Err(e) => return Err(e),
    };
    Ok(FetchedChannel {
        title: tab.channel.or(tab.uploader),
        thumbnail: avatar(&tab.thumbnails),
        entries: tab.entries.unwrap_or_default(),
    })
}

/// Folds a fresh listing into the stored items. Returns the newly added ones.
fn merge_channel(state: &mut SubsState, channel_id: &str, fetched: FetchedChannel, at: i64) -> Vec<FeedItem> {
    let Some(channel) = state.channels.iter_mut().find(|c| c.id == channel_id) else {
        return Vec::new();
    };
    let first_fetch = channel.last_checked.is_none();
    if let Some(title) = fetched.title.filter(|t| !t.is_empty()) {
        channel.title = title;
    }
    if fetched.thumbnail.is_some() {
        channel.thumbnail = fetched.thumbnail;
    }
    channel.last_checked = Some(at);
    channel.last_error = None;
    let channel_title = channel.title.clone();

    let mut listed = HashSet::new();
    let mut added = Vec::new();
    for entry in fetched.entries {
        let Some(id) = entry.id.clone() else { continue };
        if matches!(entry.live_status.as_deref(), Some("is_upcoming") | Some("is_live")) {
            continue;
        }
        listed.insert(id.clone());
        let thumbnail = video_thumbnail(&id, &entry.thumbnails);
        if let Some(existing) = state.items.iter_mut().find(|i| i.video_id == id) {
            if let Some(title) = entry.title {
                existing.title = title;
            }
            existing.duration = entry.duration.or(existing.duration);
            existing.thumbnail = thumbnail.or(existing.thumbnail.take());
            continue;
        }
        let too_old = entry.timestamp.map_or(true, |ts| at - ts > FIRST_FETCH_WINDOW_SECS);
        let item = FeedItem {
            url: entry.url.unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}")),
            video_id: id,
            channel_id: channel_id.to_string(),
            channel_title: channel_title.clone(),
            title: entry.title.unwrap_or_else(|| "Без названия".to_string()),
            thumbnail,
            duration: entry.duration,
            published: entry.timestamp,
            state: if first_fetch && too_old { ItemState::Seen } else { ItemState::New },
            first_seen: at,
        };
        if item.state == ItemState::New {
            added.push(item.clone());
        }
        state.items.push(item);
    }

    state.items.retain(|i| {
        i.channel_id != channel_id
            || listed.contains(&i.video_id)
            || i.state == ItemState::New
            || at - i.published.unwrap_or(i.first_seen) < PRUNE_AFTER_SECS
    });
    added
}

fn new_channel(id: String, title: String, thumbnail: Option<String>) -> Channel {
    Channel {
        id,
        title,
        thumbnail,
        enabled: true,
        auto_download: None,
        added_at: now(),
        last_checked: None,
        last_error: None,
    }
}

// ---------------------------------------------------------------------------
// Commands

#[tauri::command]
pub async fn subs_get(app: AppHandle) -> Result<SubsState, String> {
    Ok(snapshot(&app).await)
}

#[tauri::command]
pub async fn subs_add_channel(app: AppHandle, url: String) -> Result<Channel, String> {
    let url = crate::probe::validate_url(&url)?;
    let ytdlp = Ytdlp::new(&app).await?;

    let channel_id = match channel_id_from_url(&url) {
        Some(id) => id,
        None => {
            // @handles, /c/ and /user/ links and even a video link all resolve
            // to the owning channel's id.
            let info = ytdlp.flat_json(&url, Some("1"), false).await?;
            info.channel_id
                .or_else(|| info.id.filter(|id| id.starts_with("UC")))
                .or_else(|| info.entries.and_then(|es| es.into_iter().find_map(|e| e.channel_id)))
                .ok_or("Не удалось определить канал по ссылке")?
        }
    };

    if let Some(existing) = snapshot(&app).await.channels.into_iter().find(|c| c.id == channel_id) {
        return Ok(existing);
    }

    let fetched = fetch_channel(&ytdlp, &channel_id).await?;
    let title = fetched.title.clone().unwrap_or_else(|| channel_id.clone());
    let channel = new_channel(channel_id.clone(), title, fetched.thumbnail.clone());
    let at = now();
    mutate(&app, |state| {
        state.channels.push(channel.clone());
        merge_channel(state, &channel_id, fetched, at);
    })
    .await;
    Ok(snapshot(&app).await.channels.into_iter().find(|c| c.id == channel_id).unwrap_or(channel))
}

/// Pulls the channel list of the signed-in account (needs cookies). Returns
/// how many channels were added; their uploads arrive on the next refresh.
#[tauri::command]
pub async fn subs_import(app: AppHandle) -> Result<usize, String> {
    let ytdlp = Ytdlp::new(&app).await?;
    if !ytdlp.net.uses_cookies() {
        return Err("Для импорта подписок нужны cookies аккаунта YouTube — укажите их в настройках".into());
    }

    let mut found: Vec<(String, String, Option<String>)> = Vec::new();
    let channels_page = ytdlp.flat_json("https://www.youtube.com/feed/channels", None, true).await;
    if let Ok(tab) = &channels_page {
        for e in tab.entries.iter().flatten() {
            let id = e
                .channel_id
                .clone()
                .or_else(|| e.id.clone().filter(|id| id.starts_with("UC")))
                .or_else(|| e.url.as_deref().and_then(channel_id_from_url));
            if let Some(id) = id {
                let title = e.title.clone().or(e.channel.clone()).unwrap_or_else(|| id.clone());
                found.push((id, title, avatar(&e.thumbnails)));
            }
        }
    }
    if found.is_empty() {
        // Fall back to the subscriptions feed: it lists videos, not channels,
        // so it only finds channels that uploaded recently — better than nothing.
        let feed = ytdlp
            .flat_json("https://www.youtube.com/feed/subscriptions", Some("1:300"), true)
            .await
            .map_err(|e| channels_page.err().unwrap_or(e))?;
        for e in feed.entries.into_iter().flatten() {
            if let Some(id) = e.channel_id {
                let title = e.channel.or(e.uploader).unwrap_or_else(|| id.clone());
                found.push((id, title, None));
            }
        }
    }
    if found.is_empty() {
        return Err("YouTube не вернул ни одной подписки — проверьте, что cookies свежие".into());
    }

    Ok(mutate(&app, |state| {
        let mut known: HashSet<String> = state.channels.iter().map(|c| c.id.clone()).collect();
        let mut added = 0;
        for (id, title, thumbnail) in found {
            if known.insert(id.clone()) {
                state.channels.push(new_channel(id, title, thumbnail));
                added += 1;
            }
        }
        added
    })
    .await)
}

#[tauri::command]
pub async fn subs_remove_channel(app: AppHandle, id: String) -> Result<(), String> {
    mutate(&app, |state| {
        state.channels.retain(|c| c.id != id);
        state.items.retain(|i| i.channel_id != id);
    })
    .await;
    Ok(())
}

#[tauri::command]
pub async fn subs_update_channel(
    app: AppHandle,
    id: String,
    enabled: bool,
    auto_download: Option<AutoDownload>,
) -> Result<(), String> {
    mutate(&app, |state| {
        if let Some(c) = state.channels.iter_mut().find(|c| c.id == id) {
            c.enabled = enabled;
            c.auto_download = auto_download;
        }
    })
    .await;
    Ok(())
}

#[tauri::command]
pub async fn subs_set_items_state(app: AppHandle, ids: Vec<String>, state: ItemState) -> Result<(), String> {
    let ids: HashSet<String> = ids.into_iter().collect();
    mutate(&app, |s| {
        for item in s.items.iter_mut().filter(|i| ids.contains(&i.video_id)) {
            item.state = state;
        }
    })
    .await;
    Ok(())
}

#[tauri::command]
pub async fn subs_refresh(app: AppHandle) -> Result<RefreshResult, String> {
    let manager = app.state::<SubsManager>();
    if manager.refreshing.swap(true, Ordering::SeqCst) {
        return Err("Обновление уже идёт".into());
    }
    let result = refresh_inner(&app).await;
    manager.refreshing.store(false, Ordering::SeqCst);
    result
}

async fn refresh_inner(app: &AppHandle) -> Result<RefreshResult, String> {
    let ytdlp = Arc::new(Ytdlp::new(app).await?);
    let channel_ids: Vec<String> = snapshot(app)
        .await
        .channels
        .into_iter()
        .filter(|c| c.enabled)
        .map(|c| c.id)
        .collect();
    let total = channel_ids.len();
    let _ = app.emit("subs://progress", RefreshProgress { done: 0, total });

    let semaphore = Arc::new(Semaphore::new(REFRESH_PARALLELISM));
    let mut tasks = tokio::task::JoinSet::new();
    for id in channel_ids {
        let ytdlp = ytdlp.clone();
        let semaphore = semaphore.clone();
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await;
            let result = fetch_channel(&ytdlp, &id).await;
            (id, result)
        });
    }

    let mut new_items = Vec::new();
    let mut failed_channels = 0;
    let mut done = 0;
    while let Some(joined) = tasks.join_next().await {
        let Ok((id, result)) = joined else { continue };
        done += 1;
        let at = now();
        match result {
            Ok(fetched) => {
                let added = mutate(app, |state| merge_channel(state, &id, fetched, at)).await;
                new_items.extend(added);
            }
            Err(e) => {
                failed_channels += 1;
                mutate(app, |state| {
                    if let Some(c) = state.channels.iter_mut().find(|c| c.id == id) {
                        c.last_error = Some(e);
                    }
                })
                .await;
            }
        }
        let _ = app.emit("subs://progress", RefreshProgress { done, total });
    }

    mutate(app, |state| state.last_refresh = Some(now())).await;
    Ok(RefreshResult { new_items, failed_channels })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, ts: i64) -> RawEntry {
        RawEntry {
            id: Some(id.into()),
            title: Some(format!("video {id}")),
            timestamp: Some(ts),
            ..Default::default()
        }
    }

    fn state_with_channel() -> SubsState {
        SubsState {
            channels: vec![new_channel("UC1".into(), "Chan".into(), None)],
            ..Default::default()
        }
    }

    fn fetched(entries: Vec<RawEntry>) -> FetchedChannel {
        FetchedChannel { title: None, thumbnail: None, entries }
    }

    #[test]
    fn first_fetch_only_marks_recent_uploads_as_new() {
        let at = 1_000_000_000;
        let mut state = state_with_channel();
        let added = merge_channel(
            &mut state,
            "UC1",
            fetched(vec![entry("fresh", at - 3600), entry("old", at - 30 * 24 * 3600)]),
            at,
        );
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].video_id, "fresh");
        assert_eq!(state.items.len(), 2);
    }

    #[test]
    fn later_fetches_treat_any_unknown_upload_as_new() {
        let at = 1_000_000_000;
        let mut state = state_with_channel();
        merge_channel(&mut state, "UC1", fetched(vec![entry("a", at)]), at);
        let added = merge_channel(
            &mut state,
            "UC1",
            fetched(vec![entry("b", at - 30 * 24 * 3600), entry("a", at)]),
            at + 60,
        );
        assert_eq!(added.iter().map(|i| i.video_id.as_str()).collect::<Vec<_>>(), vec!["b"]);
    }

    #[test]
    fn known_items_keep_their_state() {
        let at = 1_000_000_000;
        let mut state = state_with_channel();
        merge_channel(&mut state, "UC1", fetched(vec![entry("a", at)]), at);
        state.items[0].state = ItemState::Seen;
        let added = merge_channel(&mut state, "UC1", fetched(vec![entry("a", at)]), at + 60);
        assert!(added.is_empty());
        assert_eq!(state.items[0].state, ItemState::Seen);
    }

    #[test]
    fn handled_items_that_left_the_listing_are_pruned_once_old() {
        let at = 1_000_000_000;
        let mut state = state_with_channel();
        merge_channel(&mut state, "UC1", fetched(vec![entry("a", at)]), at);
        state.items[0].state = ItemState::Seen;
        let later = at + PRUNE_AFTER_SECS + 1;
        merge_channel(&mut state, "UC1", fetched(vec![entry("b", later)]), later);
        assert!(state.items.iter().all(|i| i.video_id != "a"));
    }

    #[test]
    fn upcoming_streams_are_skipped() {
        let at = 1_000_000_000;
        let mut state = state_with_channel();
        let mut upcoming = entry("soon", at);
        upcoming.live_status = Some("is_upcoming".into());
        merge_channel(&mut state, "UC1", fetched(vec![upcoming]), at);
        assert!(state.items.is_empty());
    }

    #[test]
    fn avatars_are_shrunk_and_made_absolute() {
        assert_eq!(
            small_avatar("//yt3.googleusercontent.com/abc=s176-c-k-c0x00ffffff-no-rj-mo"),
            "https://yt3.googleusercontent.com/abc=s88-c-k-c0x00ffffff-no-rj"
        );
        assert_eq!(
            small_avatar("https://yt3.googleusercontent.com/ytc/xyz=s0"),
            "https://yt3.googleusercontent.com/ytc/xyz=s88-c-k-c0x00ffffff-no-rj"
        );
    }

    #[test]
    fn channel_id_is_read_from_channel_urls() {
        assert_eq!(
            channel_id_from_url("https://www.youtube.com/channel/UCsBjURrPoezykLs9EqgamOA/videos").as_deref(),
            Some("UCsBjURrPoezykLs9EqgamOA")
        );
        assert_eq!(channel_id_from_url("https://www.youtube.com/@fireship"), None);
    }
}

