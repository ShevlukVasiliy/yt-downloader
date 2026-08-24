import { useEffect, useState } from "react";
import { CheckCircle2, Loader2, XCircle, RefreshCw, DownloadCloud } from "lucide-react";
import { api } from "../lib/api";
import { useT } from "../i18n";
import type { DepsStatus, ToolStatus } from "../lib/types";

function Row({ tool, label, optional }: { tool: ToolStatus; label: string; optional?: boolean }) {
  const t = useT();
  return (
    <div className="flex items-center justify-between rounded-lg border px-3.5 py-2.5" style={{ borderColor: "var(--border)" }}>
      <div className="flex items-center gap-2.5">
        {tool.found ? (
          <CheckCircle2 size={18} className="shrink-0" style={{ color: "var(--success)" }} />
        ) : optional ? (
          <XCircle size={18} className="shrink-0" style={{ color: "var(--text-faint)" }} />
        ) : (
          <XCircle size={18} className="shrink-0" style={{ color: "var(--danger)" }} />
        )}
        <span className="font-medium">
          {label}
          {optional && !tool.found && <span style={{ color: "var(--text-faint)" }}> ({t("deps.optional")})</span>}
        </span>
      </div>
      <span className="text-sm" style={{ color: "var(--text-muted)" }}>
        {tool.found
          ? `${tool.version ?? ""} · ${tool.bundled ? t("deps.bundled") : t("deps.system")}`
          : t("deps.notFound")}
      </span>
    </div>
  );
}

const FIRST_RUN_HINT_KEY = "yt-downloader:deps-checked-once";

export function DepsGate({ onReady }: { onReady: () => void }) {
  const t = useT();
  const [status, setStatus] = useState<DepsStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const [slow, setSlow] = useState(false);
  // The "first launch can be slow" hint is only useful the very first time the
  // app ever checks deps — show it once, then stay quiet on every later launch.
  const [isFirstEverCheck] = useState(() => localStorage.getItem(FIRST_RUN_HINT_KEY) !== "1");

  const check = async () => {
    setChecking(true);
    setSlow(false);
    const slowTimer = isFirstEverCheck ? setTimeout(() => setSlow(true), 4000) : null;
    try {
      const s = await api.checkDeps();
      setStatus(s);
      if (s.allReady) onReady();
    } finally {
      if (slowTimer) clearTimeout(slowTimer);
      setChecking(false);
      setSlow(false);
      localStorage.setItem(FIRST_RUN_HINT_KEY, "1");
    }
  };

  useEffect(() => {
    void check();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="flex h-full w-full items-center justify-center p-8">
      <div
        className="w-full max-w-md rounded-2xl border p-7 shadow-xl"
        style={{ background: "var(--bg-elevated)", borderColor: "var(--border)" }}
      >
        <div
          className="mb-4 flex h-12 w-12 items-center justify-center rounded-xl"
          style={{ background: "var(--accent-muted)" }}
        >
          <DownloadCloud size={24} style={{ color: "var(--accent)" }} />
        </div>
        <h1 className="text-lg font-semibold">{t("deps.title")}</h1>
        <p className="mt-1.5 text-sm" style={{ color: "var(--text-muted)" }}>
          {t("deps.subtitle")}
        </p>

        {checking && !status ? (
          <div className="mt-6 flex flex-col gap-1.5 text-sm" style={{ color: "var(--text-muted)" }}>
            <div className="flex items-center gap-2">
              <Loader2 size={16} className="animate-spin" />
              {t("deps.checking")}
            </div>
            {slow && <p className="text-xs">{t("deps.firstRunSlow")}</p>}
          </div>
        ) : status ? (
          <div className="mt-6 flex flex-col gap-2">
            <Row tool={status.ytDlp} label="yt-dlp" />
            <Row tool={status.ffmpeg} label="ffmpeg" />
            <Row tool={status.ffprobe} label="ffprobe" />
            <Row tool={status.jsRuntime} label={t("deps.jsRuntime")} />
            <Row tool={status.potProvider} label={t("deps.potProvider")} optional />
          </div>
        ) : null}

        {status && !status.allReady && (
          <div className="mt-5 rounded-lg p-3 text-xs" style={{ background: "var(--bg-elevated-2)", color: "var(--text-muted)" }}>
            <div className="mb-1.5">{t("deps.installHint")}</div>
            <code className="block rounded px-2 py-1.5" style={{ background: "var(--bg)", color: "var(--text)" }}>
              brew install yt-dlp ffmpeg
            </code>
          </div>
        )}

        <button
          onClick={() => void check()}
          disabled={checking}
          className="mt-5 flex w-full items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium transition-colors disabled:opacity-60"
          style={{ background: "var(--accent)", color: "white" }}
        >
          {checking ? <Loader2 size={16} className="animate-spin" /> : <RefreshCw size={16} />}
          {t("deps.retry")}
        </button>
      </div>
    </div>
  );
}
