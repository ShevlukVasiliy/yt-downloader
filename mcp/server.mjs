#!/usr/bin/env node
// MCP server for YT Downloader: lets an assistant inspect YouTube links and
// download video, audio or subtitles with the app's bundled yt-dlp/ffmpeg/JS
// runtime/PO-token provider and the app's own settings (download folder,
// cookies, proxy). Runs on its own — the app doesn't need to be open, and
// downloads made here don't show up in the app's queue.

import { spawn, execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { readFile, rename, rm, writeFile } from "node:fs/promises";
import { homedir, platform } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

const APP_ID = "dev.vasiliy.yt-downloader";
const here = dirname(fileURLToPath(import.meta.url));

// ---------------------------------------------------------------------------
// Locations

function dataDir() {
  if (process.env.YTD_DATA_DIR) return process.env.YTD_DATA_DIR;
  const home = homedir();
  if (platform() === "darwin") return join(home, "Library", "Application Support", APP_ID);
  if (platform() === "win32") return join(process.env.APPDATA ?? join(home, "AppData", "Roaming"), APP_ID);
  return join(process.env.XDG_DATA_HOME ?? join(home, ".local", "share"), APP_ID);
}

/** Folders that may hold `binaries/` and `pot-plugin/`, most preferred first. */
function resourceDirs() {
  const dirs = [];
  if (process.env.YTD_APP_RESOURCES) dirs.push(process.env.YTD_APP_RESOURCES);
  if (platform() === "darwin") {
    dirs.push("/Applications/YT Downloader.app/Contents/Resources");
    dirs.push(join(homedir(), "Applications", "YT Downloader.app", "Contents", "Resources"));
  }
  dirs.push(join(here, "..", "src-tauri")); // running from the repo
  return dirs;
}

const exe = (name) => (platform() === "win32" ? `${name}.exe` : name);

/** Same lookup order as the app (src-tauri/src/bin.rs): self-updated copy, bundle, PATH. */
function findTool(name) {
  const file = exe(name);
  const candidates = [join(dataDir(), "bin", file), ...resourceDirs().map((d) => join(d, "binaries", file))];
  for (const c of candidates) if (existsSync(c)) return c;
  try {
    const which = platform() === "win32" ? "where" : "which";
    return execFileSync(which, [name], { encoding: "utf8" }).split("\n")[0].trim() || null;
  } catch {
    return null;
  }
}

function findPotPlugin() {
  for (const d of resourceDirs()) {
    const p = join(d, "pot-plugin");
    if (existsSync(join(p, "bgutil"))) return p;
  }
  return null;
}

function loadSettings() {
  try {
    const raw = JSON.parse(readFileSync(join(dataDir(), "settings.json"), "utf8"));
    return raw.settings ?? {};
  } catch {
    return {};
  }
}

// ---------------------------------------------------------------------------
// yt-dlp argv (mirrors src-tauri/src/spec.rs)

function baseArgs({ withCookies = true } = {}) {
  const s = loadSettings();
  const args = ["--no-update", "--retries", String(s.networkRetries ?? 10), "--fragment-retries", String(s.networkRetries ?? 10)];

  const qjs = findTool("qjs");
  if (qjs) args.push("--js-runtimes", `quickjs:${qjs}`);
  const pot = findTool("bgutil-pot");
  const plugin = findPotPlugin();
  if (pot && plugin) {
    // CLI mode: no long-running token server to manage.
    args.push("--plugin-dirs", plugin, "--extractor-args", `youtubepot-bgutilcli:cli_path=${pot}`);
  }
  const ffmpeg = findTool("ffmpeg");
  if (ffmpeg) args.push("--ffmpeg-location", dirname(ffmpeg));

  if (s.proxy) args.push("--proxy", s.proxy);
  if (withCookies) {
    if (s.cookiesFile) args.push("--cookies", s.cookiesFile);
    else if (s.cookiesFromBrowser) args.push("--cookies-from-browser", s.cookiesFromBrowser);
  }
  return args;
}

function formatArgs(opts) {
  const args = [];
  if (opts.mode === "video") {
    const h = opts.maxHeight ? `[height<=${opts.maxHeight}]` : "";
    const container = opts.container ?? "mp4";
    // H.264 + AAC for mp4 so QuickTime can play it (see spec.rs).
    const selector =
      container === "mp4"
        ? `bestvideo${h}[vcodec^=avc1]+bestaudio[acodec^=mp4a]/bestvideo${h}+bestaudio/best${h}`
        : `bestvideo${h}+bestaudio/best${h}`;
    args.push("-f", selector, "--merge-output-format", container, "--embed-thumbnail", "--embed-metadata");
  } else if (opts.mode === "audio") {
    args.push("-x", "--audio-format", opts.audioFormat ?? "mp3", "--audio-quality", opts.audioKbps ? `${opts.audioKbps}K` : "0");
    args.push("--embed-thumbnail", "--embed-metadata");
  } else {
    const langs = [...(opts.subLangs ?? ["ru", "en"])];
    if (opts.autoSubs !== false) langs.push(".*-orig");
    args.push("--ignore-errors", "--skip-download", "--write-subs");
    if (opts.autoSubs !== false) args.push("--write-auto-subs");
    args.push("--sub-langs", langs.join(","), "--convert-subs", opts.subFormat === "vtt" ? "vtt" : "srt");
  }
  return args;
}

function runCapture(args) {
  return new Promise((resolve) => {
    const ytdlp = findTool("yt-dlp");
    if (!ytdlp) return resolve({ code: -1, stdout: "", stderr: "yt-dlp не найден — установите YT Downloader" });
    const child = spawn(ytdlp, args);
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (d) => (stdout += d));
    child.stderr.on("data", (d) => (stderr += d));
    child.on("close", (code) => resolve({ code, stdout, stderr }));
    child.on("error", (e) => resolve({ code: -1, stdout, stderr: String(e) }));
  });
}

