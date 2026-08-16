export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !isFinite(bytes) || bytes < 0) return "—";
  if (bytes === 0) return "0 Б";
  const units = ["Б", "КБ", "МБ", "ГБ"];
  const exp = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / Math.pow(1024, exp);
  return `${value.toFixed(exp === 0 ? 0 : 1)} ${units[exp]}`;
}

export function formatSpeed(bytesPerSec: number | null | undefined): string {
  if (bytesPerSec == null || bytesPerSec <= 0) return "—";
  return `${formatBytes(bytesPerSec)}/с`;
}

export function formatEta(seconds: number | null | undefined): string {
  if (seconds == null || !isFinite(seconds) || seconds < 0) return "—";
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  return `${m}:${String(sec).padStart(2, "0")}`;
}

export function formatDuration(seconds: number | null | undefined): string {
  if (seconds == null || !isFinite(seconds) || seconds < 0) return "—";
  return formatEta(seconds);
}

export function formatPercent(downloaded: number | null, total: number | null): number | null {
  if (downloaded == null || total == null || total <= 0) return null;
  return Math.min(100, Math.round((downloaded / total) * 1000) / 10);
}

export function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max - 1)}…`;
}
