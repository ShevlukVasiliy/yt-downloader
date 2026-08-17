import { useEffect, useRef, useState } from "react";
import { Search, Loader2, Clipboard, X, ListVideo, Video } from "lucide-react";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { useAnalysisStore } from "../store/useAnalysis";
import { useUrlHistoryStore } from "../store/useUrlHistory";
import { useT } from "../i18n";

function looksLikeYoutubeUrl(text: string | null | undefined): text is string {
  if (!text) return false;
  const trimmed = text.trim();
  return (
    (trimmed.startsWith("http://") || trimmed.startsWith("https://")) &&
    (trimmed.includes("youtube.com") || trimmed.includes("youtu.be"))
  );
}

export function UrlBar() {
  const t = useT();
  const { url, setUrl, analyze, loadCached, loading, error, pendingDisambiguation, resolveDisambiguation } =
    useAnalysisStore();
  const historyEntries = useUrlHistoryStore((s) => s.entries);
  const [clipboardSuggestion, setClipboardSuggestion] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState(false);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const draftRef = useRef("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const checkClipboard = async () => {
      try {
        const text = await readText();
        if (looksLikeYoutubeUrl(text) && text.trim() !== url) {
          setClipboardSuggestion(text.trim());
        } else {
          setClipboardSuggestion(null);
        }
      } catch {
        // clipboard read can fail (permissions, empty, non-text) — ignore
      }
    };
    void checkClipboard();
    window.addEventListener("focus", checkClipboard);
    return () => window.removeEventListener("focus", checkClipboard);
  }, [url]);

  const submit = (value: string) => {
    const trimmed = value.trim();
    if (!trimmed) return;
    void analyze(trimmed);
  };

  const goToHistoryEntry = (index: number) => {
    const entry = historyEntries[index];
    if (!entry) return;
    setHistoryIndex(index);
    setUrl(entry.url);
    loadCached(entry.url, entry.analysis);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowUp") {
      if (historyEntries.length === 0) return;
      e.preventDefault();
      if (historyIndex === -1) draftRef.current = url;
      goToHistoryEntry(Math.min(historyIndex + 1, historyEntries.length - 1));
    } else if (e.key === "ArrowDown") {
      if (historyIndex === -1) return;
      e.preventDefault();
      const next = historyIndex - 1;
      if (next < 0) {
        setHistoryIndex(-1);
        setUrl(draftRef.current);
      } else {
        goToHistoryEntry(next);
      }
    }
  };

  return (
    <div className="w-full">
      <form
        onSubmit={(e) => {
          e.preventDefault();
          submit(url);
        }}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={(e) => {
          e.preventDefault();
          setDragOver(false);
          const text = e.dataTransfer.getData("text/uri-list") || e.dataTransfer.getData("text/plain");
          if (text) {
            setUrl(text.trim());
            submit(text);
          }
        }}
        className="relative flex items-center gap-2 rounded-xl border px-3 py-2.5 transition-colors"
        style={{
          background: "var(--bg-elevated)",
          borderColor: dragOver ? "var(--accent)" : "var(--border)",
        }}
      >
        <Search size={18} style={{ color: "var(--text-faint)" }} className="shrink-0" />
        <input
          ref={inputRef}
          value={url}
          onChange={(e) => {
            setUrl(e.target.value);
            if (historyIndex !== -1) setHistoryIndex(-1);
          }}
          onKeyDown={handleKeyDown}
          placeholder={historyEntries.length > 0 ? t("urlbar.placeholderWithHistory") : t("urlbar.placeholder")}
          className="no-drag min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:opacity-60"
          style={{ color: "var(--text)" }}
        />
        {url && (
          <button
            type="button"
            onClick={() => setUrl("")}
            className="no-drag rounded-md p-1 opacity-60 hover:opacity-100"
            aria-label={t("common.close")}
          >
            <X size={15} />
          </button>
        )}
        <button
          type="submit"
          disabled={loading || !url.trim()}
          className="no-drag flex shrink-0 items-center gap-1.5 rounded-lg px-3.5 py-1.5 text-sm font-medium transition-opacity disabled:opacity-50"
          style={{ background: "var(--accent)", color: "white" }}
        >
          {loading && <Loader2 size={14} className="animate-spin" />}
          {loading ? t("urlbar.analyzing") : t("urlbar.analyze")}
        </button>

        {dragOver && (
          <div
            className="pointer-events-none absolute inset-0 flex items-center justify-center rounded-xl text-sm font-medium"
            style={{ background: "var(--accent-muted)", color: "var(--accent)" }}
          >
            {t("urlbar.dropHint")}
          </div>
        )}
      </form>

      {clipboardSuggestion && !loading && (
        <button
          onClick={() => {
            setUrl(clipboardSuggestion);
            submit(clipboardSuggestion);
            setClipboardSuggestion(null);
          }}
          className="mt-2 flex items-center gap-1.5 rounded-full border px-3 py-1 text-xs transition-colors"
          style={{ borderColor: "var(--border)", color: "var(--text-muted)" }}
        >
          <Clipboard size={12} />
          {t("urlbar.pasteHint")}
          <span className="font-medium" style={{ color: "var(--accent)" }}>
            {t("urlbar.pasteButton")}
          </span>
        </button>
      )}

      {error && (
        <p className="mt-2 text-sm" style={{ color: "var(--danger)" }}>
          {error}
        </p>
      )}

      {pendingDisambiguation && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-6"
          style={{ background: "rgba(0,0,0,0.5)" }}
          onClick={() => useAnalysisStore.setState({ pendingDisambiguation: null })}
        >
          <div
            onClick={(e) => e.stopPropagation()}
            className="w-full max-w-sm rounded-2xl border p-6 shadow-2xl"
            style={{ background: "var(--bg-elevated)", borderColor: "var(--border)" }}
          >
            <h2 className="text-base font-semibold">{t("disambiguate.title")}</h2>
            <p className="mt-1 text-sm" style={{ color: "var(--text-muted)" }}>
              {t("disambiguate.subtitle")}
            </p>
            <div className="mt-4 flex flex-col gap-2">
              <button
                onClick={() => resolveDisambiguation("video")}
                className="flex items-center gap-3 rounded-lg border px-4 py-3 text-sm font-medium transition-colors hover:border-[var(--accent)]"
                style={{ borderColor: "var(--border)" }}
              >
                <Video size={18} style={{ color: "var(--accent)" }} />
                {t("disambiguate.videoOnly")}
              </button>
              <button
                onClick={() => resolveDisambiguation("playlist")}
                className="flex items-center gap-3 rounded-lg border px-4 py-3 text-sm font-medium transition-colors hover:border-[var(--accent)]"
                style={{ borderColor: "var(--border)" }}
              >
                <ListVideo size={18} style={{ color: "var(--accent)" }} />
                {t("disambiguate.wholePlaylist")}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
