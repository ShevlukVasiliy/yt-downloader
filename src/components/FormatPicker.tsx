import { useState } from "react";
import { ChevronDown, ChevronUp } from "lucide-react";
import type { AudioFormat, DownloadSpec, VideoContainer } from "../lib/types";
import { useT } from "../i18n";
import { formatBytes } from "../lib/format";
import { estimateBytesPerVideo } from "../lib/estimate";

const HEIGHT_CHIPS = [2160, 1440, 1080, 720, 480, 360];
const CONTAINERS: VideoContainer[] = ["mp4", "mkv", "webm"];
const AUDIO_FORMATS: AudioFormat[] = ["mp3", "m4a", "opus", "flac", "wav"];
const AUDIO_QUALITIES: { label: string; kbps: number | null }[] = [
  { label: "320", kbps: 320 },
  { label: "256", kbps: 256 },
  { label: "192", kbps: 192 },
  { label: "128", kbps: 128 },
];

function Chip({
  active,
  disabled,
  onClick,
  children,
}: {
  active: boolean;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className="rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-35"
      style={{
        borderColor: active ? "var(--accent)" : "var(--border)",
        background: active ? "var(--accent-muted)" : "transparent",
        color: active ? "var(--accent)" : "var(--text)",
      }}
    >
      {children}
    </button>
  );
}

function Checkbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
}) {
  return (
    <label className="flex cursor-pointer items-center gap-2 text-sm">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="h-4 w-4 accent-[var(--accent)]"
      />
      {label}
    </label>
  );
}

export interface FormatPickerProps {
  spec: DownloadSpec;
  onChange: (spec: DownloadSpec) => void;
  availableHeights?: number[];
  durationSeconds?: number | null;
  compact?: boolean;
}

