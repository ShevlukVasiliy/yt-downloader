import { Inbox } from "lucide-react";

export function EmptyState({ label }: { label: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 py-12 text-center" style={{ color: "var(--text-faint)" }}>
      <Inbox size={28} strokeWidth={1.5} />
      <p className="text-sm">{label}</p>
    </div>
  );
}