function errorLine(stderr) {
  const lines = stderr.split("\n").filter((l) => l.startsWith("ERROR"));
  return lines.at(-1) ?? stderr.trim().split("\n").at(-1) ?? "unknown error";
}

// ---------------------------------------------------------------------------
// Subtitles post-processing (mirrors finish_subtitles in src-tauri/src/job.rs)

function srtToText(srt) {
  const out = [];
  for (const raw of srt.split("\n")) {
    const line = raw.trim();
    if (!line || line.includes("-->") || /^\d+$/.test(line)) continue;
    const clean = line.replace(/<[^>]*>/g, "").trim();
    if (clean && out.at(-1) !== clean) out.push(clean);
  }
  return out.join("\n") + "\n";
}

async function finishSubtitles(written, format) {
  const ext = format === "vtt" ? "vtt" : "srt";
  const converted = [...new Set(written.map((p) => p.replace(/\.[^./]+$/, `.${ext}`)))].filter((p) => existsSync(p));
  const finals = [];
  for (let path of converted) {
    const orig = path.match(/^(.*)-orig\.(\w+)$/);
    if (orig) {
      const plain = `${orig[1]}.${orig[2]}`;
      if (converted.includes(plain)) {
        await rm(path, { force: true });
        continue;
      }
      await rename(path, plain);
      path = plain;
    }
    if (format === "txt") {
      const txt = path.replace(/\.srt$/, ".txt");
      await writeFile(txt, srtToText(await readFile(path, "utf8")));
      await rm(path, { force: true });
      path = txt;
    }
    finals.push(path);
  }
  return finals;
}

// ---------------------------------------------------------------------------
// Jobs

const jobs = new Map();
const waiting = [];
let running = 0;

function maxParallel() {
  return Math.min(Math.max(loadSettings().concurrency ?? 3, 1), 5);
}

function pump() {
  while (running < maxParallel() && waiting.length > 0) {
    const job = waiting.shift();
    running++;
    runJob(job).finally(() => {
      running--;
      pump();
    });
  }
}

