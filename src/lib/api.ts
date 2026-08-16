import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  Analysis,
  DepsStatus,
  Job,
  JobProgress,
  JobStatus,
  NewJobRequest,
  Settings,
} from "./types";

export const api = {
  checkDeps: () => invoke<DepsStatus>("check_deps"),
  updateYtDlp: () => invoke<string>("update_yt_dlp"),

  analyzeUrl: (url: string) => invoke<Analysis>("analyze_url", { url }),

  getJobs: () => invoke<Job[]>("get_jobs"),
  enqueueJobs: (requests: NewJobRequest[]) => invoke<Job[]>("enqueue_jobs", { requests }),
  pauseJob: (id: string) => invoke<void>("pause_job", { id }),
  cancelJob: (id: string) => invoke<void>("cancel_job", { id }),
  retryJob: (id: string) => invoke<void>("retry_job", { id }),
  removeJob: (id: string) => invoke<void>("remove_job", { id }),
  setConcurrency: (value: number) => invoke<void>("set_concurrency", { value }),
  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  pickDownloadDir: (defaultPath?: string) =>
    invoke<string | null>("pick_download_dir", { defaultPath: defaultPath ?? null }),
};

export interface JobProgressEvent {
  id: string;
  status: JobStatus;
  progress: JobProgress;
}

export function onJobProgress(handler: (event: JobProgressEvent) => void) {
  return listen<JobProgressEvent>("job://progress", (e) => handler(e.payload));
}

export function onJobList(handler: (jobs: Job[]) => void) {
  return listen<Job[]>("job://list", (e) => handler(e.payload));
}
