import { create } from "zustand";
import { api } from "../lib/api";
import type { Analysis, DownloadSpec, PlaylistEntry, PlaylistInfo } from "../lib/types";
import { useUrlHistoryStore } from "./useUrlHistory";
import { useQueueStore } from "./useQueue";
import { translate, useLocaleStore } from "../i18n";

/** Video ids that already have a finished download in the queue/history. */
export function downloadedIds(): Set<string> {
  return new Set(
    useQueueStore
      .getState()
      .jobs.filter((j) => j.status === "done")
      .map((j) => j.videoId),
  );
}

function initialSelection(analysis: Analysis): Set<string> {
  if (analysis.kind !== "playlist") return new Set();
  const done = downloadedIds();
  return new Set(analysis.entries.filter((e) => !done.has(e.id)).map((e) => e.id));
}

function detectDisambiguation(raw: string): { videoUrl: string; playlistUrl: string } | null {
  try {
    const u = new URL(raw.trim());
    const v = u.searchParams.get("v");
    const list = u.searchParams.get("list");
    const isRealPlaylist = list && !/^(RD|UL|LL)/.test(list);
    if (v && isRealPlaylist) {
      return {
        videoUrl: `https://www.youtube.com/watch?v=${v}`,
        playlistUrl: `https://www.youtube.com/playlist?list=${list}`,
      };
    }
  } catch {
    // not URL-parseable; let the backend produce a friendly validation error
  }
  return null;
}

/** All http(s) links in the text — handles newline/space/comma separated and glued-together links. */
export function extractUrls(text: string): string[] {
  const found = text.match(/https?:\/\/.+?(?=https?:\/\/|[\s,;]|$)/g) ?? [];
  return [...new Set(found)];
}

/** Analyzes each link and merges videos + playlist entries into one synthetic playlist. */
async function analyzeMany(urls: string[]): Promise<{ analysis: PlaylistInfo | null; failed: string[] }> {
  const results = await Promise.allSettled(urls.map((u) => api.analyzeUrl(u)));
  const entries: PlaylistEntry[] = [];
  const seen = new Set<string>();
  const failed: string[] = [];
  const push = (e: Omit<PlaylistEntry, "index">) => {
    if (seen.has(e.id)) return;
    seen.add(e.id);
    entries.push({ ...e, index: entries.length + 1 });
  };
  results.forEach((r, i) => {
    if (r.status === "rejected") {
      failed.push(`${urls[i]}: ${String(r.reason)}`);
      return;
    }
    const a = r.value;
    if (a.kind === "video") {
      push({ id: a.id, title: a.title, duration: a.duration, thumbnail: a.thumbnail, uploader: a.uploader, webpageUrl: a.webpageUrl });
    } else {
      a.entries.forEach(push);
    }
  });
  if (entries.length === 0) return { analysis: null, failed };
  return {
    analysis: {
      kind: "playlist",
      id: `batch-${Date.now()}`,
      title: translate(useLocaleStore.getState().locale, "urlbar.batchTitle", { count: urls.length - failed.length }),
      uploader: null,
      thumbnail: entries[0].thumbnail,
      webpageUrl: urls.join(" "),
      entries,
    },
    failed,
  };
}

interface AnalysisState {
  url: string;
  analysis: Analysis | null;
  loading: boolean;
  error: string | null;
  pendingDisambiguation: { videoUrl: string; playlistUrl: string } | null;
  selectedIds: Set<string>;
  overrides: Record<string, Partial<DownloadSpec>>;

  setUrl: (url: string) => void;
  analyze: (url: string) => Promise<void>;
  loadCached: (url: string, analysis: Analysis) => void;
  resolveDisambiguation: (choice: "video" | "playlist") => void;
  reset: () => void;

  toggleSelected: (id: string) => void;
  selectAll: () => void;
  deselectAll: () => void;
  invertSelection: () => void;
  selectRange: (from: number, to: number) => void;
  setOverride: (id: string, spec: Partial<DownloadSpec> | null) => void;
}

export const useAnalysisStore = create<AnalysisState>((set, get) => ({
  url: "",
  analysis: null,
  loading: false,
  error: null,
  pendingDisambiguation: null,
  selectedIds: new Set(),
  overrides: {},

  setUrl: (url) => set({ url }),

  analyze: async (rawUrl) => {
    const urls = extractUrls(rawUrl);
    if (urls.length > 1) {
      set({ loading: true, error: null, pendingDisambiguation: null });
      const { analysis, failed } = await analyzeMany(urls);
      set({
        loading: false,
        error: failed.length ? failed.join("\n") : null,
        ...(analysis ? { analysis, selectedIds: initialSelection(analysis), overrides: {} } : {}),
      });
      return;
    }
    const disambiguation = detectDisambiguation(rawUrl);
    if (disambiguation) {
      set({ pendingDisambiguation: disambiguation, error: null });
      return;
    }
    set({ loading: true, error: null, pendingDisambiguation: null });
    try {
      const analysis = await api.analyzeUrl(rawUrl);
      set({
        url: analysis.webpageUrl,
        analysis,
        loading: false,
        selectedIds: initialSelection(analysis),
        overrides: {},
      });
      useUrlHistoryStore.getState().add(analysis.webpageUrl, analysis);
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  loadCached: (url, analysis) => {
    set({
      url,
      analysis,
      loading: false,
      error: null,
      pendingDisambiguation: null,
      selectedIds: initialSelection(analysis),
      overrides: {},
    });
  },

  resolveDisambiguation: (choice) => {
    const pending = get().pendingDisambiguation;
    if (!pending) return;
    const target = choice === "video" ? pending.videoUrl : pending.playlistUrl;
    set({ pendingDisambiguation: null, url: target });
    void get().analyze(target);
  },

  reset: () =>
    set({
      url: "",
      analysis: null,
      loading: false,
      error: null,
      pendingDisambiguation: null,
      selectedIds: new Set(),
      overrides: {},
    }),

  toggleSelected: (id) =>
    set((s) => {
      const next = new Set(s.selectedIds);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return { selectedIds: next };
    }),

  selectAll: () =>
    set((s) => {
      if (s.analysis?.kind !== "playlist") return {};
      return { selectedIds: new Set(s.analysis.entries.map((e) => e.id)) };
    }),

  deselectAll: () => set({ selectedIds: new Set() }),

  invertSelection: () =>
    set((s) => {
      if (s.analysis?.kind !== "playlist") return {};
      const next = new Set<string>();
      for (const e of s.analysis.entries) {
        if (!s.selectedIds.has(e.id)) next.add(e.id);
      }
      return { selectedIds: next };
    }),

  selectRange: (from, to) =>
    set((s) => {
      if (s.analysis?.kind !== "playlist") return {};
      const lo = Math.min(from, to);
      const hi = Math.max(from, to);
      const next = new Set(s.selectedIds);
      for (const e of s.analysis.entries) {
        if (e.index >= lo && e.index <= hi) next.add(e.id);
      }
      return { selectedIds: next };
    }),

  setOverride: (id, spec) =>
    set((s) => {
      const next = { ...s.overrides };
      if (spec === null) delete next[id];
      else next[id] = spec;
      return { overrides: next };
    }),
}));