function runJob(job) {
  return new Promise((resolve) => {
    const ytdlp = findTool("yt-dlp");
    if (!ytdlp) {
      Object.assign(job, { status: "error", error: "yt-dlp не найден — установите YT Downloader" });
      return resolve();
    }
    const s = loadSettings();
    const outDir = job.outputDir ?? s.downloadDir ?? join(homedir(), "Downloads", "YouTube");
    const template = job.playlist
      ? `%(playlist_title)s/${s.filenameTemplate ?? "%(title)s.%(ext)s"}`
      : (s.filenameTemplate ?? "%(title)s.%(ext)s");

    const args = [...baseArgs(), ...formatArgs(job.opts), "--newline", "--continue", "--paths", outDir, "-o", template];
    if (!job.playlist) args.push("--no-playlist");
    if (job.items) args.push("--playlist-items", job.items);
    if (s.rateLimitKbps) args.push("--limit-rate", `${s.rateLimitKbps}K`);
    if (job.opts.mode !== "subtitles") {
      args.push("--progress", "--progress-template", "download:[P]%(progress._percent_str)s|%(info.title)s");
      args.push("--print", "after_move:[F]%(filepath)s");
    }
    args.push("--", job.url);

    job.status = "downloading";
    job.startedAt = new Date().toISOString();
    const child = spawn(ytdlp, args);
    job.child = child;
    const subtitleFiles = [];
    let buf = "";
    child.stdout.on("data", (d) => {
      buf += d;
      let i;
      while ((i = buf.indexOf("\n")) >= 0) {
        const line = buf.slice(0, i).trim();
        buf = buf.slice(i + 1);
        if (line.startsWith("[P]")) {
          const [pct, title] = line.slice(3).split("|");
          job.progress = pct.trim();
          job.current = title;
        } else if (line.startsWith("[F]")) job.files.push(line.slice(3));
        else if (line.startsWith("[info] Writing video subtitles to: "))
          subtitleFiles.push(line.slice("[info] Writing video subtitles to: ".length));
      }
    });
    let stderr = "";
    child.stderr.on("data", (d) => (stderr = (stderr + d).slice(-8000)));
    child.on("close", async (code) => {
      job.child = undefined;
      job.finishedAt = new Date().toISOString();
      if (job.status === "canceled") return resolve();
      if (job.opts.mode === "subtitles") job.files = await finishSubtitles(subtitleFiles, job.opts.subFormat);
      if (code === 0 && job.files.length > 0) job.status = "done";
      else if (code === 0 && job.opts.mode === "subtitles") {
        job.status = "error";
        job.error = "Нет субтитров на выбранных языках";
      } else if (code === 0) job.status = "done";
      else {
        job.status = job.files.length > 0 ? "partial" : "error";
        job.error = errorLine(stderr);
      }
      resolve();
    });
  });
}

function publicJob(job) {
  const { child, opts, ...rest } = job;
  return { ...rest, mode: opts.mode };
}

// ---------------------------------------------------------------------------
// MCP

const server = new McpServer({ name: "yt-downloader", version: "0.1.0" });
const text = (value) => ({ content: [{ type: "text", text: typeof value === "string" ? value : JSON.stringify(value, null, 2) }] });

server.registerTool(
  "analyze",
  {
    title: "Inspect a YouTube link",
    description:
      "Look up a YouTube video, playlist or channel link without downloading: title, channel, duration, available heights for a video, or the list of entries for a playlist/channel.",
    inputSchema: {
      url: z.string().describe("YouTube URL (video, playlist, channel, @handle, or https://www.youtube.com/playlist?list=WL for Watch Later)"),
      limit: z.number().int().positive().max(500).optional().describe("Max playlist entries to return (default 50)"),
    },
  },
  async ({ url, limit = 50 }) => {
    const { code, stdout, stderr } = await runCapture([...baseArgs(), "--dump-single-json", "--flat-playlist", "--", url]);
    if (code !== 0) return { ...text(errorLine(stderr)), isError: true };
    const info = JSON.parse(stdout);
    if (info._type === "playlist" || info.entries) {
      const entries = info.entries ?? [];
      return text({
        kind: "playlist",
        title: info.title,
        channel: info.channel ?? info.uploader,
        count: entries.length,
        entries: entries.slice(0, limit).map((e, i) => ({
          index: i + 1,
          id: e.id,
          title: e.title,
          duration: e.duration,
          url: e.url ?? `https://www.youtube.com/watch?v=${e.id}`,
        })),
      });
    }
    const heights = [...new Set((info.formats ?? []).map((f) => f.height).filter(Boolean))].sort((a, b) => b - a);
    const subs = Object.keys(info.subtitles ?? {});
    const autoOrig = Object.keys(info.automatic_captions ?? {}).filter((l) => l.endsWith("-orig"));
    return text({
      kind: "video",
      id: info.id,
      title: info.title,
      channel: info.channel ?? info.uploader,
      duration: info.duration,
      uploadDate: info.upload_date,
      availableHeights: heights,
      subtitles: subs,
      autoCaptionsOriginal: autoOrig,
      url: info.webpage_url,
    });
  },
);

