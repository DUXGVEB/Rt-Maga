<script lang="ts">
  import { Bell, BookOpen, CheckCheck, Sparkles, Trash2, X } from "lucide-svelte";
  import { imgProxyUrl } from "../api";
  import { backdropFade, createOverlayGuard, drawerSlide } from "../motion";
  import { deviceEnv } from "../stores/deviceEnv.svelte";
  import { notificationsStore } from "../stores/notifications.svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import type { NotificationItemView } from "../types";

  const t = $derived(settingsStore.t);
  const isCompactMobile = $derived(deviceEnv.layoutTier === "compact");
  const drawerGuard = createOverlayGuard(180);

  function closeDrawerSafely() {
    if (!drawerGuard.canAct()) return;
    drawerGuard.lock();
    notificationsStore.closeDrawer();
  }

  $effect(() => {
    if (!notificationsStore.drawerOpen) return;
    drawerGuard.lock(140);
    return router.registerBackInterceptor(() => {
      if (notificationsStore.drawerOpen) {
        notificationsStore.closeDrawer();
        return true;
      }
      return false;
    });
  });

  function formatTime(unixSecs: number): string {
    return new Date(unixSecs * 1000).toLocaleString(settingsStore.locale, {
      month: "short",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  function openItem(item: NotificationItemView) {
    void notificationsStore.markRead(item.id);
    notificationsStore.closeDrawer();
    router.goManga(item.mangaId);
  }
</script>

{#if notificationsStore.drawerOpen}
  <!-- Backdrop (Svelte transition only — no competing CSS transition) -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    transition:backdropFade|local
    onclick={closeDrawerSafely}
    class="fixed inset-0 z-40 bg-black/48 backdrop-blur-[2px]"
  ></div>

  <!-- Adaptive Panel: Ergonomic Bottom Sheet on Compact Mobile (<768px), Right Slide-Over Drawer on Tablet/Desktop (>=768px) -->
  <aside
    transition:drawerSlide|local={{
      side: isCompactMobile ? "bottom" : "right",
      distance: isCompactMobile ? 56 : 48,
      duration: 220,
    }}
    class={isCompactMobile
      ? "glass-chrome fixed right-0 bottom-0 left-0 z-50 flex max-h-[84dvh] w-full flex-col rounded-t-3xl border-t shadow-2xl"
      : "glass-chrome fixed top-0 right-0 bottom-0 z-50 flex w-full max-w-md flex-col border-l shadow-2xl"}
    style={isCompactMobile
      ? "padding-bottom: max(0.75rem, var(--safe-bottom)); padding-left: var(--safe-left); padding-right: var(--safe-right); border-color: var(--border); box-shadow: var(--shadow-lg);"
      : "padding-top: var(--safe-top); padding-bottom: var(--safe-bottom); padding-right: var(--safe-right); border-color: var(--border); box-shadow: var(--shadow-lg);"}
  >
    {#if isCompactMobile}
      <div class="sheet-handle" aria-hidden="true"></div>
    {/if}

    <div
      class="flex items-center justify-between gap-2 border-b px-4 py-3"
      style="border-color: var(--border);"
    >
      <div class="flex items-center gap-2">
        <Bell size={18} style="color: var(--accent);" />
        <h2 class="text-base font-bold tracking-tight">{t.notifCenterTitle}</h2>
        {#if notificationsStore.unreadCount > 0}
          <span
            class="rounded-full px-2 py-0.5 text-[11px] font-semibold tabular-nums"
            style="background-color: var(--accent); color: var(--accent-fg);"
          >
            {notificationsStore.unreadCount}
          </span>
        {/if}
      </div>

      <div class="flex items-center gap-1">
        {#if notificationsStore.unreadCount > 0}
          <button
            type="button"
            onclick={() => notificationsStore.markAllRead()}
            class="gpu-press inline-flex min-h-[40px] items-center gap-1 rounded-xl px-2.5 py-1 text-xs font-semibold hover:opacity-80"
            style="color: var(--accent);"
            title={t.notifMarkAllRead}
          >
            <CheckCheck size={15} />
            <span class="hidden sm:inline">{t.notifMarkAllRead}</span>
          </button>
        {/if}
        {#if notificationsStore.items.length > 0}
          <button
            type="button"
            onclick={() => notificationsStore.clearAll()}
            class="gpu-press flex h-10 w-10 items-center justify-center rounded-xl opacity-75 hover:opacity-100"
            style="color: var(--danger);"
            title={t.clearAll}
            aria-label={t.clearAll}
          >
            <Trash2 size={16} />
          </button>
        {/if}
        <button
          type="button"
          onclick={closeDrawerSafely}
          class="gpu-press flex h-10 w-10 items-center justify-center rounded-xl opacity-75 hover:opacity-100"
          aria-label={t.cancel}
        >
          <X size={18} />
        </button>
      </div>
    </div>

    <div class="flex-1 overflow-y-auto overscroll-contain p-3">
      {#if notificationsStore.items.length === 0}
        <div
          class="flex h-full min-h-44 flex-col items-center justify-center gap-3 px-6 text-center"
          style="color: var(--text-muted);"
        >
          <Bell size={28} strokeWidth={1.5} />
          <div class="text-sm font-medium" style="color: var(--text);">{t.notifEmptyTitle}</div>
        </div>
      {:else}
        <div class="flex flex-col gap-2">
          {#each notificationsStore.items as item (item.id)}
            <div
              role="button"
              tabindex="0"
              onclick={() => openItem(item)}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  openItem(item);
                }
              }}
              class="gpu-press flex min-h-[64px] cursor-pointer items-center gap-3 rounded-2xl border p-3 hover:opacity-95"
              style="background-color: {item.read
                ? 'var(--surface-subtle)'
                : 'color-mix(in srgb, var(--surface) 92%, var(--accent-soft))'}; border-color: {item.read
                ? 'var(--border)'
                : 'color-mix(in srgb, var(--accent) 35%, var(--border))'};"
            >
              <div
                class="h-14 w-10 shrink-0 overflow-hidden rounded-xl"
                style="background-color: var(--surface-hover);"
              >
                {#if item.coverToken}
                  <img
                    src={imgProxyUrl(item.coverToken)}
                    alt={item.mangaTitle}
                    loading="lazy"
                    class="h-full w-full object-cover"
                  />
                {:else}
                  <div
                    class="flex h-full w-full items-center justify-center"
                    style="color: var(--text-muted);"
                  >
                    <BookOpen size={15} />
                  </div>
                {/if}
              </div>

              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1.5">
                  {#if !item.read}
                    <span
                      class="h-2 w-2 shrink-0 rounded-full"
                      style="background-color: var(--accent);"
                    ></span>
                  {/if}
                  <span class="truncate text-sm font-semibold">{item.mangaTitle}</span>
                </div>
                <div class="mt-1 flex flex-wrap items-center gap-1.5 text-xs">
                  <span
                    class="inline-flex items-center gap-1 rounded-lg px-2 py-0.5 font-medium"
                    style="background-color: var(--accent-soft); color: var(--accent);"
                  >
                    <Sparkles size={11} />
                    {t.notifChaptersLabel(item.chapterRange, item.chapterCount)}
                  </span>
                </div>
                <div class="mt-1 text-[11px]" style="color: var(--text-muted);">
                  {formatTime(item.createdAt)}
                </div>
              </div>

              <button
                type="button"
                onclick={(e) => {
                  e.stopPropagation();
                  void notificationsStore.remove(item.id);
                }}
                class="gpu-press flex h-9 w-9 items-center justify-center rounded-xl opacity-60 hover:opacity-100"
                style="color: var(--text-muted);"
                title={t.dismissNotification}
                aria-label={t.dismissNotification}
              >
                <X size={15} />
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </aside>
{/if}
