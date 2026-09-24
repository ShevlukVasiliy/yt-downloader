import { useState } from "react";
import {
  Pause,
  Play,
  X,
  RotateCcw,
  FolderOpen,
  ChevronDown,
  ChevronUp,
  Trash2,
  Loader2,
  CheckCircle2,
  AlertCircle,
  Music,
  Video as VideoIcon,
} from "lucide-react";
import type { Job, JobStatus } from "../lib/types";
import { useT } from "../i18n";
import { useQueueStore } from "../store/useQueue";
import { formatBytes, formatEta, formatSpeed, formatPercent, truncate } from "../lib/format";
import { api } from "../lib/api";

const ACTIVE_STATUSES: JobStatus[] = ["downloading", "merging", "converting"];

function statusColor(status: JobStatus): string {
  switch (status) {
    case "done":
      return "var(--success)";
    case "error":
      return "var(--danger)";
    case "canceled":
    case "paused":
      return "var(--text-faint)";
    default:
      return "var(--accent)";
  }
}

export function JobCard({ job }: { job: Job }) {
  const t = useT();
  const { pause, cancel, retry, remove } = useQueueStore();
  const [detailsOpen, setDetailsOpen] = useState(false);

  const percent = formatPercent(job.progress.downloadedBytes, job.progress.totalBytes);
  const isActive = ACTIVE_STATUSES.includes(job.status);
  const canPause = isActive || job.status === "queued";
  const canResume = job.status === "paused";
  const canCancel = job.status === "queued" || isActive || job.status === "paused";
  const canRetry = job.status === "error" || job.status === "canceled";
  // Removal is always available — for a running/queued job it cancels first
  // (backend), so it never leaves an orphaned yt-dlp process behind.
  const canRemove = true;

  return (
    <div className="flex gap-3 rounded-xl border p-3" style={{ borderColor: "var(--border)", background: "var(--bg-elevated)" }}>
      <div className="relative h-14 w-20 shrink-0 overflow-hidden rounded-lg" style={{ background: "var(--bg-elevated-2)" }}>
        {job.thumbnail ? (
          <img src={job.thumbnail} alt="" className="h-full w-full object-cover" />
        ) : (
          <div className="flex h-full w-full items-center justify-center">
            {job.spec.mode === "audio" ? (
              <Music size={16} style={{ color: "var(--text-faint)" }} />
            ) : (
              <VideoIcon size={16} style={{ color: "var(--text-faint)" }} />
            )}
          </div>
        )}
      </div>

      <div className="flex min-w-0 flex-1 flex-col gap-1.5">
        <div className="flex items-start justify-between gap-2">
          <p className="min-w-0 flex-1 truncate text-sm font-medium">{truncate(job.title, 80)}</p>
          <span className="shrink-0 text-xs font-medium" style={{ color: statusColor(job.status) }}>
            {t(`queue.status.${job.status}` as const)}
          </span>
        </div>

        {isActive && (
          <div className="h-1.5 w-full overflow-hidden rounded-full" style={{ background: "var(--bg-elevated-2)" }}>
            <div
              className="h-full rounded-full transition-all"
              style={{
                width: `${percent ?? (job.status === "downloading" ? 3 : 100)}%`,
                background: "var(--accent)",
                opacity: job.status === "downloading" ? 1 : 0.6,
              }}
            />
          </div>
        )}

        <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs" style={{ color: "var(--text-muted)" }}>
          {job.status === "downloading" && (
            <>
              {percent != null && <span>{percent}%</span>}
              <span>
                {formatBytes(job.progress.downloadedBytes)}
                {job.progress.totalBytes ? ` / ${formatBytes(job.progress.totalBytes)}` : ""}
              </span>
              <span>{formatSpeed(job.progress.speedBytesPerSec)}</span>
              <span>ETA {formatEta(job.progress.etaSeconds)}</span>
            </>
          )}
          {(job.status === "merging" || job.status === "converting") && (
            <span className="flex items-center gap-1">
              <Loader2 size={11} className="animate-spin" />
              {t(`queue.status.${job.status}` as const)}…
            </span>
          )}
          {job.status === "done" && (
            <span className="flex items-center gap-1" style={{ color: "var(--success)" }}>
              <CheckCircle2 size={12} />
              {job.outputPath ? truncate(job.outputPath.split("/").pop() ?? "", 50) : ""}
            </span>
          )}
          {job.status === "error" && (
            <button
              onClick={() => setDetailsOpen((v) => !v)}
              className="flex items-center gap-1"
              style={{ color: "var(--danger)" }}
            >
              <AlertCircle size={12} />
              {truncate(job.error ?? "", 60)}
              {detailsOpen ? <ChevronUp size={12} /> : <ChevronDown size={12} />}
            </button>
          )}
        </div>

        {detailsOpen && job.status === "error" && (job.errorDetail || job.error) && (
          <pre
            className="max-h-32 overflow-auto whitespace-pre-wrap rounded-md p-2 text-[11px]"
            style={{ background: "var(--bg)", color: "var(--text-muted)" }}
          >
            {job.errorDetail ?? job.error}
          </pre>
        )}
      </div>

      <div className="flex shrink-0 items-center gap-1">
        {canPause && (
          <button onClick={() => pause(job.id)} className="rounded-md p-1.5 opacity-70 hover:opacity-100" title={t("queue.pause")}>
            <Pause size={15} />
          </button>
        )}
        {canResume && (
          <button onClick={() => retry(job.id)} className="rounded-md p-1.5 opacity-70 hover:opacity-100" title={t("queue.resume")}>
            <Play size={15} />
          </button>
        )}
        {canRetry && (
          <button onClick={() => retry(job.id)} className="rounded-md p-1.5 opacity-70 hover:opacity-100" title={t("queue.retry")}>
            <RotateCcw size={15} />
          </button>
        )}
        {job.status === "done" && job.outputPath && (
          <button
            onClick={() => void api.revealInFolder(job.outputPath!)}
            className="rounded-md p-1.5 opacity-70 hover:opacity-100"
            title={t("queue.showInFinder")}
          >
            <FolderOpen size={15} />
          </button>
        )}
        {canCancel && (
          <button onClick={() => cancel(job.id)} className="rounded-md p-1.5 opacity-70 hover:opacity-100" title={t("queue.cancel")}>
            <X size={15} />
          </button>
        )}
        {canRemove && (
          <button onClick={() => remove(job.id)} className="rounded-md p-1.5 opacity-70 hover:opacity-100" title={t("queue.remove")}>
            <Trash2 size={15} />
          </button>
        )}
      </div>
    </div>
  );
}
