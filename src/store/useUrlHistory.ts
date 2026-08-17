import { create } from "zustand";
import type { Analysis } from "../lib/types";

export interface UrlHistoryEntry {
  url: string;
  analysis: Analysis;
  timestamp: number;
}

const STORAGE_KEY = "yt-downloader:url-history";
const MAX_ENTRIES = 30;

function load(): UrlHistoryEntry[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function save(entries: UrlHistoryEntry[]) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
  } catch {
    // storage full/unavailable — history is a convenience cache, safe to drop
  }
}

interface UrlHistoryState {
  entries: UrlHistoryEntry[];
  add: (url: string, analysis: Analysis) => void;
}

export const useUrlHistoryStore = create<UrlHistoryState>((set, get) => ({
  entries: load(),

  add: (url, analysis) => {
    const withoutDuplicate = get().entries.filter((e) => e.url !== url);
    const next = [{ url, analysis, timestamp: Date.now() }, ...withoutDuplicate].slice(0, MAX_ENTRIES);
    set({ entries: next });
    save(next);
  },
}));
