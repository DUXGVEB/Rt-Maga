export interface ToastItem {
  id: number;
  message: string;
  kind: "info" | "success" | "error";
}

class ToastStore {
  items = $state<ToastItem[]>([]);
  private nextId = 1;

  push(message: string, kind: "info" | "success" | "error" = "info", durationMs = 3600) {
    const id = this.nextId++;
    this.items = [...this.items, { id, message, kind }];
    window.setTimeout(() => {
      this.dismiss(id);
    }, durationMs);
  }

  dismiss(id: number) {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toasts = new ToastStore();
