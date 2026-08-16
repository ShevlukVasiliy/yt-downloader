use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Video,
    Audio,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum VideoContainer {
    Mp4,
    Mkv,
    Webm,
}

impl VideoContainer {
    fn as_str(&self) -> &'static str {
        match self {
            VideoContainer::Mp4 => "mp4",
            VideoContainer::Mkv => "mkv",
            VideoContainer::Webm => "webm",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    Mp3,
    M4a,
    Opus,
    Flac,
    Wav,
}

impl AudioFormat {
    fn as_str(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::M4a => "m4a",
            AudioFormat::Opus => "opus",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum AudioQuality {
    Best,
    Kbps(u32),
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleOptions {
    pub enabled: bool,
    pub langs: Vec<String>,
    pub auto: bool,
    pub embed: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Extras {
    pub embed_thumbnail: bool,
    pub embed_metadata: bool,
    pub embed_chapters: bool,
    pub sponsorblock: bool,
    pub write_info_json: bool,
    pub subtitles: SubtitleOptions,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSpec {
    pub mode: Mode,
    pub max_height: Option<u32>,
    pub container: VideoContainer,
    pub video_only: bool,
    pub audio_format: AudioFormat,
    pub audio_quality: AudioQuality,
    pub extras: Extras,
    pub output_dir: String,
    pub filename_template: String,
    pub rate_limit_kbps: Option<u32>,
    pub proxy: Option<String>,
    pub cookies_from_browser: Option<String>,
    pub download_archive_path: Option<String>,
    pub ffmpeg_dir: Option<String>,
    pub network_retries: u32,
}

/// Pure translation of a DownloadSpec into yt-dlp argv. Deliberately side-effect
/// free and independent of any resolved binary paths so it stays unit-testable.
pub fn to_argv(spec: &DownloadSpec, url: &str) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "--newline".into(),
        "--no-warnings".into(),
        "--no-playlist".into(),
        "--continue".into(),
        "--retries".into(),
        spec.network_retries.to_string(),
        "--fragment-retries".into(),
        spec.network_retries.to_string(),
        "--progress-template".into(),
        "download:[DLPROG]%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.filename)s".into(),
        "--progress-template".into(),
        "postprocess:[PPPROG]%(progress.status)s|%(postprocessor)s|%(info.filename)s".into(),
    ];

    match spec.mode {
        Mode::Video => {
            let height = spec
                .max_height
                .map(|h| format!("[height<={h}]"))
                .unwrap_or_default();

            // YouTube's best video-only streams are frequently VP9/AV1, which Apple's
            // QuickTime/AVFoundation-based players can't decode: merging them into an
            // .mp4 container "succeeds" but plays back as audio-only with a player
            // error. Prefer H.264 (avc1) + AAC for mp4 so the default just plays,
            // falling back to whatever's available. Other containers (mkv/webm) don't
            // have this constraint, so leave codec selection unrestricted there.
            let prefer_compat = matches!(spec.container, VideoContainer::Mp4);

            let format_selector = if spec.video_only {
                if prefer_compat {
                    format!("bestvideo{height}[vcodec^=avc1]/bestvideo{height}/best{height}")
                } else {
                    format!("bestvideo{height}/best{height}")
                }
            } else if prefer_compat {
                format!(
                    "bestvideo{height}[vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo{height}+bestaudio/best{height}"
                )
            } else {
                format!("bestvideo{height}+bestaudio/best{height}")
            };

            args.push("-f".into());
            args.push(format_selector);
            args.push("--merge-output-format".into());
            args.push(spec.container.as_str().into());
        }
        Mode::Audio => {
            args.push("-x".into());
            args.push("--audio-format".into());
            args.push(spec.audio_format.as_str().into());
            args.push("--audio-quality".into());
            args.push(match &spec.audio_quality {
                AudioQuality::Best => "0".into(),
                AudioQuality::Kbps(k) => format!("{k}K"),
            });
        }
    }

    if spec.extras.embed_thumbnail {
        args.push("--embed-thumbnail".into());
    }
    if spec.extras.embed_metadata {
        args.push("--embed-metadata".into());
    }
    if spec.extras.embed_chapters {
        args.push("--embed-chapters".into());
    }
    if spec.extras.write_info_json {
        args.push("--write-info-json".into());
    }
    if spec.extras.sponsorblock {
        args.push("--sponsorblock-remove".into());
        args.push("sponsor,selfpromo,interaction".into());
    }
    if spec.extras.subtitles.enabled && !spec.extras.subtitles.langs.is_empty() {
        args.push("--write-subs".into());
        if spec.extras.subtitles.auto {
            args.push("--write-auto-subs".into());
        }
        args.push("--sub-langs".into());
        args.push(spec.extras.subtitles.langs.join(","));
        if spec.extras.subtitles.embed {
            args.push("--embed-subs".into());
        }
    }

    args.push("--paths".into());
    args.push(spec.output_dir.clone());
    args.push("-o".into());
    args.push(spec.filename_template.clone());

    if let Some(rate) = spec.rate_limit_kbps {
        args.push("--limit-rate".into());
        args.push(format!("{rate}K"));
    }
    if let Some(proxy) = spec.proxy.as_deref().filter(|p| !p.is_empty()) {
        args.push("--proxy".into());
        args.push(proxy.into());
    }
    if let Some(browser) = spec.cookies_from_browser.as_deref().filter(|b| !b.is_empty()) {
        args.push("--cookies-from-browser".into());
        args.push(browser.into());
    }
    if let Some(archive) = spec.download_archive_path.as_deref().filter(|a| !a.is_empty()) {
        args.push("--download-archive".into());
        args.push(archive.into());
    }
    if let Some(ffdir) = spec.ffmpeg_dir.as_deref().filter(|d| !d.is_empty()) {
        args.push("--ffmpeg-location".into());
        args.push(ffdir.into());
    }

    args.push("--".into());
    args.push(url.to_string());

    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_spec() -> DownloadSpec {
        DownloadSpec {
            mode: Mode::Video,
            max_height: Some(1080),
            container: VideoContainer::Mp4,
            video_only: false,
            audio_format: AudioFormat::Mp3,
            audio_quality: AudioQuality::Best,
            extras: Extras::default(),
            output_dir: "/tmp/out".into(),
            filename_template: "%(title)s.%(ext)s".into(),
            rate_limit_kbps: None,
            proxy: None,
            cookies_from_browser: None,
            download_archive_path: None,
            ffmpeg_dir: None,
            network_retries: 10,
        }
    }

    #[test]
    fn video_1080_mp4_prefers_avc1_for_quicktime_compat_and_merges() {
        let spec = base_spec();
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        assert!(argv.contains(&"-f".to_string()));
        let f_index = argv.iter().position(|a| a == "-f").unwrap();
        assert_eq!(
            argv[f_index + 1],
            "bestvideo[height<=1080][vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo[height<=1080]+bestaudio/best[height<=1080]"
        );
        assert!(argv.contains(&"--merge-output-format".to_string()));
        assert!(argv.contains(&"mp4".to_string()));
        assert!(!argv.contains(&"-x".to_string()));
    }

    #[test]
    fn video_best_quality_omits_height_filter() {
        let mut spec = base_spec();
        spec.max_height = None;
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        let f_index = argv.iter().position(|a| a == "-f").unwrap();
        assert_eq!(
            argv[f_index + 1],
            "bestvideo[vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo+bestaudio/best"
        );
    }

    #[test]
    fn video_only_strips_audio_track_from_selector() {
        let mut spec = base_spec();
        spec.video_only = true;
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        let f_index = argv.iter().position(|a| a == "-f").unwrap();
        assert_eq!(
            argv[f_index + 1],
            "bestvideo[height<=1080][vcodec^=avc1]/bestvideo[height<=1080]/best[height<=1080]"
        );
        assert!(!argv[f_index + 1].contains("bestaudio"));
    }

    #[test]
    fn mkv_container_does_not_restrict_codec() {
        let mut spec = base_spec();
        spec.container = VideoContainer::Mkv;
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        let f_index = argv.iter().position(|a| a == "-f").unwrap();
        assert_eq!(argv[f_index + 1], "bestvideo[height<=1080]+bestaudio/best[height<=1080]");
        assert!(!argv[f_index + 1].contains("avc1"));
    }

    #[test]
    fn audio_mp3_best_sets_extract_flags_not_video_flags() {
        let mut spec = base_spec();
        spec.mode = Mode::Audio;
        spec.audio_format = AudioFormat::Mp3;
        spec.audio_quality = AudioQuality::Kbps(320);
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        assert!(argv.contains(&"-x".to_string()));
        assert!(!argv.contains(&"-f".to_string()));
        assert!(!argv.contains(&"--merge-output-format".to_string()));
        let fmt_index = argv.iter().position(|a| a == "--audio-format").unwrap();
        assert_eq!(argv[fmt_index + 1], "mp3");
        let q_index = argv.iter().position(|a| a == "--audio-quality").unwrap();
        assert_eq!(argv[q_index + 1], "320K");
    }

    #[test]
    fn extras_add_expected_flags() {
        let mut spec = base_spec();
        spec.extras.embed_thumbnail = true;
        spec.extras.embed_metadata = true;
        spec.extras.embed_chapters = true;
        spec.extras.write_info_json = true;
        spec.extras.sponsorblock = true;
        spec.extras.subtitles = SubtitleOptions {
            enabled: true,
            langs: vec!["ru".into(), "en".into()],
            auto: true,
            embed: true,
        };
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        for flag in [
            "--embed-thumbnail",
            "--embed-metadata",
            "--embed-chapters",
            "--write-info-json",
            "--sponsorblock-remove",
            "--write-subs",
            "--write-auto-subs",
            "--sub-langs",
            "--embed-subs",
        ] {
            assert!(argv.contains(&flag.to_string()), "missing {flag}");
        }
        let langs_index = argv.iter().position(|a| a == "--sub-langs").unwrap();
        assert_eq!(argv[langs_index + 1], "ru,en");
    }

    #[test]
    fn subtitles_disabled_omits_all_subtitle_flags() {
        let spec = base_spec();
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        assert!(!argv.iter().any(|a| a.contains("subs")));
    }

    #[test]
    fn optional_network_and_archive_flags_only_when_set() {
        let mut spec = base_spec();
        spec.rate_limit_kbps = Some(500);
        spec.proxy = Some("socks5://127.0.0.1:1080".into());
        spec.cookies_from_browser = Some("chrome".into());
        spec.download_archive_path = Some("/tmp/archive.txt".into());
        spec.ffmpeg_dir = Some("/opt/homebrew/bin".into());
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        assert!(argv.windows(2).any(|w| w[0] == "--limit-rate" && w[1] == "500K"));
        assert!(argv
            .windows(2)
            .any(|w| w[0] == "--proxy" && w[1] == "socks5://127.0.0.1:1080"));
        assert!(argv.windows(2).any(|w| w[0] == "--cookies-from-browser" && w[1] == "chrome"));
        assert!(argv
            .windows(2)
            .any(|w| w[0] == "--download-archive" && w[1] == "/tmp/archive.txt"));
        assert!(argv
            .windows(2)
            .any(|w| w[0] == "--ffmpeg-location" && w[1] == "/opt/homebrew/bin"));
    }

    #[test]
    fn url_is_last_and_separated_by_double_dash() {
        let spec = base_spec();
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        assert_eq!(argv[argv.len() - 1], "https://www.youtube.com/watch?v=abc");
        assert_eq!(argv[argv.len() - 2], "--");
    }

    #[test]
    fn network_retries_apply_to_both_retries_flags() {
        let mut spec = base_spec();
        spec.network_retries = 25;
        let argv = to_argv(&spec, "https://www.youtube.com/watch?v=abc");
        assert!(argv.windows(2).any(|w| w[0] == "--retries" && w[1] == "25"));
        assert!(argv.windows(2).any(|w| w[0] == "--fragment-retries" && w[1] == "25"));
    }
}

#[cfg(test)]
mod wire_format {
    // Locks in the JSON shape the TS side (src/lib/types.ts) depends on.
    use super::*;

    #[test]
    fn audio_quality_matches_ts_discriminated_union() {
        assert_eq!(serde_json::to_string(&AudioQuality::Best).unwrap(), r#"{"kind":"best"}"#);
        assert_eq!(
            serde_json::to_string(&AudioQuality::Kbps(320)).unwrap(),
            r#"{"kind":"kbps","value":320}"#
        );
    }
}
