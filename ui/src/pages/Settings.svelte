<script lang="ts">
  import { Download, RefreshCw, Sparkles, Trash2 } from "lucide-svelte";
  import { api } from "../lib/api";
  import LanSyncPanel from "../lib/components/LanSyncPanel.svelte";
  import ProxySettingsPanel from "../lib/components/ProxySettingsPanel.svelte";
  import { LANGUAGE_OPTIONS } from "../lib/i18n";
  import { appReleaseStore } from "../lib/stores/appRelease.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type {
    CacheInfo,
    GroupAlignment,
    HorizontalDirection,
    HorizontalSnapMode,
    Language,
    ReaderBg,
    ReadingOrientation,
    Settings,
    Theme,
    VerticalLayoutMode,
  } from "../lib/types";

  let cache = $state<CacheInfo | null>(null);
  let clearingCache = $state(false);

  const s = $derived(settingsStore.value);
  const t = $derived(settingsStore.t);
  const isHorizontal = $derived(s.reader.orientation === "horizontal");
  const isVerticalGrouped = $derived(
    !isHorizontal && s.reader.vertical.layoutMode === "vertical_grouped"
  );

  async function loadCacheInfo() {
    try {
      cache = await api.cacheInfo();
    } catch {
      cache = null;
    }
  }

  $effect(() => {
    void s.cache.maxMb;
    const timer = window.setTimeout(() => {
      void loadCacheInfo();
    }, 300);
    return () => window.clearTimeout(timer);
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

    <!-- Orientation -->
    <div class="row">
      <div>
        <div class="label">{t.readingModeLabel}</div>
        <div class="desc">{t.readingModeDesc}</div>
      </div>
      <div class="flex gap-1.5">
        {#each [["vertical", t.modeVertical], ["horizontal", t.modeHorizontal]] as [val, label] (val)}
          <button
            type="button"
            onclick={() => patch((d) => (d.reader.orientation = val as ReadingOrientation))}
            class="pill"
            class:pill-active={s.reader.orientation === val}
          >
            {label}
          </button>
        {/each}
      </div>
    </div>

    {#if !isHorizontal}
      <!-- Vertical layout mode -->
      <div class="row">
        <div>
          <div class="label">{t.verticalStyleLabel}</div>
          <div class="desc">{t.verticalStyleDesc}</div>
        </div>
        <div class="flex gap-1.5">
          {#each [["continuous_strip", t.verticalContinuous], ["vertical_grouped", t.verticalGrouped]] as [val, label] (val)}
            <button
              type="button"
              onclick={() =>
                patch((d) => (d.reader.vertical.layoutMode = val as VerticalLayoutMode))}
              class="pill"
              class:pill-active={s.reader.vertical.layoutMode === val}
            >
              {label}
            </button>
          {/each}
        </div>
      </div>

      {#if isVerticalGrouped}
        <!-- Vertical grouped: pages per group -->
        <div class="row">
          <div>
            <div class="label">{t.pagesPerGroupLabel}</div>
            <div class="desc">{t.pagesPerGroupDesc}</div>
          </div>
          <div class="flex items-center gap-1.5">
            {#each [1, 2, 3] as n (n)}
              <button
                type="button"
                onclick={() => patch((d) => (d.reader.vertical.pagesPerGroup = n))}
                class="pill"
                class:pill-active={s.reader.vertical.pagesPerGroup === n}
              >
                {n}
              </button>
            {/each}
            <input
              type="number"
              min="1"
              max="8"
              value={s.reader.vertical.pagesPerGroup}
              onchange={(e) => {
                const v = Math.max(1, Math.min(8, Math.round(Number(e.currentTarget.value) || 1)));
                patch((d) => (d.reader.vertical.pagesPerGroup = v));
              }}
              class="w-14 rounded-lg border px-2 py-1 text-center text-xs"
              style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
            />
          </div>
        </div>

        <div class="row">
          <div class="label">{t.pageGapLabel(s.reader.vertical.pageGapPx)}</div>
          <input
            type="range"
            min="0"
            max="48"
            step="2"
            value={s.reader.vertical.pageGapPx}
            oninput={(e) => {
              const v = Number(e.currentTarget.value);
              patch((d) => {
                d.reader.vertical.pageGapPx = v;
                d.reader.gapPx = v;
              }, 150);
            }}
            class="w-48 accent-rose-500"
          />
        </div>

        <div class="row">
          <div class="label">{t.groupGapLabel(s.reader.vertical.groupGapPx)}</div>
          <input
            type="range"
            min="8"
            max="96"
            step="4"
            value={s.reader.vertical.groupGapPx}
            oninput={(e) => {
              const v = Number(e.currentTarget.value);
              patch((d) => (d.reader.vertical.groupGapPx = v), 150);
            }}
            class="w-48 accent-rose-500"
          />
        </div>

        <div class="row">
          <div class="label">{t.groupSnapLabel}</div>
          <input
            type="checkbox"
            checked={s.reader.vertical.snapEnabled}
            onchange={(e) => {
              const checked = e.currentTarget.checked;
              patch((d) => (d.reader.vertical.snapEnabled = checked));
            }}
            class="h-4 w-4 accent-rose-500"
          />
        </div>

        <div class="row">
          <div class="label">{t.groupAlignLabel}</div>
          <div class="flex gap-1.5">
            {#each [["center", t.alignCenter], ["start", t.alignStart]] as [val, label] (val)}
              <button
                type="button"
                onclick={() => patch((d) => (d.reader.vertical.alignment = val as GroupAlignment))}
                class="pill"
                class:pill-active={s.reader.vertical.alignment === val}
              >
                {label}
              </button>
            {/each}
          </div>
        </div>
      {:else}
        <!-- Continuous strip page gap -->
        <div class="row">
          <div>
            <div class="label">{t.gapLabel(s.reader.vertical.pageGapPx)}</div>
            <div class="desc">{t.gapDesc}</div>
          </div>
          <input
            type="range"
            min="0"
            max="48"
            step="2"
            value={s.reader.vertical.pageGapPx}
            oninput={(e) => {
              const v = Number(e.currentTarget.value);
              patch((d) => {
                d.reader.vertical.pageGapPx = v;
                d.reader.gapPx = v;
              }, 150);
            }}
            class="w-48 accent-rose-500"
          />
        </div>
      {/if}

      <!-- Vertical focus dimming -->
      <div class="row">
        <div>
          <div class="label">{t.dimUnfocusedLabel}</div>
          <div class="desc">{t.dimUnfocusedDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.reader.vertical.dimUnfocused}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.reader.vertical.dimUnfocused = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>

      {#if s.reader.vertical.dimUnfocused}
        <div class="row">
          <div class="label">{t.dimLevelLabel(s.reader.vertical.dimLevel)}</div>
          <input
            type="range"
            min="0"
            max="100"
            step="5"
            value={s.reader.vertical.dimLevel}
            oninput={(e) => {
              const v = Number(e.currentTarget.value);
              patch((d) => (d.reader.vertical.dimLevel = v), 120);
            }}
            class="w-48 accent-rose-500"
          />
        </div>
      {/if}

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
    {:else}
      <!-- Horizontal Mode Controls -->
      <div class="row">
        <div class="label">{t.horizontalDirLabel}</div>
        <div class="flex gap-1.5">
          {#each [["ltr", t.dirLtr], ["rtl", t.dirRtl]] as [val, label] (val)}
            <button
              type="button"
              onclick={() =>
                patch((d) => (d.reader.horizontal.direction = val as HorizontalDirection))}
              class="pill"
              class:pill-active={s.reader.horizontal.direction === val}
            >
              {label}
            </button>
          {/each}
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.pagesPerGroupLabel}</div>
          <div class="desc">{t.pagesPerGroupDesc}</div>
        </div>
        <div class="flex items-center gap-1.5">
          {#each [1, 2, 3] as n (n)}
            <button
              type="button"
              onclick={() => patch((d) => (d.reader.horizontal.pagesPerGroup = n))}
              class="pill"
              class:pill-active={s.reader.horizontal.pagesPerGroup === n}
            >
              {n}
            </button>
          {/each}
          <input
            type="number"
            min="1"
            max="8"
            value={s.reader.horizontal.pagesPerGroup}
            onchange={(e) => {
              const v = Math.max(1, Math.min(8, Math.round(Number(e.currentTarget.value) || 1)));
              patch((d) => (d.reader.horizontal.pagesPerGroup = v));
            }}
            class="w-14 rounded-lg border px-2 py-1 text-center text-xs"
            style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
          />
        </div>
      </div>

      <div class="row">
        <div class="label">{t.pageGapLabel(s.reader.horizontal.pageGapPx)}</div>
        <input
          type="range"
          min="0"
          max="48"
          step="2"
          value={s.reader.horizontal.pageGapPx}
          oninput={(e) => {
            const v = Number(e.currentTarget.value);
            patch((d) => (d.reader.horizontal.pageGapPx = v), 150);
          }}
          class="w-48 accent-rose-500"
        />
      </div>

      <div class="row">
        <div class="label">{t.groupGapLabel(s.reader.horizontal.groupGapPx)}</div>
        <input
          type="range"
          min="8"
          max="96"
          step="4"
          value={s.reader.horizontal.groupGapPx}
          oninput={(e) => {
            const v = Number(e.currentTarget.value);
            patch((d) => (d.reader.horizontal.groupGapPx = v), 150);
          }}
          class="w-48 accent-rose-500"
        />
      </div>

      <div class="row">
        <div class="label">{t.snapModeLabel}</div>
        <div class="flex gap-1.5">
          {#each [["none", t.snapNone], ["page", t.snapPage], ["group", t.snapGroup]] as [val, label] (val)}
            <button
              type="button"
              onclick={() =>
                patch((d) => (d.reader.horizontal.snapMode = val as HorizontalSnapMode))}
              class="pill"
              class:pill-active={s.reader.horizontal.snapMode === val}
            >
              {label}
            </button>
          {/each}
        </div>
      </div>

      <div class="row">
        <div>
          <div class="label">{t.centerGroupsLabel}</div>
          <div class="desc">{t.centerGroupsDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.reader.horizontal.centerGroups}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.reader.horizontal.centerGroups = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>

      <div class="row">
        <div>
          <div class="label">{t.dimUnfocusedLabel}</div>
          <div class="desc">{t.dimUnfocusedDesc}</div>
        </div>
        <input
          type="checkbox"
          checked={s.reader.horizontal.dimUnfocused}
          onchange={(e) => {
            const checked = e.currentTarget.checked;
            patch((d) => (d.reader.horizontal.dimUnfocused = checked));
          }}
          class="h-4 w-4 accent-rose-500"
        />
      </div>

      {#if s.reader.horizontal.dimUnfocused}
        <div class="row">
          <div class="label">{t.dimLevelLabel(s.reader.horizontal.dimLevel)}</div>
          <input
            type="range"
            min="0"
            max="100"
            step="5"
            value={s.reader.horizontal.dimLevel}
            oninput={(e) => {
              const v = Number(e.currentTarget.value);
              patch((d) => (d.reader.horizontal.dimLevel = v), 120);
            }}
            class="w-48 accent-rose-500"
          />
        </div>
      {/if}
    {/if}

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

  <!-- Serverless LAN Sync -->
  <LanSyncPanel />

  <!-- In-App Proxy Mode -->
  <ProxySettingsPanel />

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

  <!-- App Release Updates -->
  <section class="settings-card">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div>
        <h2 class="section-title">{t.appUpdatesSection}</h2>
        <div class="desc">{t.appVersionLabel(appReleaseStore.currentVersion)}</div>
      </div>

      <div class="flex items-center gap-2">
        {#if appReleaseStore.release?.updateAvailable}
          <button
            type="button"
            onclick={() => appReleaseStore.openPrompt()}
            class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-semibold text-white"
            style="background-color: var(--accent);"
          >
            <Download size={13} />
            <span>{t.updateDownloadAndInstall} (v{appReleaseStore.release.latestVersion})</span>
          </button>
        {/if}

        <button
          type="button"
          onclick={() => appReleaseStore.checkNow()}
          disabled={appReleaseStore.checking}
          class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
          style="border-color: var(--border); background-color: var(--surface-subtle);"
        >
          <RefreshCw size={13} class={appReleaseStore.checking ? "animate-spin" : ""} />
          <span>{t.checkReleaseNow}</span>
        </button>
      </div>
    </div>

    {#if appReleaseStore.release?.updateAvailable}
      <div
        class="flex items-center gap-2 rounded-xl border px-3 py-2 text-xs font-medium"
        style="background-color: var(--accent-soft); border-color: var(--accent); color: var(--accent);"
      >
        <Sparkles size={14} class="shrink-0" />
        <span>{t.releaseAvailableBanner(appReleaseStore.release.latestVersion)}</span>
      </div>
    {/if}

    <div class="row">
      <div>
        <div class="label">{t.checkAppUpdatesLabel}</div>
        <div class="desc">{t.checkAppUpdatesDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.updates.checkAppUpdates}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.updates.checkAppUpdates = checked));
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
