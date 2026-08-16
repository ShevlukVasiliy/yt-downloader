import type { DownloadSpec } from "./types";

// Rough industry-average H.264/mp4 bitrates per resolution. Good enough for an
// "≈" estimate next to the download button — not meant to be exact.
const VIDEO_BITRATE_KBPS: [number, number][] = [
  [360, 1000],
  [480, 2500],
  [720, 5000],
  [1080, 8000],
  [1440, 16000],
  [2160, 35000],
];

function videoBitrateFor(maxHeight: number | null, availableHeights: number[]): number {
  const height = maxHeight ?? availableHeights[0] ?? 1080;
  let bitrate = VIDEO_BITRATE_KBPS[0][1];
  for (const [h, kbps] of VIDEO_BITRATE_KBPS) {
    if (h <= height) bitrate = kbps;
  }
  return bitrate;
}

function audioBitrateKbps(spec: DownloadSpec): number {
  if (spec.audioQuality.kind === "kbps") return spec.audioQuality.value;
  return 192; // heuristic for "best"
}

export function estimateBytesPerVideo(
  spec: DownloadSpec,
  durationSeconds: number | null | undefined,
  availableHeights: number[] = [],
): number | null {
  if (!durationSeconds || durationSeconds <= 0) return null;
  const kbps =
    spec.mode === "audio" ? audioBitrateKbps(spec) : videoBitrateFor(spec.maxHeight, availableHeights);
  return Math.round((durationSeconds * kbps * 1000) / 8);
}
