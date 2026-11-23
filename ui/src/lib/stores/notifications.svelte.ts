import { api } from "../api";
import type { NotificationItemView } from "../types";

class NotificationsStore {
  items = $state<NotificationItemView[]>([]);
  drawerOpen = $state(false);
  loading = $state(false);

  unreadCount = $derived(this.items.filter((n) => !n.read).length);

  private initialized = false;

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    await this.refresh();
    try {
      await api.onNotificationsUpdated(() => {
        void this.refresh();
      });
    } catch {
      // Outside Tauri runtime.
    }
  }

  async refresh() {
    this.loading = true;
    try {
      this.items = await api.listNotifications();
    } catch {
      // Ignore outside Tauri runtime.
    } finally {
      this.loading = false;
    }
  }

  openDrawer() {
    this.drawerOpen = true;
    void this.refresh();
  }

  closeDrawer() {
    this.drawerOpen = false;
  }

  toggleDrawer() {
    if (this.drawerOpen) {
      this.closeDrawer();
    } else {
      this.openDrawer();
    }
  }

  async markRead(id: number) {
    this.items = this.items.map((item) => (item.id === id ? { ...item, read: true } : item));
    try {
      await api.markNotificationRead(id);
    } catch {
      // Best-effort.
    }
  }

  async markAllRead() {
    this.items = this.items.map((item) => ({ ...item, read: true }));
    try {
      await api.markAllNotificationsRead();
    } catch {
      // Best-effort.
    }
  }

  async remove(id: number) {
    this.items = this.items.filter((item) => item.id !== id);
    try {
      await api.deleteNotification(id);
    } catch {
      // Best-effort.
    }
  }

  async clearAll() {
    this.items = [];
    try {
      await api.clearNotifications();
    } catch {
      // Best-effort.
    }
  }
}

export const notificationsStore = new NotificationsStore();
