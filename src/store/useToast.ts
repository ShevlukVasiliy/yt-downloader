import { create } from "zustand";

export interface Toast {
  id: string;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}

interface ToastState {
  toasts: Toast[];
  push: (toast: Omit<Toast, "id">) => void;
  dismiss: (id: string) => void;
}

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  push: (toast) => {
    const id = crypto.randomUUID();
    set((s) => ({ toasts: [...s.toasts, { ...toast, id }] }));
    setTimeout(() => {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    }, 6000);
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
}));
