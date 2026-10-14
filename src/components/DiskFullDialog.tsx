import { HardDrive } from "lucide-react";
import { useT } from "../i18n";
import { useQueueStore } from "../store/useQueue";
import { api } from "../lib/api";
import { formatBytes } from "../lib/format";

export function DiskFullDialog() {
  const t = useT();
  const { diskFull, dismissDiskFull, resumeAfterDiskFull } = useQueueStore();
  if (!diskFull) return null;

  const rows: [string, string][] = [
    [t("diskFull.folder"), diskFull.dir],
    [t("diskFull.free"), formatBytes(diskFull.freeBytes)],
  ];
  if (diskFull.neededBytes) rows.push([t("diskFull.needed"), formatBytes(diskFull.neededBytes)]);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-6"
      style={{ background: "rgba(0,0,0,0.5)" }}
      onClick={dismissDiskFull}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        className="w-full max-w-md rounded-2xl border p-6 shadow-2xl"
        style={{ background: "var(--bg-elevated)", borderColor: "var(--border)" }}
      >
        <div className="flex items-center gap-2.5">
          <HardDrive size={20} style={{ color: "var(--danger)" }} />
          <h2 className="text-base font-semibold">{t("diskFull.title")}</h2>
        </div>
        <p className="mt-2 text-sm" style={{ color: "var(--text-muted)" }}>
          {t("diskFull.body")}
        </p>
        <dl className="mt-4 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-sm">
          {rows.map(([label, value]) => (
            <div key={label} className="contents">
              <dt style={{ color: "var(--text-faint)" }}>{label}</dt>
              <dd className="truncate" title={value}>{value}</dd>
            </div>
          ))}
        </dl>
        <div className="mt-5 flex justify-end gap-2">
          <button onClick={dismissDiskFull} className="rounded-lg px-3 py-2 text-sm" style={{ color: "var(--text-muted)" }}>
            {t("common.close")}
          </button>
          <button
            onClick={() => void api.revealInFolder(diskFull.dir)}
            className="rounded-lg border px-3 py-2 text-sm font-medium"
            style={{ borderColor: "var(--border)" }}
          >
            {t("diskFull.openFolder")}
          </button>
          <button
            onClick={() => void resumeAfterDiskFull()}
            className="rounded-lg px-3 py-2 text-sm font-semibold"
            style={{ background: "var(--accent)", color: "white" }}
          >
            {t("diskFull.retry")}
          </button>
        </div>
      </div>
    </div>
  );
}
