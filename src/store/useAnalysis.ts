import { create } from "zustand";
import { api } from "../lib/api";
import type { Analysis, DownloadSpec } from "../lib/types";

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
    const disambiguation = detectDisambiguation(rawUrl);
    if (disambiguation) {
      set({ pendingDisambiguation: disambiguation, error: null });
      return;
    }
    set({ loading: true, error: null, pendingDisambiguation: null });
    try {
      const analysis = await api.analyzeUrl(rawUrl);
      set({
        analysis,
        loading: false,
        selectedIds:
          analysis.kind === "playlist" ? new Set(analysis.entries.map((e) => e.id)) : new Set(),
        overrides: {},
      });
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
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
