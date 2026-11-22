<script lang="ts">
  import { RefreshCw, Trash2 } from "lucide-svelte";
  import { api } from "../lib/api";
  import { LANGUAGE_OPTIONS } from "../lib/i18n";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { CacheInfo, Language, ReaderBg, Settings, Theme } from "../lib/types";

  let cache = $state<CacheInfo | null>(null);
  let clearingCache = $state(false);

  const s = $derived(settingsStore.value);
  const t = $derived(settingsStore.t);

  async function loadCacheInfo() {
    try {
      cache = await api.cacheInfo();
    } catch {
      cache = null;
    }
  }

  $effect(() => {
    loadCacheInfo();
  });

  function patch(mutator: (draft: Settings) => void, debounceMs = 0) {
    settingsStore.update(mutator, debounceMs);
  }

  async function clearCache() {
    clearingCache = true;
    try {
      cache = await api.clearImageCache();
      toasts.push(t.toastCacheCleared, "success");
    } catch {
      toasts.push(t.toastCacheClearFailed, "error");
    } finally {
      clearingCache = false;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<div class="mx-auto flex w-full max-w-3xl flex-col gap-6 px-8 py-8">
  <div>
    <h1 class="text-2xl font-bold tracking-tight">{t.settingsTitle}</h1>
    <p class="mt-1 text-xs" style="color: var(--text-muted);">
      {t.settingsSubtitle}
    </p>
  </div>

  <!-- Appearance -->
  <section class="settings-card">
    <h2 class="section-title">{t.appearanceSection}</h2>

    <div class="row">
      <div>
        <div class="label">{t.languageLabel}</div>
        <div class="desc">{t.languageDesc}</div>
      </div>
      <select
        value={s.language}
        onchange={(e) => {
          const lang = e.currentTarget.value as Language;
          patch((d) => (d.language = lang));
        }}
        class="rounded-lg border px-3 py-1.5 text-xs font-medium"
        style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
      >
        {#each LANGUAGE_OPTIONS as opt (opt.code)}
          <option value={opt.code}>{opt.label}</option>
        {/each}
      </select>
    </div>

    <div class="row">
      <div>
        <div class="label">{t.colorThemeLabel}</div>
        <div class="desc">{t.colorThemeDesc}</div>
      </div>
      <div class="flex gap-1.5">
        {#each [["system", t.themeSystem], ["light", t.themeLight], ["dark", t.themeDark]] as [val, label] (val)}
          <button
            type="button"
            onclick={() => patch((d) => (d.theme = val as Theme))}
            class="pill"
            class:pill-active={s.theme === val}
          >
            {label}
          </button>
        {/each}
      </div>
    </div>
  </section>

  <!-- Reader -->
  <section class="settings-card">
    <h2 class="section-title">{t.readerSection}</h2>

    <div class="row">
      <div>
        <div class="label">{t.widthModeLabel}</div>
        <div class="desc">{t.widthModeDesc}</div>
      </div>
      <div class="flex gap-1.5">
        <button
          type="button"
          onclick={() => patch((d) => (d.reader.width = { mode: "px", value: 800 }))}
          class="pill"
          class:pill-active={s.reader.width.mode === "px"}
        >
          {t.widthPixels}
        </button>
        <button
          type="button"
          onclick={() => patch((d) => (d.reader.width = { mode: "percent", value: 70 }))}
          class="pill"
          class:pill-active={s.reader.width.mode === "percent"}
        >
          {t.widthPercent}
        </button>
        <button
          type="button"
          onclick={() => patch((d) => (d.reader.width = { mode: "fit_window" }))}
          class="pill"
          class:pill-active={s.reader.width.mode === "fit_window"}
        >
          {t.fitWindow}
        </button>
      </div>
    </div>

    {#if s.reader.width.mode === "px"}
      <div class="row">
        <div class="label">{t.widthLabelPx(s.reader.width.value)}</div>
        <input
          type="range"
          min="480"
          max="1400"
          step="20"
          value={s.reader.width.value}
          oninput={(e) => {
            const v = Number(e.currentTarget.value);
            patch((d) => (d.reader.width = { mode: "px", value: v }), 150);
          }}
          class="w-48 accent-rose-500"
        />
      </div>
    {:else if s.reader.width.mode === "percent"}
      <div class="row">
        <div class="label">{t.widthLabelPct(s.reader.width.value)}</div>
        <input
          type="range"
          min="35"
          max="100"
          step="5"
          value={s.reader.width.value}
          oninput={(e) => {
            const v = Number(e.currentTarget.value);
            patch((d) => (d.reader.width = { mode: "percent", value: v }), 150);
          }}
          class="w-48 accent-rose-500"
        />
      </div>
    {/if}

    <div class="row">
      <div>
        <div class="label">{t.gapLabel(s.reader.gapPx)}</div>
        <div class="desc">{t.gapDesc}</div>
      </div>
      <input
        type="range"
        min="0"
        max="32"
        step="2"
        value={s.reader.gapPx}
        oninput={(e) => {
          const v = Number(e.currentTarget.value);
          patch((d) => (d.reader.gapPx = v), 150);
        }}
        class="w-48 accent-rose-500"
      />
    </div>

    <div class="row">
      <div>
        <div class="label">{t.preloadLabel(s.reader.preloadImages)}</div>
        <div class="desc">{t.preloadDesc}</div>
      </div>
      <input
        type="range"
        min="1"
        max="12"
        step="1"
        value={s.reader.preloadImages}
        oninput={(e) => {
          const v = Number(e.currentTarget.value);
          patch((d) => (d.reader.preloadImages = v), 150);
        }}
        class="w-48 accent-rose-500"
      />
    </div>

    <div class="row">
      <div>
        <div class="label">{t.autoAppendNext}</div>
        <div class="desc">{t.autoAppendDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.reader.autoNextChapter}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.reader.autoNextChapter = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>

    <div class="row">
      <div class="label">{t.readerBg}</div>
      <div class="flex gap-1.5">
        {#each [["dark", t.bgDark], ["light", t.bgLight], ["follow_theme", t.bgAppTheme]] as [val, label] (val)}
          <button
            type="button"
            onclick={() => patch((d) => (d.reader.readerBg = val as ReaderBg))}
            class="pill"
            class:pill-active={s.reader.readerBg === val}
          >
            {label}
          </button>
        {/each}
      </div>
    </div>
  </section>

  <!-- Updates -->
  <section class="settings-card">
    <div class="flex items-center justify-between">
      <h2 class="section-title">{t.updatesSection}</h2>
      <button
        type="button"
        onclick={() => updatesStore.checkNow()}
        disabled={updatesStore.checking}
        class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface-subtle);"
      >
        <RefreshCw size={13} class={updatesStore.checking ? "animate-spin" : ""} />
        <span>{t.checkNow}</span>
      </button>
    </div>

    <div class="row">
      <div>
        <div class="label">{t.autoCheckLabel}</div>
        <div class="desc">{t.autoCheckDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.updates.enabled}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.updates.enabled = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>

    <div class="row">
      <div class="label">{t.checkIntervalLabel}</div>
      <select
        value={s.updates.intervalHours}
        onchange={(e) => {
          const v = Number(e.currentTarget.value);
          patch((d) => (d.updates.intervalHours = v));
        }}
        class="rounded-lg border px-3 py-1.5 text-xs"
        style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
      >
        {#each [1, 3, 6, 12, 24] as hrs (hrs)}
          <option value={hrs}>{t.everyHours(hrs)}</option>
        {/each}
      </select>
    </div>

    <div class="row">
      <div>
        <div class="label">{t.notificationsLabel}</div>
        <div class="desc">{t.notificationsDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.updates.notifyWindows}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.updates.notifyWindows = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>
  </section>

  <!-- Scraper / Sources -->
  <section class="settings-card">
    <h2 class="section-title">{t.sourcesSection}</h2>

    <div class="row">
      <div>
        <div class="label">{t.headlessLabel}</div>
        <div class="desc">{t.headlessDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.sources.headlessEnabled}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.sources.headlessEnabled = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>
  </section>

  <!-- Cache -->
  <section class="settings-card">
    <h2 class="section-title">{t.cacheSection}</h2>

    <div class="row">
      <div>
        <div class="label">{t.cacheUsageLabel}</div>
        <div class="desc">
          {#if cache}
            {t.cacheUsageDesc(formatBytes(cache.bytes), cache.files)}
          {:else}
            {t.calculating}
          {/if}
        </div>
      </div>
      <button
        type="button"
        onclick={clearCache}
        disabled={clearingCache}
        class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
        style="border-color: var(--border); color: var(--danger);"
      >
        <Trash2 size={14} />
        <span>{clearingCache ? t.clearing : t.clearCache}</span>
      </button>
    </div>

    <div class="row">
      <div class="label">{t.maxCacheLabel(s.cache.maxMb)}</div>
      <input
        type="range"
        min="256"
        max="4096"
        step="256"
        value={s.cache.maxMb}
        oninput={(e) => {
          const v = Number(e.currentTarget.value);
          patch((d) => (d.cache.maxMb = v), 150);
        }}
        class="w-48 accent-rose-500"
      />
    </div>
  </section>
</div>

<style>
  .settings-card {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.25rem;
    border-radius: 1rem;
    border: 1px solid var(--border);
    background-color: var(--surface);
  }

  .section-title {
    font-size: 0.9rem;
    font-weight: 600;
    margin: 0;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }

  .label {
    font-size: 0.875rem;
    font-weight: 500;
  }

  .desc {
    font-size: 0.75rem;
    color: var(--text-muted);
    margin-top: 0.15rem;
  }

  .pill {
    padding: 0.35rem 0.75rem;
    border-radius: 0.5rem;
    border: 1px solid var(--border);
    background-color: var(--surface-subtle);
    color: var(--text);
    font-size: 0.75rem;
    font-weight: 500;
    cursor: pointer;
  }

  .pill-active {
    border-color: var(--accent);
    background-color: var(--accent-soft);
    color: var(--accent);
  }
</style>
