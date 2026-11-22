import { api } from "../api";
import type { FoundUpdate, UpdateProgress } from "../types";
import { toasts } from "./toast.svelte";

class UpdatesStore {
  newCount = $state(0);
  checking = $state(false);
  progress = $state<UpdateProgress | null>(null);
  lastFound = $state<FoundUpdate[]>([]);
  private initialized = false;

  async init() {
    if (this.initialized) return;
    this.initialized = true;
    try {
      await api.onUpdatesProgress((p) => {
        this.progress = p;
        this.checking = p.done < p.total;
      });
      await api.onUpdatesFound((found) => {
        this.lastFound = found;
        this.newCount += found.length;
        if (found.length === 1) {
          toasts.push(`New chapter: ${found[0].title} (${found[0].latest})`, "success", 5000);
        } else if (found.length > 1) {
          toasts.push(`${found.length} manga have new chapters`, "success", 5000);
        }
      });
    } catch {
      // Outside Tauri runtime.
    }
  }

  setNewCountFromHistory(count: number) {
    this.newCount = count;
  }

  async checkNow() {
    if (this.checking) return;
    this.checking = true;
    try {
      const summary = await api.checkUpdatesNow();
      if (summary.found.length === 0) {
        toasts.push(
          summary.checked === 0
            ? "No manga in history to check yet."
            : `Checked ${summary.checked} manga - no new chapters yet.`,
          "info"
        );
      }
    } catch {
      toasts.push("Failed to check for updates right now.", "error");
    } finally {
      this.checking = false;
      this.progress = null;
    }
  }
}

export const updatesStore = new UpdatesStore();
