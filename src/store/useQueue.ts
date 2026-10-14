import { create } from "zustand";
import { api, onDiskFull, onJobList, onJobProgress } from "../lib/api";
import type { DiskFullEvent, Job, NewJobRequest } from "../lib/types";
import { useToastStore } from "./useToast";
import { translate, useLocaleStore } from "../i18n";

interface QueueState {
  jobs: Job[];
  initialized: boolean;
  /** Set when a download ran out of disk space — drives the dialog. */
  diskFull: DiskFullEvent | null;
  dismissDiskFull: () => void;
  resumeAfterDiskFull: () => Promise<void>;
  init: () => Promise<void>;
  enqueue: (requests: NewJobRequest[]) => Promise<void>;
  pause: (id: string) => Promise<void>;
  cancel: (id: string) => Promise<void>;
  retry: (id: string) => Promise<void>;
  remove: (id: string) => Promise<void>;
  clearFinished: () => Promise<void>;
}

let unlisten: (() => void)[] = [];

export const useQueueStore = create<QueueState>((set, get) => ({
  jobs: [],
  initialized: false,
  diskFull: null,
  dismissDiskFull: () => set({ diskFull: null }),
  resumeAfterDiskFull: async () => {
    set({ diskFull: null });
    await api.resumeAfterDiskFull();
  },

  init: async () => {
    if (get().initialized) return;
    set({ initialized: true });
    const jobs = await api.getJobs();
    set({ jobs });

    const offList = await onJobList((nextJobs) => {
      const previous = new Map(get().jobs.map((j) => [j.id, j.status]));
      for (const job of nextJobs) {
        if (job.status === "done" && previous.get(job.id) !== "done" && job.outputPath) {
          const locale = useLocaleStore.getState().locale;
          useToastStore.getState().push({
            message: `${translate(locale, "queue.status.done")}: ${job.title}`,
            actionLabel: translate(locale, "queue.showInFinder"),
            onAction: () => void api.revealInFolder(job.outputPath!),
          });
        }
      }
      set({ jobs: nextJobs });
    });
    const offProgress = await onJobProgress(({ id, status, progress }) => {
      set((s) => ({
        jobs: s.jobs.map((j) => (j.id === id ? { ...j, status, progress } : j)),
      }));
    });
    // Several parallel downloads hit a full disk at once — keep one dialog,
    // with the most informative numbers.
    const offDiskFull = await onDiskFull((event) => {
      set((s) => ({ diskFull: s.diskFull && !event.neededBytes ? { ...event, neededBytes: s.diskFull.neededBytes } : event }));
    });
    unlisten.push(offList, offProgress, offDiskFull);
  },

  enqueue: async (requests) => {
    await api.enqueueJobs(requests);
  },
  pause: async (id) => {
    await api.pauseJob(id);
  },
  cancel: async (id) => {
    await api.cancelJob(id);
  },
  retry: async (id) => {
    await api.retryJob(id);
  },
  remove: async (id) => {
    set((s) => ({ jobs: s.jobs.filter((j) => j.id !== id) }));
    await api.removeJob(id);
  },
  clearFinished: async () => {
    const finished = get().jobs.filter((j) => ["done", "error", "canceled"].includes(j.status));
    set((s) => ({ jobs: s.jobs.filter((j) => !["done", "error", "canceled"].includes(j.status)) }));
    await Promise.all(finished.map((j) => api.removeJob(j.id)));
  },
}));
