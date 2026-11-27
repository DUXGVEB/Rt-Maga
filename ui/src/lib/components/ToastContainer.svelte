<script lang="ts">
  import { X } from "lucide-svelte";
  import { flip } from "svelte/animate";
  import { flipDuration, toastSlide } from "../motion";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
</script>

{#if toasts.items.length > 0}
  <div
    class="pointer-events-none fixed right-3 bottom-20 left-3 z-50 flex flex-col items-end gap-2 sm:right-5 sm:left-auto md:bottom-5"
    style="padding-bottom: var(--safe-bottom); padding-left: var(--safe-left); padding-right: var(--safe-right);"
  >
    {#each toasts.items as toast (toast.id)}
      <div
        animate:flip={{ duration: flipDuration(190) }}
        transition:toastSlide|local
        class="glass-chrome pointer-events-auto flex max-w-sm items-center gap-3 rounded-2xl border px-4 py-2.5 text-sm"
        style="border-color: {toast.kind === 'error'
          ? 'var(--danger)'
          : toast.kind === 'success'
            ? 'var(--accent)'
            : 'var(--border-strong)'}; color: var(--text); box-shadow: var(--shadow-md);"
      >
        <span class="flex-1 leading-snug">{toast.message}</span>
        <button
          type="button"
          onclick={() => toasts.dismiss(toast.id)}
          class="gpu-press flex h-7 w-7 shrink-0 items-center justify-center rounded-lg opacity-65 hover:opacity-100"
          aria-label={settingsStore.t.dismissNotification}
        >
          <X size={14} />
        </button>
      </div>
    {/each}
  </div>
{/if}
