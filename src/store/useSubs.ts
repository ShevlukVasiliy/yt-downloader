import { create } from "zustand";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { api, onSubsProgress, onSubsState } from "../lib/api";
import { createDefaultSpec, withSettings } from "../lib/defaultSpec";
import type { AutoDownload, Channel, DownloadSpec, FeedItem, ItemState, NewJobRequest, SubsState } from "../lib/types";
import { translate, useLocaleStore } from "../i18n";
import { useSettingsStore } from "./useSettings";
import { useToastStore } from "./useToast";

function toJob(item: FeedItem, spec: DownloadSpec): NewJobRequest {
  return {
    groupId: null,
    groupTitle: null,
    videoId: item.videoId,
    title: item.title,
    thumbnail: item.thumbnail,
    url: item.url,
    spec,
  };
}

async function notify(title: string, body: string) {
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (granted) sendNotification({ title, body });
  } catch {
    // notifications are a nicety — a denied or missing permission is fine
  }
}

interface SubsStoreState extends SubsState {
  loaded: boolean;
  refreshing: boolean;
  progress: { done: number; total: number } | null;

  init: () => Promise<void>;
  /** `background` refreshes stay quiet on failure and announce new uploads
   * with a system notification instead of a toast. */
  refresh: (background?: boolean) => Promise<void>;
  addChannel: (url: string) => Promise<Channel>;
  importChannels: () => Promise<number>;
  removeChannel: (id: string) => Promise<void>;
  updateChannel: (id: string, patch: { enabled?: boolean; autoDownload?: AutoDownload | null }) => Promise<void>;
  setItemsState: (ids: string[], state: ItemState) => Promise<void>;
  download: (items: FeedItem[], spec: DownloadSpec) => Promise<void>;
}

let initialized = false;

export const useSubsStore = create<SubsStoreState>((set, get) => ({
  channels: [],
  items: [],
  lastRefresh: null,
  loaded: false,
  refreshing: false,
  progress: null,

  init: async () => {
    if (initialized) return;
    initialized = true;
    const state = await api.subsGet();
    set({ ...state, loaded: true });
    await onSubsState((next) => set(next));
    await onSubsProgress((progress) => set({ progress }));
  },

  refresh: async (background = false) => {
    if (get().refreshing) return;
    const locale = useLocaleStore.getState().locale;
    set({ refreshing: true, progress: null });
    try {
      const result = await api.subsRefresh();
      const settings = useSettingsStore.getState().settings;

      // Channels marked for auto-download skip the inbox entirely.
      const channels = new Map(get().channels.map((c) => [c.id, c]));
      const auto = result.newItems.filter((i) => channels.get(i.channelId)?.autoDownload);
      if (auto.length > 0 && settings) {
        await api.enqueueJobs(
          auto.map((item) => {
            const mode = channels.get(item.channelId)!.autoDownload!;
            return toJob(item, withSettings({ ...createDefaultSpec(), mode }, settings));
          }),
        );
        await api.subsSetItemsState(
          auto.map((i) => i.videoId),
          "queued",
        );
      }

      const inbox = result.newItems.length - auto.length;
      if (background) {
        if (settings?.feedNotifications && result.newItems.length > 0) {
          const parts = [];
          if (inbox > 0) parts.push(translate(locale, "subs.notifyNew", { count: inbox }));
          if (auto.length > 0) parts.push(translate(locale, "subs.notifyAuto", { count: auto.length }));
          await notify(translate(locale, "subs.title"), parts.join(", "));
        }
      } else {
        const message =
          result.newItems.length === 0
            ? translate(locale, "subs.nothingNew")
            : translate(locale, "subs.foundNew", { count: result.newItems.length });
        const failed =
          result.failedChannels > 0 ? ` · ${translate(locale, "subs.failedChannels", { count: result.failedChannels })}` : "";
        useToastStore.getState().push({ message: message + failed });
      }
    } catch (e) {
      if (!background) useToastStore.getState().push({ message: String(e) });
    } finally {
      set({ refreshing: false, progress: null });
    }
  },

  addChannel: async (url) => api.subsAddChannel(url),

  importChannels: async () => api.subsImport(),

  removeChannel: async (id) => {
    await api.subsRemoveChannel(id);
  },

  updateChannel: async (id, patch) => {
    const channel = get().channels.find((c) => c.id === id);
    if (!channel) return;
    await api.subsUpdateChannel(
      id,
      patch.enabled ?? channel.enabled,
      patch.autoDownload === undefined ? channel.autoDownload : patch.autoDownload,
    );
  },

  setItemsState: async (ids, state) => {
    if (ids.length === 0) return;
    const idSet = new Set(ids);
    set((s) => ({ items: s.items.map((i) => (idSet.has(i.videoId) ? { ...i, state } : i)) }));
    await api.subsSetItemsState(ids, state);
  },

  download: async (items, spec) => {
    const settings = useSettingsStore.getState().settings;
    if (!settings || items.length === 0) return;
    const finalSpec = withSettings(spec, settings);
    await api.enqueueJobs(items.map((item) => toJob(item, finalSpec)));
    await get().setItemsState(
      items.map((i) => i.videoId),
      "queued",
    );
  },
}));

/** Count of uploads still waiting in the inbox, for the tab badge. */
export function useNewCount() {
  return useSubsStore((s) => s.items.filter((i) => i.state === "new").length);
}