server.registerTool(
  "download",
  {
    title: "Download from YouTube",
    description:
      "Start downloading a video, playlist or channel as video, audio or subtitles only. Returns a job id immediately; use `status` to follow it. Files go to the folder set in YT Downloader's settings unless outputDir is given.",
    inputSchema: {
      url: z.string().describe("YouTube URL"),
      mode: z.enum(["video", "audio", "subtitles"]).default("video"),
      maxHeight: z.number().int().optional().describe("video: max resolution, e.g. 1080; omit for best"),
      container: z.enum(["mp4", "mkv", "webm"]).optional().describe("video: container (default mp4)"),
      audioFormat: z.enum(["mp3", "m4a", "opus", "flac", "wav"]).optional().describe("audio: format (default mp3)"),
      audioKbps: z.number().int().optional().describe("audio: bitrate in kbps; omit for best"),
      subLangs: z.array(z.string()).optional().describe("subtitles: language codes (default ['ru','en'])"),
      subFormat: z.enum(["srt", "vtt", "txt"]).optional().describe("subtitles: srt, vtt, or txt (plain text transcript). Default srt"),
      autoSubs: z.boolean().optional().describe("subtitles: fall back to auto-generated captions in the video's own language (default true)"),
      playlist: z.boolean().optional().describe("Treat the URL as a playlist/channel and download its entries (default: true for playlist/channel URLs)"),
      items: z.string().optional().describe("Playlist entries to take, yt-dlp syntax, e.g. '1-5,8' or '1:10'"),
      outputDir: z.string().optional().describe("Override the download folder"),
    },
  },
  async (input) => {
    const looksLikeList = /[?&]list=|\/playlist|\/@|\/channel\/|\/c\/|\/user\//.test(input.url) && !/[?&]v=/.test(input.url);
    const job = {
      id: randomUUID().slice(0, 8),
      url: input.url,
      opts: input,
      playlist: input.playlist ?? looksLikeList,
      items: input.items,
      outputDir: input.outputDir,
      status: "queued",
      progress: null,
      current: null,
      files: [],
      error: null,
      createdAt: new Date().toISOString(),
    };
    jobs.set(job.id, job);
    waiting.push(job);
    pump();
    return text({ jobId: job.id, status: job.status, playlist: job.playlist });
  },
);

server.registerTool(
  "status",
  {
    title: "Download status",
    description: "Status of one download job (by id) or of all jobs started in this session: progress, current item, saved files, errors.",
    inputSchema: { jobId: z.string().optional() },
  },
  async ({ jobId }) => {
    if (jobId) {
      const job = jobs.get(jobId);
      return job ? text(publicJob(job)) : { ...text(`Нет задачи ${jobId}`), isError: true };
    }
    return text([...jobs.values()].map(publicJob));
  },
);

server.registerTool(
  "cancel",
  {
    title: "Cancel a download",
    description: "Stop a queued or running download job.",
    inputSchema: { jobId: z.string() },
  },
  async ({ jobId }) => {
    const job = jobs.get(jobId);
    if (!job) return { ...text(`Нет задачи ${jobId}`), isError: true };
    const i = waiting.indexOf(job);
    if (i >= 0) waiting.splice(i, 1);
    job.status = "canceled";
    job.child?.kill();
    return text(publicJob(job));
  },
);

server.registerTool(
  "subscriptions_inbox",
  {
    title: "New videos from subscriptions",
    description:
      "Uploads currently waiting in YT Downloader's Subscriptions inbox (as of the app's last refresh). Read-only; refresh happens in the app.",
    inputSchema: { includeHandled: z.boolean().optional().describe("Also list skipped/queued videos") },
  },
  async ({ includeHandled = false }) => {
    try {
      const state = JSON.parse(await readFile(join(dataDir(), "subscriptions.json"), "utf8"));
      const items = state.items
        .filter((i) => includeHandled || i.state === "new")
        .sort((a, b) => (b.published ?? 0) - (a.published ?? 0))
        .map((i) => ({
          title: i.title,
          channel: i.channelTitle,
          url: i.url,
          duration: i.duration,
          published: i.published ? new Date(i.published * 1000).toISOString().slice(0, 10) : null,
          state: i.state,
        }));
      return text({ lastRefresh: state.lastRefresh ? new Date(state.lastRefresh * 1000).toISOString() : null, items });
    } catch {
      return text("Подписок пока нет — добавьте каналы во вкладке «Подписки» в приложении");
    }
  },
);

await server.connect(new StdioServerTransport());
