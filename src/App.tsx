import { useEffect, useState } from "react";
import { Download, ListChecks, History, Settings as SettingsIcon } from "lucide-react";
import { DepsGate } from "./components/DepsGate";
import { UrlBar } from "./components/UrlBar";
import { AnalysisPanel } from "./components/AnalysisPanel";
import { PlaylistTable } from "./components/PlaylistTable";
import { FormatPicker } from "./components/FormatPicker";
import { QueueList } from "./components/QueueList";
import { SettingsPanel } from "./components/SettingsPanel";
import { ToastHost } from "./components/ToastHost";
import { useAnalysisStore } from "./store/useAnalysis";
import { useFormatStore } from "./store/useFormat";
import { useQueueStore } from "./store/useQueue";
import { useSettingsStore } from "./store/useSettings";
import { useT } from "./i18n";
import { withSettings } from "./lib/defaultSpec";
import type { DownloadSpec, NewJobRequest } from "./lib/types";

type Tab = "queue" | "history" | "settings";

function Tabs({ tab, setTab }: { tab: Tab; setTab: (t: Tab) => void }) {
  const t = useT();
  const items: { id: Tab; label: string; icon: React.ReactNode }[] = [
    { id: "queue", label: t("queue.title"), icon: <ListChecks size={14} /> },
    { id: "history", label: t("queue.history"), icon: <History size={14} /> },
    { id: "settings", label: t("queue.settings"), icon: <SettingsIcon size={14} /> },
  ];
  return (
    <div className="flex gap-1 border-b px-3 pt-2" style={{ borderColor: "var(--border)" }}>
      {items.map((item) => (
        <button
          key={item.id}
          onClick={() => setTab(item.id)}
          className="flex items-center gap-1.5 rounded-t-lg px-3 py-2 text-sm font-medium transition-colors"
          style={{
            color: tab === item.id ? "var(--text)" : "var(--text-muted)",
            borderBottom: tab === item.id ? "2px solid var(--accent)" : "2px solid transparent",
          }}
        >
          {item.icon}
          {item.label}
        </button>
      ))}
    </div>
  );
}

function AppShell() {
  const t = useT();
  const [tab, setTab] = useState<Tab>("queue");
  const [savePlaylistFolder, setSavePlaylistFolder] = useState(true);
  const { analysis, selectedIds, overrides, reset: resetAnalysis } = useAnalysisStore();
  const { spec, setSpec } = useFormatStore();
  const { enqueue } = useQueueStore();
  const { settings } = useSettingsStore();

  useEffect(() => {
    void useQueueStore.getState().init();
    void useSettingsStore.getState().init();
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.key === "Enter") {
        e.preventDefault();
        void handleDownload();
      } else if (e.key === ",") {
        e.preventDefault();
        setTab("settings");
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [analysis, selectedIds, overrides, spec, settings]);

  const selectedCount = analysis?.kind === "playlist" ? selectedIds.size : analysis ? 1 : 0;

  const durationForEstimate = analysis?.kind === "video" ? analysis.duration : null;
  const availableHeights = analysis?.kind === "video" ? analysis.availableHeights : [];

  const handleDownload = async () => {
    if (!analysis || !settings || selectedCount === 0) return;

    let requests: NewJobRequest[] = [];
    if (analysis.kind === "video") {
      requests = [
        {
          groupId: null,
          groupTitle: null,
          videoId: analysis.id,
          title: analysis.title,
          thumbnail: analysis.thumbnail,
          url: analysis.webpageUrl,
          spec: withSettings(spec, settings),
        },
      ];
    } else {
      requests = analysis.entries
        .filter((e) => selectedIds.has(e.id))
        .map((e) => {
          const itemSpec: DownloadSpec = overrides[e.id] ? { ...spec, ...overrides[e.id] } : spec;
          const finalSpec = withSettings(itemSpec, settings);
          return {
            groupId: analysis.id,
            groupTitle: analysis.title,
            videoId: e.id,
            title: e.title,
            thumbnail: e.thumbnail,
            url: e.webpageUrl,
            spec: savePlaylistFolder
              ? { ...finalSpec, filenameTemplate: `%(playlist_title)s/${finalSpec.filenameTemplate}` }
              : finalSpec,
          };
        });
    }

    await enqueue(requests);
    resetAnalysis();
    setTab("queue");
  };

  return (
    <div className="flex h-screen w-screen flex-col" style={{ background: "var(--bg)", color: "var(--text)" }}>
      <div className="flex items-center gap-2 border-b px-4 py-2.5" style={{ borderColor: "var(--border)" }} data-tauri-drag-region>
        <img src="/app-icon.svg" alt="" className="h-5 w-5" />
        <span className="text-sm font-semibold">{t("app.title")}</span>
      </div>

      <div className="flex min-h-0 flex-1">
        <div className="flex w-[58%] flex-col gap-4 overflow-y-auto border-r p-5" style={{ borderColor: "var(--border)" }}>
          <UrlBar />
          <AnalysisPanel />
          {analysis?.kind === "playlist" && (
            <>
              <PlaylistTable />
              <label className="flex cursor-pointer items-start gap-2.5 rounded-lg border p-3" style={{ borderColor: "var(--border)" }}>
                <input
                  type="checkbox"
                  checked={savePlaylistFolder}
                  onChange={(e) => setSavePlaylistFolder(e.target.checked)}
                  className="mt-0.5 h-4 w-4 shrink-0 accent-[var(--accent)]"
                />
                <span className="flex flex-col gap-0.5">
                  <span className="text-sm font-medium">{t("playlist.saveInFolder")}</span>
                  <span className="text-xs" style={{ color: "var(--text-faint)" }}>
                    {t("playlist.saveInFolderHint", { title: analysis.title })}
                  </span>
                </span>
              </label>
            </>
          )}

          {analysis && (
            <div className="flex flex-col gap-4 rounded-xl border p-4" style={{ borderColor: "var(--border)" }}>
              <FormatPicker
                spec={spec}
                onChange={setSpec}
                availableHeights={availableHeights}
                durationSeconds={durationForEstimate}
              />
              <button
                onClick={() => void handleDownload()}
                disabled={selectedCount === 0}
                className="flex items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-semibold transition-opacity disabled:opacity-50"
                style={{ background: "var(--accent)", color: "white" }}
              >
                <Download size={16} />
                {t("format.download", { count: selectedCount })}
              </button>
            </div>
          )}
        </div>

        <div className="flex w-[42%] flex-col overflow-hidden">
          <Tabs tab={tab} setTab={setTab} />
          <div className="flex-1 overflow-y-auto p-4">
            {tab === "queue" && <QueueList mode="active" />}
            {tab === "history" && <QueueList mode="history" />}
            {tab === "settings" && <SettingsPanel />}
          </div>
        </div>
      </div>

      <ToastHost />
    </div>
  );
}

function App() {
  const [ready, setReady] = useState(false);

  if (!ready) {
    return <DepsGate onReady={() => setReady(true)} />;
  }
  return <AppShell />;
}

export default App;
