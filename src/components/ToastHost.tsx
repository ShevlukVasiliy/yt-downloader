import { CheckCircle2, X } from "lucide-react";
import { useToastStore } from "../store/useToast";

export function ToastHost() {
  const { toasts, dismiss } = useToastStore();

  if (toasts.length === 0) return null;

  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-50 flex flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className="pointer-events-auto flex items-center gap-2.5 rounded-xl border px-3.5 py-2.5 shadow-xl"
          style={{ background: "var(--bg-elevated)", borderColor: "var(--border)" }}
        >
          <CheckCircle2 size={16} style={{ color: "var(--success)" }} className="shrink-0" />
          <span className="max-w-64 truncate text-sm">{toast.message}</span>
          {toast.actionLabel && toast.onAction && (
            <button
              onClick={() => {
                toast.onAction?.();
                dismiss(toast.id);
              }}
              className="shrink-0 text-sm font-medium"
              style={{ color: "var(--accent)" }}
            >
              {toast.actionLabel}
            </button>
          )}
          <button onClick={() => dismiss(toast.id)} className="shrink-0 opacity-50 hover:opacity-100">
            <X size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
