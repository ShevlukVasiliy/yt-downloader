import { Clock, ListVideo, Radio, User, X } from "lucide-react";
import { useAnalysisStore } from "../store/useAnalysis";
import { useT } from "../i18n";
import { formatDuration } from "../lib/format";

function Skeleton() {
  return (
    <div className="flex gap-4 rounded-xl border p-4" style={{ borderColor: "var(--border)" }}>
      <div className="skeleton h-24 w-40 shrink-0 rounded-lg" />
      <div className="flex flex-1 flex-col gap-2 py-1">
        <div className="skeleton h-4 w-3/4 rounded" />
        <div className="skeleton h-3 w-1/3 rounded" />
        <div className="skeleton h-3 w-1/4 rounded" />
      </div>
    </div>
  );
}

export function AnalysisPanel() {
  const t = useT();
  const { analysis, loading, reset } = useAnalysisStore();

  if (loading) return <Skeleton />;
  if (!analysis) return null;

  const totalDuration =
    analysis.kind === "playlist"
      ? analysis.entries.reduce((sum, e) => sum + (e.duration ?? 0), 0)
      : null;

  return (
    <div className="flex gap-4 rounded-xl border p-4" style={{ borderColor: "var(--border)", background: "var(--bg-elevated)" }}>
      <div className="relative h-24 w-40 shrink-0 overflow-hidden rounded-lg" style={{ background: "var(--bg-elevated-2)" }}>
        {analysis.thumbnail && (
          <img src={analysis.thumbnail} alt="" className="h-full w-full object-cover" />
        )}
        {analysis.kind === "playlist" && (
          <div
            className="absolute bottom-1 right-1 flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] font-medium"
            style={{ background: "rgba(0,0,0,0.75)", color: "white" }}
          >
            <ListVideo size={11} />
            {analysis.entries.length}
          </div>
        )}
      </div>

      <div className="flex min-w-0 flex-1 flex-col justify-center gap-1.5">
        <h2 className="truncate text-[15px] font-semibold leading-tight">{analysis.title}</h2>

        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs" style={{ color: "var(--text-muted)" }}>
          {analysis.uploader && (
            <span className="flex items-center gap-1">
              <User size={12} />
              {analysis.uploader}
            </span>
          )}
          {analysis.kind === "video" && analysis.isLive && (
            <span className="flex items-center gap-1" style={{ color: "var(--danger)" }}>
              <Radio size={12} />
              {t("analysis.live")}
            </span>
          )}
          {analysis.kind === "video" && analysis.duration != null && !analysis.isLive && (
            <span className="flex items-center gap-1">
              <Clock size={12} />
              {formatDuration(analysis.duration)}
            </span>
          )}
          {analysis.kind === "playlist" && (
            <span className="flex items-center gap-1">
              <ListVideo size={12} />
              {t("analysis.videoCount", { count: analysis.entries.length })}
            </span>
          )}
          {analysis.kind === "playlist" && totalDuration ? (
            <span className="flex items-center gap-1">
              <Clock size={12} />
              {t("analysis.totalDuration", { duration: formatDuration(totalDuration) })}
            </span>
          ) : null}
        </div>
      </div>

      <button
        onClick={reset}
        className="h-fit shrink-0 rounded-lg p-1.5 opacity-60 hover:opacity-100"
        title={t("analysis.newSearch")}
      >
        <X size={16} />
      </button>
    </div>
  );
}
