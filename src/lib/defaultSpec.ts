import type { DownloadSpec, Settings } from "./types";

export function createDefaultSpec(): DownloadSpec {
  return {
    mode: "video",
    maxHeight: null,
    container: "mp4",
    videoOnly: false,
    audioFormat: "mp3",
    audioQuality: { kind: "best" },
    extras: {
      embedThumbnail: true,
      embedMetadata: true,
      embedChapters: false,
      sponsorblock: false,
      writeInfoJson: false,
      subtitles: { enabled: false, langs: ["ru", "en"], auto: false, embed: false },
    },
    outputDir: "",
    filenameTemplate: "%(title)s.%(ext)s",
    rateLimitKbps: null,
    proxy: null,
    cookiesFromBrowser: null,
    cookiesFile: null,
    downloadArchivePath: null,
    ffmpegDir: null,
    networkRetries: 10,
  };
}

export function withSettings(spec: DownloadSpec, settings: Settings): DownloadSpec {
  return {
    ...spec,
    outputDir: settings.downloadDir,
    filenameTemplate: settings.filenameTemplate,
    rateLimitKbps: settings.rateLimitKbps,
    proxy: settings.proxy,
    cookiesFromBrowser: settings.cookiesFromBrowser,
    cookiesFile: settings.cookiesFile,
    networkRetries: settings.networkRetries,
  };
}
