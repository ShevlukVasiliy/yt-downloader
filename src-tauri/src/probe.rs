use crate::bin::{resolve, Tool};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tokio::process::Command;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub webpage_url: String,
    pub available_heights: Vec<u32>,
    pub has_audio_only: bool,
    pub is_live: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistEntry {
    pub id: String,
    pub title: String,
    pub duration: Option<f64>,
    pub thumbnail: Option<String>,
    pub index: u32,
    pub uploader: Option<String>,
    pub webpage_url: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub thumbnail: Option<String>,
    pub webpage_url: String,
    pub entries: Vec<PlaylistEntry>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Analysis {
    Video(VideoInfo),
    Playlist(PlaylistInfo),
}

#[derive(Deserialize, Debug, Default)]
struct RawThumbnail {
    url: Option<String>,
    #[serde(default)]
    preference: Option<i64>,
}

#[derive(Deserialize, Debug, Default)]
struct RawFormat {
    height: Option<u32>,
    vcodec: Option<String>,
    acodec: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct RawEntry {
    id: Option<String>,
    title: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
    thumbnails: Option<Vec<RawThumbnail>>,
    uploader: Option<String>,
    channel: Option<String>,
    url: Option<String>,
    webpage_url: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct RawInfo {
    id: Option<String>,
    title: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
    thumbnails: Option<Vec<RawThumbnail>>,
    webpage_url: Option<String>,
    original_url: Option<String>,
    #[serde(rename = "_type")]
    r#type: Option<String>,
    entries: Option<Vec<RawEntry>>,
    formats: Option<Vec<RawFormat>>,
    is_live: Option<bool>,
    was_live: Option<bool>,
}

fn best_thumbnail(direct: &Option<String>, list: &Option<Vec<RawThumbnail>>) -> Option<String> {
    if let Some(url) = direct {
        return Some(url.clone());
    }
    list.as_ref().and_then(|thumbs| {
        thumbs
            .iter()
            .max_by_key(|t| t.preference.unwrap_or(i64::MIN))
            .and_then(|t| t.url.clone())
            .or_else(|| thumbs.last().and_then(|t| t.url.clone()))
    })
}

/// Search engines (Google, Bing, etc.) often hand out click-tracking redirect
/// links instead of the real destination when a user copies a link from a
/// results page. Unwrap the real URL out of the `url=`/`q=` query param so
/// pasting one of these still works.
fn unwrap_redirect(parsed: &url::Url) -> Option<url::Url> {
    let host = parsed.host_str()?;
    let is_known_redirector = host.ends_with("google.com")
        || host.ends_with("bing.com")
        || host.ends_with("duckduckgo.com");
    if !is_known_redirector {
        return None;
    }
    parsed
        .query_pairs()
        .find(|(key, _)| key == "url" || key == "q" || key == "uddg")
        .and_then(|(_, target)| url::Url::parse(&target).ok())
}

/// Some copy sources (share sheets, chat apps re-linkifying text) double-encode
/// the URL, so the query separators show up as literal `%3F`/`%3D`/`%26` in what
/// should be the query string (e.g. `.../watch%3Fv%3DID` instead of
/// `.../watch?v=ID`). Decoding once turns those back into real separators; a
/// normally-formed URL has no such sequences and passes through unchanged.
fn decode_if_double_encoded(input: &str) -> String {
    if input.contains("%3F") || input.contains("%3D") || input.contains("%26") {
        urlencoding::decode(input)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| input.to_string())
    } else {
        input.to_string()
    }
}

fn is_youtube_host(host: &str) -> bool {
    host == "youtube.com"
        || host.ends_with(".youtube.com")
        || host == "youtu.be"
        || host.ends_with(".youtu.be")
        || host == "music.youtube.com"
}

pub fn validate_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Вставьте ссылку".to_string());
    }
    let looks_like_url = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    if !looks_like_url {
        return Err("Это не похоже на ссылку".to_string());
    }

    let normalized = decode_if_double_encoded(trimmed);
    let parsed = url::Url::parse(&normalized).map_err(|_| "Это не похоже на ссылку".to_string())?;
    let mut effective = unwrap_redirect(&parsed).unwrap_or(parsed);
    if let Ok(reparsed) = url::Url::parse(&decode_if_double_encoded(effective.as_str())) {
        effective = reparsed;
    }

    let is_youtube = effective.host_str().map(is_youtube_host).unwrap_or(false);
    if !is_youtube {
        return Err("Поддерживаются только ссылки YouTube".to_string());
    }
    Ok(effective.to_string())
}

pub fn friendly_error(stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("private video") {
        "Это приватное видео — нужен вход в аккаунт".to_string()
    } else if lower.contains("video unavailable") {
        "Видео недоступно".to_string()
    } else if lower.contains("this live event") {
        "Трансляция ещё не началась".to_string()
    } else if lower.contains("sign in") || lower.contains("age") {
        "Требуется подтверждение возраста или вход в аккаунт".to_string()
    } else if lower.contains("unable to download webpage") || lower.contains("network") {
        "Не удалось подключиться к YouTube — проверьте интернет".to_string()
    } else {
        stderr
            .lines()
            .find(|l| l.contains("ERROR"))
            .unwrap_or("Не удалось получить информацию о ссылке")
            .to_string()
    }
}

