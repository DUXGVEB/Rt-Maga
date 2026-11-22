<script lang="ts">
  import { BookOpen, Clock, Home, Moon, RefreshCw, Settings, Sun } from "lucide-svelte";
  import { router } from "../stores/router.svelte";
  import { settingsStore } from "../stores/settings.svelte";
  import { updatesStore } from "../stores/updates.svelte";

  const isDark = $derived(settingsStore.resolvedTheme === "dark");

  function toggleTheme() {
    settingsStore.update((d) => {
      d.theme = isDark ? "light" : "dark";
    });
  }
</script>

<aside
  class="hidden h-full w-56 shrink-0 flex-col justify-between border-r px-3 py-5 select-none md:flex"
  style="background-color: var(--surface); border-color: var(--border);"
>
  <div class="flex flex-col gap-6">
    <!-- Brand -->
    <button
      type="button"
      onclick={() => router.goHome()}
      class="flex items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left transition-opacity hover:opacity-85"
    >
      <div
        class="flex h-8 w-8 items-center justify-center rounded-lg text-white shadow-sm"
        style="background-color: var(--accent);"
      >
        <BookOpen size={17} strokeWidth={2.2} />
      </div>
      <div class="leading-tight">
        <div class="text-base font-semibold tracking-tight">RtMaga</div>
        <div class="text-[11px]" style="color: var(--text-muted);">Đọc truyện gọn nhẹ</div>
      </div>
    </button>

    <!-- Navigation -->
    <nav class="flex flex-col gap-1">
      <button
        type="button"
        onclick={() => router.goHome()}
        class="nav-item"
        class:active={router.current.name === "home" || router.current.name === "manga"}
      >
        <Home size={18} />
        <span>Trang chủ</span>
        {#if updatesStore.newCount > 0}
          <span
            class="ml-auto rounded-full px-2 py-0.5 text-[11px] font-semibold text-white"
            style="background-color: var(--accent);"
          >
            {updatesStore.newCount}
          </span>
        {/if}
      </button>

      <button
        type="button"
        onclick={() => router.goHistory()}
        class="nav-item"
        class:active={router.current.name === "history"}
      >
        <Clock size={18} />
        <span>Lịch sử</span>
      </button>

      <button
        type="button"
        onclick={() => router.goSettings()}
        class="nav-item"
        class:active={router.current.name === "settings"}
      >
        <Settings size={18} />
        <span>Cài đặt</span>
      </button>
    </nav>
  </div>

  <!-- Footer controls -->
  <div class="flex flex-col gap-2 border-t pt-3" style="border-color: var(--border);">
    <button
      type="button"
      onclick={() => updatesStore.checkNow()}
      disabled={updatesStore.checking}
      class="nav-item text-xs"
      title="Kiểm tra chap mới cho các truyện trong lịch sử"
    >
      <RefreshCw size={15} class={updatesStore.checking ? "animate-spin" : ""} />
      <span>
        {#if updatesStore.checking && updatesStore.progress}
          Đang kiểm tra ({updatesStore.progress.done}/{updatesStore.progress.total})
        {:else if updatesStore.checking}
          Đang kiểm tra...
        {:else}
          Kiểm tra chap mới
        {/if}
      </span>
    </button>

    <button type="button" onclick={toggleTheme} class="nav-item text-xs">
      {#if isDark}
        <Sun size={15} />
        <span>Chế độ sáng</span>
      {:else}
        <Moon size={15} />
        <span>Chế độ tối</span>
      {/if}
    </button>
  </div>
</aside>

<style>
  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.65rem;
    width: 100%;
    padding: 0.55rem 0.75rem;
    border-radius: 0.6rem;
    font-size: 0.875rem;
    font-weight: 500;
    color: var(--text-muted);
    background: transparent;
    border: none;
    cursor: pointer;
    transition: background-color 140ms ease, color 140ms ease;
    text-align: left;
  }

  .nav-item:hover:not(:disabled) {
    background-color: var(--surface-hover);
    color: var(--text);
  }

  .nav-item.active {
    background-color: var(--accent-soft);
    color: var(--accent);
  }

  .nav-item:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
