import { create } from "zustand";
import type { DownloadSpec } from "../lib/types";
import { createDefaultSpec } from "../lib/defaultSpec";

interface FormatState {
  spec: DownloadSpec;
  setSpec: (spec: DownloadSpec) => void;
  reset: () => void;
}

export const useFormatStore = create<FormatState>((set) => ({
  spec: createDefaultSpec(),
  setSpec: (spec) => set({ spec }),
  reset: () => set({ spec: createDefaultSpec() }),
}));
