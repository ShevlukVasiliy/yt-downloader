import { useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Settings2, X } from "lucide-react";
import { useAnalysisStore } from "../store/useAnalysis";
import { useFormatStore } from "../store/useFormat";
import { useT } from "../i18n";
import { formatDuration, truncate } from "../lib/format";
import type { PlaylistEntry } from "../lib/types";
import { FormatPicker } from "./FormatPicker";

export function PlaylistTable() {
  const t = useT();
  const { analysis, selectedIds, overrides, toggleSelected, selectAll, deselectAll, invertSelection, selectRange, setOverride } =
    useAnalysisStore();
  const commonSpec = useFormatStore((s) => s.spec);
  const [search, setSearch] = useState("");
  const [rangeFrom, setRangeFrom] = useState("");
  const [rangeTo, setRangeTo] = useState("");
  const [overrideTarget, setOverrideTarget] = useState<PlaylistEntry | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const entries = analysis?.kind === "playlist" ? analysis.entries : [];

  const filtered = useMemo(() => {
    if (!search.trim()) return entries;
    const q = search.trim().toLowerCase();
    return entries.filter((e) => e.title.toLowerCase().includes(q));
  }, [entries, search]);

  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 44,
    overscan: 12,
  });

  if (analysis?.kind !== "playlist") return null;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <button onClick={selectAll} className="rounded-md border px-2.5 py-1 text-xs" style={{ borderColor: "var(--border)" }}>
          {t("playlist.selectAll")}
        </button>
        <button onClick={deselectAll} className="rounded-md border px-2.5 py-1 text-xs" style={{ borderColor: "var(--border)" }}>
          {t("playlist.deselectAll")}
        </button>
        <button onClick={invertSelection} className="rounded-md border px-2.5 py-1 text-xs" style={{ borderColor: "var(--border)" }}>
          {t("playlist.invert")}
        </button>

        <div className="flex items-center gap-1 text-xs">
          <input
            value={rangeFrom}
            onChange={(e) => setRangeFrom(e.target.value.replace(/\D/g, ""))}
            placeholder={t("playlist.rangeFrom")}
            className="w-16 rounded-md border bg-transparent px-2 py-1 outline-none"
            style={{ borderColor: "var(--border)" }}
          />
          <input
            value={rangeTo}
            onChange={(e) => setRangeTo(e.target.value.replace(/\D/g, ""))}
            placeholder={t("playlist.rangeTo")}
            className="w-16 rounded-md border bg-transparent px-2 py-1 outline-none"
            style={{ borderColor: "var(--border)" }}
          />
          <button
            onClick={() => {
              const from = parseInt(rangeFrom, 10);
              const to = parseInt(rangeTo, 10);
              if (!isNaN(from) && !isNaN(to)) selectRange(from, to);
            }}
            className="rounded-md border px-2.5 py-1"
            style={{ borderColor: "var(--border)" }}
          >
            {t("playlist.rangeApply")}
          </button>
        </div>

        <input
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={t("playlist.search")}
          className="ml-auto w-48 rounded-md border bg-transparent px-2.5 py-1 text-xs outline-none"
          style={{ borderColor: "var(--border)" }}
        />

        <span className="text-xs font-medium" style={{ color: "var(--text-muted)" }}>
          {t("playlist.selected", { selected: selectedIds.size, total: entries.length })}
        </span>
      </div>

      <div
        ref={parentRef}
        className="h-72 overflow-y-auto rounded-lg border"
        style={{ borderColor: "var(--border)" }}
      >
        {filtered.length === 0 ? (
          <div className="flex h-full items-center justify-center text-sm" style={{ color: "var(--text-faint)" }}>
            {t("playlist.noResults")}
          </div>
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((row) => {
              const entry = filtered[row.index];
              const checked = selectedIds.has(entry.id);
              const hasOverride = !!overrides[entry.id];
              return (
                <div
                  key={entry.id}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: row.size,
                    transform: `translateY(${row.start}px)`,
                  }}
                  className="text-sm"
                >
                  <div
                    className="flex h-full w-full items-center gap-3 border-b px-3"
                    style={{ borderColor: "var(--border)" }}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={() => toggleSelected(entry.id)}
                      className="h-4 w-4 shrink-0 accent-[var(--accent)]"
                    />
                    <span className="w-7 shrink-0 text-right text-xs" style={{ color: "var(--text-faint)" }}>
                      {entry.index}
                    </span>
                    <span className="min-w-0 flex-1 truncate">{truncate(entry.title, 90)}</span>
                    {hasOverride && (
                      <span
                        className="shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium"
                        style={{ background: "var(--accent-muted)", color: "var(--accent)" }}
                      >
                        {t("playlist.customBadge")}
                      </span>
                    )}
                    <span className="w-12 shrink-0 text-right text-xs" style={{ color: "var(--text-muted)" }}>
                      {formatDuration(entry.duration)}
                    </span>
                    <button
                      onClick={() => setOverrideTarget(entry)}
                      className="shrink-0 rounded-md p-1 opacity-50 hover:opacity-100"
                      title={t("playlist.overrideFormat")}
                    >
                      <Settings2 size={14} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {overrideTarget && (
        <OverrideDialog
          entry={overrideTarget}
          baseSpec={{ ...commonSpec, ...overrides[overrideTarget.id] }}
          hasOverride={!!overrides[overrideTarget.id]}
          onClose={() => setOverrideTarget(null)}
          onSave={(spec) => {
            setOverride(overrideTarget.id, spec);
            setOverrideTarget(null);
          }}
          onUseCommon={() => {
            setOverride(overrideTarget.id, null);
            setOverrideTarget(null);
          }}
        />
      )}
    </div>
  );
}

function OverrideDialog({
  entry,
  baseSpec,
  hasOverride,
  onClose,
  onSave,
  onUseCommon,
}: {
  entry: PlaylistEntry;
  baseSpec: import("../lib/types").DownloadSpec;
  hasOverride: boolean;
  onClose: () => void;
  onSave: (spec: import("../lib/types").DownloadSpec) => void;
  onUseCommon: () => void;
}) {
  const t = useT();
  const [spec, setSpec] = useState(baseSpec);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-6"
      style={{ background: "rgba(0,0,0,0.5)" }}
      onClick={onClose}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="max-h-[80vh] w-full max-w-md overflow-y-auto rounded-2xl border p-5 shadow-2xl"
        style={{ background: "var(--bg-elevated)", borderColor: "var(--border)" }}
      >
        <div className="mb-4 flex items-start justify-between gap-3">
          <h3 className="text-sm font-semibold leading-snug">{truncate(entry.title, 70)}</h3>
          <button onClick={onClose} className="shrink-0 rounded-md p-1 opacity-60 hover:opacity-100">
            <X size={16} />
          </button>
        </div>

        <FormatPicker spec={spec} onChange={setSpec} durationSeconds={entry.duration} compact />

        <div className="mt-5 flex items-center justify-between gap-2">
          {hasOverride ? (
            <button onClick={onUseCommon} className="text-xs" style={{ color: "var(--text-muted)" }}>
              {t("playlist.useCommonFormat")}
            </button>
          ) : (
            <span />
          )}
          <button
            onClick={() => onSave(spec)}
            className="rounded-lg px-4 py-2 text-sm font-medium"
            style={{ background: "var(--accent)", color: "white" }}
          >
            {t("common.close")}
          </button>
        </div>
      </div>
    </div>
  );
}
