<script lang="ts">
  import { Bell, Download, RefreshCw, RotateCcw, Smartphone, Sparkles, Trash2 } from "lucide-svelte";
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import LanSyncPanel from "../lib/components/LanSyncPanel.svelte";
  import NetworkProtectionPanel from "../lib/components/NetworkProtectionPanel.svelte";
  import { LANGUAGE_OPTIONS } from "../lib/i18n";
  import { appReleaseStore } from "../lib/stores/appRelease.svelte";
  import { deviceEnv } from "../lib/stores/deviceEnv.svelte";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type {
    CacheInfo,
    FloatingEdge,
    FloatingScreenKey,
    HorizontalDirection,
    HorizontalSnapMode,
    Language,
    NotificationPermissionInfo,
    ReaderBg,
    ReadingOrientation,
    Settings,
    Theme,
  } from "../lib/types";

  let cache = $state<CacheInfo | null>(null);
  let clearingCache = $state(false);
  let notifPerm = $state<NotificationPermissionInfo>({ granted: true, canRequest: false });

  const s = $derived(settingsStore.value);
  const t = $derived(settingsStore.t);
  const isHorizontal = $derived(s.reader.orientation === "horizontal");

  const floatingScreens: FloatingScreenKey[] = [
    "home",
    "bookmarks",
    "history",
    "manga",
    "reader",
    "settings",
  ];

  function floatingScreenLabel(key: FloatingScreenKey): string {
    switch (key) {
      case "home":
        return t.navHome;
      case "bookmarks":
        return t.navBookmarks;
      case "history":
        return t.navHistory;
      case "manga":
        return t.floatingScreenManga;
      case "reader":
        return t.floatingScreenReader;
      case "settings":
        return t.navSettings;
    }
  }

  function toggleScrollTopScreen(screen: FloatingScreenKey) {
    patch((d) => {
      const list = d.floating.scrollToTopScreens;
      if (list.includes(screen)) {
        d.floating.scrollToTopScreens = list.filter((x) => x !== screen);
      } else {
        d.floating.scrollToTopScreens = [...list, screen];
      }
    });
  }

  onMount(async () => {
    try {
      notifPerm = await api.checkNotificationPermission();
    } catch {
      notifPerm = { granted: true, canRequest: false };
    }
  });

  async function requestAndroidNotifPermission() {
    try {
      notifPerm = await api.requestNotificationPermission();
    } catch {
      // Ignore outside Android.
    }
  }

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

  <!-- Device Identity -->
  <section class="settings-card">
    <div class="flex items-center gap-2">
      <Smartphone size={15} style="color: var(--accent);" />
      <h2 class="section-title">{t.deviceIdentitySection}</h2>
    </div>

    <div class="row">
      <div>
        <div class="label">{t.deviceDetectedLabel}</div>
        <div class="desc">
          {deviceEnv.info.realDeviceName}
          {#if deviceEnv.info.skin && deviceEnv.info.skin !== "desktop"}
            · {deviceEnv.info.skin.toUpperCase()}
          {/if}
        </div>
      </div>
    </div>

    <div class="row">
      <div>
        <div class="label">{t.deviceCustomNameLabel}</div>
        <div class="desc">{t.deviceCustomNameDesc}</div>
      </div>
      <div class="flex items-center gap-1.5">
        <input
          type="text"
          maxlength="48"
          value={s.device.customName ?? ""}
          placeholder={deviceEnv.info.realDeviceName}
          oninput={(e) => {
            const val = e.currentTarget.value;
            patch((d) => {
              d.device.customName = val.trim() ? val : null;
            }, 300);
          }}
          class="w-44 rounded-lg border px-2.5 py-1.5 text-xs outline-none sm:w-52"
          style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
        />
        {#if s.device.customName}
          <button
            type="button"
            onclick={() => patch((d) => (d.device.customName = null))}
            class="rounded-lg border p-1.5"
            style="border-color: var(--border); background-color: var(--surface-subtle);"
            title={t.deviceResetName}
          >
            <RotateCcw size={13} />
          </button>
        {/if}
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
      <!-- Continuous vertical strip page gap -->
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
        <div class="label">{t.autoWebtoonLabel}</div>
        <div class="desc">{t.autoWebtoonDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.reader.autoWebtoonDetect}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.reader.autoWebtoonDetect = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>

    <div class="row">
      <div>
        <div class="label">{t.tapZonesLabel}</div>
        <div class="desc">{t.tapZonesDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.reader.tapZonesEnabled}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.reader.tapZonesEnabled = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>

    <div class="row">
      <div>
        <div class="label">{t.preloadLabel(s.reader.preloadImages)}</div>
        <div class="desc">{t.preloadDesc}</div>
      </div>
      <input
        type="range"
        min="5"
        max="8"
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

  <!-- Floating Controls Customization -->
  <section class="settings-card">
    <div>
      <h2 class="section-title">{t.floatingSection}</h2>
      <div class="desc">{t.floatingSubtitle}</div>
    </div>

    <!-- Scroll-to-Top per-screen toggles -->
    <div class="flex flex-col gap-2">
      <div class="label">{t.floatingScrollTopScreens}</div>
      <div class="flex flex-wrap gap-1.5">
        {#each floatingScreens as screen (screen)}
          {@const active = s.floating.scrollToTopScreens.includes(screen)}
          <button
            type="button"
            onclick={() => toggleScrollTopScreen(screen)}
            class="pill"
            class:pill-active={active}
          >
            {floatingScreenLabel(screen)}
          </button>
        {/each}
      </div>
    </div>

    <!-- Scroll-to-Top position -->
    <div class="row">
      <div class="label">{t.floatingScrollTopPos}</div>
      <div class="flex flex-wrap items-center gap-2">
        {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
          <button
            type="button"
            onclick={() => patch((d) => (d.floating.scrollToTop.edge = edge as FloatingEdge))}
            class="pill"
            class:pill-active={s.floating.scrollToTop.edge === edge}
          >
            {label}
          </button>
        {/each}
        <input
          type="range"
          min="15"
          max="88"
          step="1"
          value={s.floating.scrollToTop.verticalPercent}
          oninput={(e) => {
            const v = Number(e.currentTarget.value);
            patch((d) => (d.floating.scrollToTop.verticalPercent = v), 120);
          }}
          class="w-28 accent-rose-500"
          title="{s.floating.scrollToTop.verticalPercent}%"
        />
        <span class="w-9 text-right text-xs" style="color: var(--text-muted);">
          {s.floating.scrollToTop.verticalPercent}%
        </span>
      </div>
    </div>

    <!-- Reader Settings floating button -->
    <div class="row">
      <div>
        <div class="label">{t.floatingReaderSettingsLabel}</div>
        <div class="desc">{t.floatingReaderSettingsDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.floating.showReaderSettings}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.floating.showReaderSettings = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>

    {#if s.floating.showReaderSettings}
      <div class="row">
        <div class="desc">{t.floatingPositionLabel}</div>
        <div class="flex flex-wrap items-center gap-2">
          {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
            <button
              type="button"
              onclick={() => patch((d) => (d.floating.readerSettings.edge = edge as FloatingEdge))}
              class="pill"
              class:pill-active={s.floating.readerSettings.edge === edge}
            >
              {label}
            </button>
          {/each}
          <input
            type="range"
            min="15"
            max="88"
            step="1"
            value={s.floating.readerSettings.verticalPercent}
            oninput={(e) => {
              const v = Number(e.currentTarget.value);
              patch((d) => (d.floating.readerSettings.verticalPercent = v), 120);
            }}
            class="w-28 accent-rose-500"
          />
          <span class="w-9 text-right text-xs" style="color: var(--text-muted);">
            {s.floating.readerSettings.verticalPercent}%
          </span>
        </div>
      </div>
    {/if}

    <!-- Mobile Orientation Lock floating button -->
    <div class="row">
      <div>
        <div class="label">{t.floatingOrientationLockLabel}</div>
        <div class="desc">{t.floatingOrientationLockDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.floating.showOrientationLock}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.floating.showOrientationLock = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>

    {#if s.floating.showOrientationLock}
      <div class="row">
        <div class="desc">{t.floatingPositionLabel}</div>
        <div class="flex flex-wrap items-center gap-2">
          {#each [["left", t.floatingEdgeLeft], ["right", t.floatingEdgeRight]] as [edge, label] (edge)}
            <button
              type="button"
              onclick={() => patch((d) => (d.floating.orientationLock.edge = edge as FloatingEdge))}
              class="pill"
              class:pill-active={s.floating.orientationLock.edge === edge}
            >
              {label}
            </button>
          {/each}
          <input
            type="range"
            min="15"
            max="88"
            step="1"
            value={s.floating.orientationLock.verticalPercent}
            oninput={(e) => {
              const v = Number(e.currentTarget.value);
              patch((d) => (d.floating.orientationLock.verticalPercent = v), 120);
            }}
            class="w-28 accent-rose-500"
          />
          <span class="w-9 text-right text-xs" style="color: var(--text-muted);">
            {s.floating.orientationLock.verticalPercent}%
          </span>
        </div>
      </div>
    {/if}

    <div class="row">
      <div>
        <div class="label">{t.floatingAutoDimLabel}</div>
        <div class="desc">{t.floatingAutoDimDesc}</div>
      </div>
      <input
        type="checkbox"
        checked={s.floating.autoDimInReader}
        onchange={(e) => {
          const checked = e.currentTarget.checked;
          patch((d) => (d.floating.autoDimInReader = checked));
        }}
        class="h-4 w-4 accent-rose-500"
      />
    </div>
  </section>

  <!-- Serverless LAN Sync -->
  <LanSyncPanel />

  <!-- Provider-Based Network Protection (Encrypted DNS + Advanced Proxy) -->
  <NetworkProtectionPanel />

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
      <div class="flex items-center gap-2">
        {#if deviceEnv.isAndroid && !notifPerm.granted}
          <button
            type="button"
            onclick={requestAndroidNotifPermission}
            class="inline-flex items-center gap-1 rounded-lg border px-2.5 py-1 text-xs font-medium"
            style="border-color: var(--accent); color: var(--accent); background-color: var(--accent-soft);"
          >
            <Bell size={12} />
            <span>{t.notifGrantPermissionBtn}</span>
          </button>
        {/if}
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
