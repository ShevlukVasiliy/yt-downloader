// Mirrors src-tauri/src/{probe,spec,queue,bin,settings}.rs — keep in sync.

export interface VideoInfo {
  kind: "video";
  id: string;
  title: string;
  uploader: string | null;
  duration: number | null;
  thumbnail: string | null;
  webpageUrl: string;
  availableHeights: number[];
  hasAudioOnly: boolean;
  isLive: boolean;
}

export interface PlaylistEntry {
  id: string;
  title: string;
  duration: number | null;
  thumbnail: string | null;
  index: number;
  uploader: string | null;
  webpageUrl: string;
}

export interface PlaylistInfo {
  kind: "playlist";
  id: string;
  title: string;
  uploader: string | null;
  thumbnail: string | null;
  webpageUrl: string;
  entries: PlaylistEntry[];
}

export type Analysis = VideoInfo | PlaylistInfo;

export type Mode = "video" | "audio";
export type VideoContainer = "mp4" | "mkv" | "webm";
export type AudioFormat = "mp3" | "m4a" | "opus" | "flac" | "wav";
export type AudioQuality = { kind: "best" } | { kind: "kbps"; value: number };

export interface SubtitleOptions {
  enabled: boolean;
  langs: string[];
  auto: boolean;
  embed: boolean;
}

export interface Extras {
  embedThumbnail: boolean;
  embedMetadata: boolean;
  embedChapters: boolean;
  sponsorblock: boolean;
  writeInfoJson: boolean;
  subtitles: SubtitleOptions;
}

export interface DownloadSpec {
  mode: Mode;
  maxHeight: number | null;
  container: VideoContainer;
  videoOnly: boolean;
  audioFormat: AudioFormat;
  audioQuality: AudioQuality;
  extras: Extras;
  outputDir: string;
  filenameTemplate: string;
  rateLimitKbps: number | null;
  proxy: string | null;
  cookiesFromBrowser: string | null;
  downloadArchivePath: string | null;
  ffmpegDir: string | null;
  networkRetries: number;
}

export type JobStatus =
  | "queued"
  | "downloading"
  | "merging"
  | "converting"
  | "paused"
  | "done"
  | "error"
  | "canceled";

export interface JobProgress {
  downloadedBytes: number | null;
  totalBytes: number | null;
  speedBytesPerSec: number | null;
  etaSeconds: number | null;
}

export interface Job {
  id: string;
  groupId: string | null;
  groupTitle: string | null;
  videoId: string;
  title: string;
  thumbnail: string | null;
  url: string;
  spec: DownloadSpec;
  status: JobStatus;
  progress: JobProgress;
  error: string | null;
  outputPath: string | null;
  createdAt: string;
  retriesLeft: number;
}

export interface NewJobRequest {
  groupId: string | null;
  groupTitle: string | null;
  videoId: string;
  title: string;
  thumbnail: string | null;
  url: string;
  spec: DownloadSpec;
}

export interface ToolStatus {
  name: string;
  found: boolean;
  version: string | null;
  bundled: boolean;
  path: string | null;
}

export interface DepsStatus {
  ytDlp: ToolStatus;
  ffmpeg: ToolStatus;
  ffprobe: ToolStatus;
  allReady: boolean;
}

export type Theme = "dark" | "light" | "system";
export type Locale = "ru" | "en";

export interface Settings {
  downloadDir: string;
  filenameTemplate: string;
  concurrency: number;
  rateLimitKbps: number | null;
  proxy: string | null;
  cookiesFromBrowser: string | null;
  theme: Theme;
  locale: Locale;
  networkRetries: number;
  autoRetryAttempts: number;
}