#[tauri::command]
pub async fn analyze_url(app: AppHandle, url: String) -> Result<Analysis, String> {
    let url = validate_url(&url)?;
    let bin = resolve(&app, Tool::YtDlp).await?;

    let output = Command::new(&bin)
        .args([
            "--dump-single-json",
            "--flat-playlist",
            "--no-warnings",
            "--no-check-certificate",
            "--ignore-no-formats-error",
            "--",
        ])
        .arg(&url)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(friendly_error(&String::from_utf8_lossy(&output.stderr)));
    }

    let raw: RawInfo =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("Некорректный ответ yt-dlp: {e}"))?;

    let is_playlist = raw.r#type.as_deref() == Some("playlist") || raw.entries.is_some();

    if is_playlist {
        let entries = raw
            .entries
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let id = e.id.clone()?;
                Some(PlaylistEntry {
                    webpage_url: e
                        .webpage_url
                        .or(e.url.clone())
                        .unwrap_or_else(|| format!("https://www.youtube.com/watch?v={id}")),
                    title: e.title.unwrap_or_else(|| "Без названия".to_string()),
                    duration: e.duration,
                    thumbnail: best_thumbnail(&e.thumbnail, &e.thumbnails),
                    uploader: e.uploader.or(e.channel),
                    index: i as u32 + 1,
                    id,
                })
            })
            .collect();

        Ok(Analysis::Playlist(PlaylistInfo {
            id: raw.id.unwrap_or_default(),
            title: raw.title.unwrap_or_else(|| "Плейлист".to_string()),
            uploader: raw.uploader.or(raw.channel),
            thumbnail: best_thumbnail(&raw.thumbnail, &raw.thumbnails),
            webpage_url: raw.webpage_url.or(raw.original_url).unwrap_or(url),
            entries,
        }))
    } else {
        let mut heights: Vec<u32> = raw
            .formats
            .as_ref()
            .map(|fs| fs.iter().filter_map(|f| f.height).collect())
            .unwrap_or_default();
        heights.sort_unstable_by(|a, b| b.cmp(a));
        heights.dedup();

        let has_audio_only = raw
            .formats
            .as_ref()
            .map(|fs| {
                fs.iter().any(|f| {
                    f.vcodec.as_deref() == Some("none")
                        && f.acodec.as_deref().unwrap_or("none") != "none"
                })
            })
            .unwrap_or(false);

        Ok(Analysis::Video(VideoInfo {
            id: raw.id.unwrap_or_default(),
            title: raw.title.unwrap_or_else(|| "Без названия".to_string()),
            uploader: raw.uploader.or(raw.channel),
            duration: raw.duration,
            thumbnail: best_thumbnail(&raw.thumbnail, &raw.thumbnails),
            webpage_url: raw.webpage_url.or(raw.original_url).unwrap_or(url),
            available_heights: heights,
            has_audio_only,
            is_live: raw.is_live.unwrap_or(false) || raw.was_live.unwrap_or(false),
        }))
    }
}

#[cfg(test)]
mod validate_url_tests {
    use super::*;

    #[test]
    fn plain_youtube_link_passes_through_unchanged() {
        let url = "https://www.youtube.com/watch?v=bFalsqzMyRs";
        assert_eq!(validate_url(url).unwrap(), url);
    }

    #[test]
    fn google_search_redirect_unwraps_to_real_youtube_url() {
        let wrapped = "https://www.google.com/url?sa=t&source=web&rct=j&opi=89978449&url=https://www.youtube.com/watch%3Fv%3DbFalsqzMyRs&ved=2ahUKEwjXseHH-aGWAxUU9AIHHTXJJowQwqsBegQIHhAB&usg=AOvVaw1yhIpk7mIP-ayba_YHiTaB";
        let result = validate_url(wrapped).unwrap();
        assert_eq!(result, "https://www.youtube.com/watch?v=bFalsqzMyRs");
    }

    #[test]
    fn non_youtube_link_is_rejected_even_when_it_contains_the_word() {
        let sneaky = "https://youtube.com.evil-site.example/watch?v=x";
        assert!(validate_url(sneaky).is_err());
    }

    #[test]
    fn non_redirector_search_link_without_youtube_target_is_rejected() {
        let url = "https://www.google.com/search?q=dying+light";
        assert!(validate_url(url).is_err());
    }

    #[test]
    fn empty_input_reports_friendly_error() {
        assert!(validate_url("   ").is_err());
    }

    #[test]
    fn double_encoded_query_separators_are_decoded() {
        let mangled = "https://www.youtube.com/watch%3Fv%3DbFalsqzMyRs";
        let result = validate_url(mangled).unwrap();
        assert_eq!(result, "https://www.youtube.com/watch?v=bFalsqzMyRs");
    }

    #[test]
    fn double_encoded_playlist_link_keeps_list_param() {
        let mangled = "https://www.youtube.com/playlist%3Flist%3DPLejGw9J2xE9VX0RFX2loRlOzg7fjatGL3";
        let result = validate_url(mangled).unwrap();
        assert_eq!(
            result,
            "https://www.youtube.com/playlist?list=PLejGw9J2xE9VX0RFX2loRlOzg7fjatGL3"
        );
    }
}
