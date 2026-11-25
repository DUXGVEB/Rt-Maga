<script lang="ts">
  import SourceManagerPanel from "../lib/components/SourceManagerPanel.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";

  let { initialQuery = "" }: { initialQuery?: string } = $props();
  const t = $derived(settingsStore.t);
</script>

<div class="mx-auto flex w-full max-w-5xl flex-col gap-6 px-5 py-6 sm:px-8 sm:py-8">
  <div>
    <h1 class="text-2xl font-bold tracking-tight">{t.sourcesPageTitle}</h1>
    <p class="mt-1 text-xs" style="color: var(--text-muted);">{t.sourcesPageSubtitle}</p>
  </div>

  <SourceManagerPanel scope="main" {initialQuery} />
</div>