export function FormatPicker({ spec, onChange, availableHeights = [], durationSeconds, compact }: FormatPickerProps) {
  const t = useT();
  const [extrasOpen, setExtrasOpen] = useState(!compact);

  const estimate = estimateBytesPerVideo(spec, durationSeconds, availableHeights);

  return (
    <div className="flex flex-col gap-4">
      <div className="inline-flex w-fit rounded-lg border p-0.5" style={{ borderColor: "var(--border)" }}>
        {(["video", "audio"] as const).map((mode) => (
          <button
            key={mode}
            onClick={() => onChange({ ...spec, mode })}
            className="rounded-md px-4 py-1.5 text-sm font-medium transition-colors"
            style={{
              background: spec.mode === mode ? "var(--accent)" : "transparent",
              color: spec.mode === mode ? "white" : "var(--text-muted)",
            }}
          >
            {t(mode === "video" ? "format.video" : "format.audio")}
          </button>
        ))}
      </div>

      {spec.mode === "video" ? (
        <>
          <div>
            <div className="mb-1.5 text-xs font-medium" style={{ color: "var(--text-muted)" }}>
              {t("format.quality")}
            </div>
            <div className="flex flex-wrap gap-1.5">
              <Chip active={spec.maxHeight === null} onClick={() => onChange({ ...spec, maxHeight: null })}>
                {t("format.best")}
              </Chip>
              {HEIGHT_CHIPS.map((h) => (
                <Chip
                  key={h}
                  active={spec.maxHeight === h}
                  disabled={availableHeights.length > 0 && !availableHeights.some((a) => a >= h)}
                  onClick={() => onChange({ ...spec, maxHeight: h })}
                >
                  {h}p
                </Chip>
              ))}
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-4">
            <div>
              <div className="mb-1.5 text-xs font-medium" style={{ color: "var(--text-muted)" }}>
                {t("format.container")}
              </div>
              <div className="flex gap-1.5">
                {CONTAINERS.map((c) => (
                  <Chip key={c} active={spec.container === c} onClick={() => onChange({ ...spec, container: c })}>
                    {c}
                  </Chip>
                ))}
              </div>
            </div>
            <Checkbox
              checked={spec.videoOnly}
              onChange={(v) => onChange({ ...spec, videoOnly: v })}
              label={t("format.noAudio")}
            />
          </div>
        </>
      ) : (
        <>
          <div>
            <div className="mb-1.5 text-xs font-medium" style={{ color: "var(--text-muted)" }}>
              {t("format.audioFormat")}
            </div>
            <div className="flex flex-wrap gap-1.5">
              {AUDIO_FORMATS.map((f) => (
                <Chip key={f} active={spec.audioFormat === f} onClick={() => onChange({ ...spec, audioFormat: f })}>
                  {f}
                </Chip>
              ))}
            </div>
          </div>
          <div>
            <div className="mb-1.5 text-xs font-medium" style={{ color: "var(--text-muted)" }}>
              {t("format.audioQuality")}
            </div>
            <div className="flex flex-wrap gap-1.5">
              <Chip
                active={spec.audioQuality.kind === "best"}
                onClick={() => onChange({ ...spec, audioQuality: { kind: "best" } })}
              >
                {t("format.best")}
              </Chip>
              {AUDIO_QUALITIES.map((q) => (
                <Chip
                  key={q.label}
                  active={spec.audioQuality.kind === "kbps" && spec.audioQuality.value === q.kbps}
                  onClick={() => onChange({ ...spec, audioQuality: { kind: "kbps", value: q.kbps! } })}
                >
                  {q.label}
                </Chip>
              ))}
            </div>
          </div>
        </>
      )}

      <div>
        <button
          type="button"
          onClick={() => setExtrasOpen((v) => !v)}
          className="flex items-center gap-1 text-xs font-medium"
          style={{ color: "var(--text-muted)" }}
        >
          {t("format.extrasToggle")}
          {extrasOpen ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
        </button>

        {extrasOpen && (
          <div className="mt-3 flex flex-col gap-2.5 rounded-lg border p-3.5" style={{ borderColor: "var(--border)" }}>
            <Checkbox
              checked={spec.extras.embedThumbnail}
              onChange={(v) => onChange({ ...spec, extras: { ...spec.extras, embedThumbnail: v } })}
              label={t("format.embedThumbnail")}
            />
            <Checkbox
              checked={spec.extras.embedMetadata}
              onChange={(v) => onChange({ ...spec, extras: { ...spec.extras, embedMetadata: v } })}
              label={t("format.embedMetadata")}
            />
            <Checkbox
              checked={spec.extras.embedChapters}
              onChange={(v) => onChange({ ...spec, extras: { ...spec.extras, embedChapters: v } })}
              label={t("format.embedChapters")}
            />
            <Checkbox
              checked={spec.extras.sponsorblock}
              onChange={(v) => onChange({ ...spec, extras: { ...spec.extras, sponsorblock: v } })}
              label={t("format.sponsorblock")}
            />
            <Checkbox
              checked={spec.extras.writeInfoJson}
              onChange={(v) => onChange({ ...spec, extras: { ...spec.extras, writeInfoJson: v } })}
              label={t("format.writeInfoJson")}
            />

            {spec.mode === "video" && (
              <div className="mt-1 border-t pt-2.5" style={{ borderColor: "var(--border)" }}>
                <Checkbox
                  checked={spec.extras.subtitles.enabled}
                  onChange={(v) =>
                    onChange({ ...spec, extras: { ...spec.extras, subtitles: { ...spec.extras.subtitles, enabled: v } } })
                  }
                  label={t("format.subtitles")}
                />
                {spec.extras.subtitles.enabled && (
                  <div className="ml-6 mt-2 flex flex-col gap-2">
                    <div className="flex gap-1.5">
                      {["ru", "en"].map((lang) => {
                        const active = spec.extras.subtitles.langs.includes(lang);
                        return (
                          <Chip
                            key={lang}
                            active={active}
                            onClick={() => {
                              const langs = active
                                ? spec.extras.subtitles.langs.filter((l) => l !== lang)
                                : [...spec.extras.subtitles.langs, lang];
                              onChange({
                                ...spec,
                                extras: { ...spec.extras, subtitles: { ...spec.extras.subtitles, langs } },
                              });
                            }}
                          >
                            {lang}
                          </Chip>
                        );
                      })}
                    </div>
                    <Checkbox
                      checked={spec.extras.subtitles.auto}
                      onChange={(v) =>
                        onChange({
                          ...spec,
                          extras: { ...spec.extras, subtitles: { ...spec.extras.subtitles, auto: v } },
                        })
                      }
                      label={t("format.subtitlesAuto")}
                    />
                    <Checkbox
                      checked={spec.extras.subtitles.embed}
                      onChange={(v) =>
                        onChange({
                          ...spec,
                          extras: { ...spec.extras, subtitles: { ...spec.extras.subtitles, embed: v } },
                        })
                      }
                      label={t("format.subtitlesEmbed")}
                    />
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {estimate != null && (
        <p className="text-xs" style={{ color: "var(--text-faint)" }}>
          {t("format.estimatedSize", { size: formatBytes(estimate) })}
        </p>
      )}
    </div>
  );
}
