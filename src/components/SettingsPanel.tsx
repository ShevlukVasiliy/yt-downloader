import { useEffect, useState } from "react";
import { FolderOpen, Loader2, RefreshCw } from "lucide-react";
import { useSettingsStore } from "../store/useSettings";
import { useT } from "../i18n";
import { api } from "../lib/api";
import type { Locale, Theme } from "../lib/types";

const FILENAME_PRESETS = [
  "%(title)s.%(ext)s",
  "%(playlist_index)02d — %(title)s.%(ext)s",
  "%(uploader)s/%(title)s.%(ext)s",
];

const COOKIE_BROWSERS = ["chrome", "safari", "firefox", "edge", "brave"];

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <label className="text-sm font-medium">{label}</label>
      {children}
      {hint && (
        <p className="text-xs" style={{ color: "var(--text-faint)" }}>
          {hint}
        </p>
      )}
    </div>
  );
}

function inputStyle() {
  return { borderColor: "var(--border)", background: "var(--bg)", color: "var(--text)" } as const;
}

export function SettingsPanel() {
  const t = useT();
  const { settings, update } = useSettingsStore();
  const [updateState, setUpdateState] = useState<"idle" | "loading" | "done" | "error">("idle");

  useEffect(() => {
    if (updateState === "done" || updateState === "error") {
      const timer = setTimeout(() => setUpdateState("idle"), 4000);
      return () => clearTimeout(timer);
    }
  }, [updateState]);

  if (!settings) return null;

  const chooseFolder = async () => {
    const dir = await api.pickDownloadDir(settings.downloadDir);
    if (dir) void update({ downloadDir: dir });
  };

  const runUpdate = async () => {
    setUpdateState("loading");
    try {
      await api.updateYtDlp();
      setUpdateState("done");
    } catch {
      setUpdateState("error");
    }
  };

  return (
    <div className="flex max-w-lg flex-col gap-5 pb-8">
      <h2 className="text-base font-semibold">{t("settings.title")}</h2>

      <Field label={t("settings.downloadFolder")}>
        <div className="flex gap-2">
          <input
            readOnly
            value={settings.downloadDir}
            className="min-w-0 flex-1 rounded-lg border px-3 py-2 text-sm outline-none"
            style={inputStyle()}
          />
          <button
            onClick={() => void chooseFolder()}
            className="flex shrink-0 items-center gap-1.5 rounded-lg border px-3 py-2 text-sm"
            style={{ borderColor: "var(--border)" }}
          >
            <FolderOpen size={14} />
            {t("settings.choose")}
          </button>
        </div>
      </Field>

      <Field label={t("settings.filenameTemplate")} hint={t("settings.filenameTemplateHint")}>
        <input
          value={settings.filenameTemplate}
          onChange={(e) => void update({ filenameTemplate: e.target.value })}
          list="filename-presets"
          className="rounded-lg border px-3 py-2 text-sm outline-none"
          style={inputStyle()}
        />
        <datalist id="filename-presets">
          {FILENAME_PRESETS.map((p) => (
            <option key={p} value={p} />
          ))}
        </datalist>
      </Field>

      <Field label={t("settings.concurrency")}>
        <div className="flex items-center gap-3">
          <input
            type="range"
            min={1}
            max={5}
            value={settings.concurrency}
            onChange={(e) => void update({ concurrency: Number(e.target.value) })}
            className="flex-1 accent-[var(--accent)]"
          />
          <span className="w-4 text-sm font-medium">{settings.concurrency}</span>
        </div>
      </Field>

      <Field label={t("settings.rateLimit")} hint={t("settings.rateLimitHint")}>
        <input
          type="number"
          min={0}
          placeholder={t("settings.rateLimitPlaceholder")}
          value={settings.rateLimitKbps ?? ""}
          onChange={(e) => void update({ rateLimitKbps: e.target.value ? Number(e.target.value) : null })}
          className="rounded-lg border px-3 py-2 text-sm outline-none"
          style={inputStyle()}
        />
      </Field>

      <Field label={t("settings.proxy")}>
        <input
          value={settings.proxy ?? ""}
          onChange={(e) => void update({ proxy: e.target.value || null })}
          placeholder={t("settings.proxyPlaceholder")}
          className="rounded-lg border px-3 py-2 text-sm outline-none"
          style={inputStyle()}
        />
      </Field>

      <Field label={t("settings.networkRetries")} hint={t("settings.networkRetriesHint")}>
        <input
          type="number"
          min={0}
          max={50}
          value={settings.networkRetries}
          onChange={(e) => void update({ networkRetries: Math.max(0, Number(e.target.value)) })}
          className="w-24 rounded-lg border px-3 py-2 text-sm outline-none"
          style={inputStyle()}
        />
      </Field>

      <Field label={t("settings.autoRetryAttempts")} hint={t("settings.autoRetryAttemptsHint")}>
        <input
          type="number"
          min={0}
          max={5}
          value={settings.autoRetryAttempts}
          onChange={(e) => void update({ autoRetryAttempts: Math.max(0, Math.min(5, Number(e.target.value))) })}
          className="w-24 rounded-lg border px-3 py-2 text-sm outline-none"
          style={inputStyle()}
        />
      </Field>

      <Field label={t("settings.cookiesFromBrowser")}>
        <select
          value={settings.cookiesFromBrowser ?? ""}
          onChange={(e) => void update({ cookiesFromBrowser: e.target.value || null })}
          className="rounded-lg border px-3 py-2 text-sm outline-none"
          style={inputStyle()}
        >
          <option value="">{t("settings.cookiesNone")}</option>
          {COOKIE_BROWSERS.map((b) => (
            <option key={b} value={b}>
              {b}
            </option>
          ))}
        </select>
      </Field>

      <Field label={t("settings.theme")}>
        <div className="flex gap-1.5">
          {(["dark", "light", "system"] as Theme[]).map((theme) => (
            <button
              key={theme}
              onClick={() => void update({ theme })}
              className="rounded-lg border px-3 py-1.5 text-sm"
              style={{
                borderColor: settings.theme === theme ? "var(--accent)" : "var(--border)",
                color: settings.theme === theme ? "var(--accent)" : "var(--text)",
              }}
            >
              {t(`settings.theme.${theme}`)}
            </button>
          ))}
        </div>
      </Field>

      <Field label={t("settings.language")}>
        <div className="flex gap-1.5">
          {(["ru", "en"] as Locale[]).map((locale) => (
            <button
              key={locale}
              onClick={() => void update({ locale })}
              className="rounded-lg border px-3 py-1.5 text-sm uppercase"
              style={{
                borderColor: settings.locale === locale ? "var(--accent)" : "var(--border)",
                color: settings.locale === locale ? "var(--accent)" : "var(--text)",
              }}
            >
              {locale}
            </button>
          ))}
        </div>
      </Field>

      <Field label={t("settings.dependencies")}>
        <button
          onClick={() => void runUpdate()}
          disabled={updateState === "loading"}
          className="flex w-fit items-center gap-2 rounded-lg border px-3 py-2 text-sm disabled:opacity-60"
          style={{ borderColor: "var(--border)" }}
        >
          {updateState === "loading" ? <Loader2 size={14} className="animate-spin" /> : <RefreshCw size={14} />}
          {updateState === "loading" ? t("settings.updating") : t("settings.updateYtDlp")}
        </button>
        {updateState === "done" && (
          <p className="text-xs" style={{ color: "var(--success)" }}>
            {t("settings.updated")}
          </p>
        )}
        {updateState === "error" && (
          <p className="text-xs" style={{ color: "var(--danger)" }}>
            {t("settings.updateFailed")}
          </p>
        )}
      </Field>
    </div>
  );
}
