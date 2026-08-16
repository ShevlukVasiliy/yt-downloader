import { useMemo, useState } from "react";
import { ChevronDown, ChevronUp, ListVideo, Trash2 } from "lucide-react";
import { useQueueStore } from "../store/useQueue";
import { useT } from "../i18n";
import { EmptyState } from "./EmptyState";
import { JobCard } from "./JobCard";
import type { Job, JobStatus } from "../lib/types";

const HISTORY_STATUSES: JobStatus[] = ["done", "error", "canceled"];

interface Group {
  groupId: string | null;
  groupTitle: string | null;
  jobs: Job[];
}

function groupJobs(jobs: Job[]): Group[] {
  const order: string[] = [];
  const map = new Map<string, Job[]>();
  for (const job of jobs) {
    const key = job.groupId ?? job.id;
    if (!map.has(key)) {
      order.push(key);
      map.set(key, []);
    }
    map.get(key)!.push(job);
  }
  return order.map((key) => {
    const list = map.get(key)!;
    return { groupId: list[0].groupId, groupTitle: list[0].groupTitle, jobs: list };
  });
}

function GroupHeader({ group }: { group: Group }) {
  const t = useT();
  const [open, setOpen] = useState(true);
  const done = group.jobs.filter((j) => j.status === "done").length;
  const percent = group.jobs.length > 0 ? Math.round((done / group.jobs.length) * 100) : 0;

  return (
    <div className="flex flex-col gap-2.5">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-3 rounded-xl border p-3.5 text-left transition-colors"
        style={{ borderColor: "var(--border)", background: "var(--bg-elevated)" }}
      >
        <div
          className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg"
          style={{ background: "var(--accent-muted)" }}
        >
          <ListVideo size={20} style={{ color: "var(--accent)" }} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[15px] font-semibold">{group.groupTitle}</div>
          <div className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full" style={{ background: "var(--bg-elevated-2)" }}>
            <div
              className="h-full rounded-full transition-all"
              style={{ width: `${percent}%`, background: "var(--accent)" }}
            />
          </div>
        </div>
        <span className="shrink-0 text-sm font-semibold" style={{ color: "var(--text-muted)" }}>
          {t("queue.groupOf", { done, total: group.jobs.length })}
        </span>
        {open ? (
          <ChevronUp size={20} className="shrink-0" style={{ color: "var(--text-faint)" }} />
        ) : (
          <ChevronDown size={20} className="shrink-0" style={{ color: "var(--text-faint)" }} />
        )}
      </button>
      {open && (
        <div className="flex flex-col gap-2.5 pl-2">
          {group.jobs.map((job) => (
            <JobCard key={job.id} job={job} />
          ))}
        </div>
      )}
    </div>
  );
}

export function QueueList({ mode }: { mode: "active" | "history" }) {
  const t = useT();
  const jobs = useQueueStore((s) => s.jobs);
  const clearFinished = useQueueStore((s) => s.clearFinished);

  const filtered = useMemo(
    () =>
      jobs.filter((j) =>
        mode === "history" ? HISTORY_STATUSES.includes(j.status) : !HISTORY_STATUSES.includes(j.status),
      ),
    [jobs, mode],
  );

  const groups = useMemo(() => groupJobs(filtered), [filtered]);

  if (filtered.length === 0) {
    return <EmptyState label={mode === "history" ? t("queue.historyEmpty") : t("queue.empty")} />;
  }

  return (
    <div className="flex flex-col gap-3">
      {mode === "history" && (
        <button
          onClick={() => void clearFinished()}
          className="ml-auto flex items-center gap-1.5 self-end text-xs"
          style={{ color: "var(--text-muted)" }}
        >
          <Trash2 size={12} />
          {t("queue.clearFinished")}
        </button>
      )}
      {groups.map((group) =>
        group.groupId ? (
          <GroupHeader key={group.groupId} group={group} />
        ) : (
          <JobCard key={group.jobs[0].id} job={group.jobs[0]} />
        ),
      )}
    </div>
  );
}
