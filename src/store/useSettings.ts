import { create } from "zustand";
import { api } from "../lib/api";
import type { Settings } from "../lib/types";
import { useLocaleStore } from "../i18n";

function applyTheme(theme: Settings["theme"]) {
  const root = document.documentElement;
  if (theme === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", theme);
  }
}

interface SettingsState {
  settings: Settings | null;
  loaded: boolean;
  init: () => Promise<void>;
  update: (patch: Partial<Settings>) => Promise<void>;
}

export const useSettingsStore = create<SettingsState>((set, get) => ({
  settings: null,
  loaded: false,

  init: async () => {
    if (get().loaded) return;
    const settings = await api.getSettings();
    set({ settings, loaded: true });
    applyTheme(settings.theme);
    useLocaleStore.getState().setLocale(settings.locale);
  },

  update: async (patch) => {
    const current = get().settings;
    if (!current) return;
    const next = { ...current, ...patch };
    set({ settings: next });
    if (patch.theme) applyTheme(next.theme);
    if (patch.locale) useLocaleStore.getState().setLocale(next.locale);
    await api.saveSettings(next);
  },
}));
