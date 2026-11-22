<script lang="ts">
  import { X } from "lucide-svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { toasts } from "../stores/toast.svelte";
</script>

{#if toasts.items.length > 0}
  <div class="pointer-events-none fixed right-5 bottom-5 z-50 flex flex-col gap-2">
    {#each toasts.items as toast (toast.id)}
      <div
        class="pointer-events-auto flex max-w-sm items-center gap-3 rounded-xl border px-4 py-2.5 text-sm shadow-lg"
        style="background-color: var(--surface); border-color: {toast.kind === 'error'
          ? 'var(--danger)'
          : toast.kind === 'success'
            ? 'var(--accent)'
            : 'var(--border)'}; color: var(--text);"
      >
        <span class="flex-1 leading-snug">{toast.message}</span>
        <button
          type="button"
          onclick={() => toasts.dismiss(toast.id)}
          class="opacity-60 transition-opacity hover:opacity-100"
          aria-label={settingsStore.t.dismissNotification}
        >
          <X size={14} />
        </button>
      </div>
    {/each}
  </div>
{/if}
