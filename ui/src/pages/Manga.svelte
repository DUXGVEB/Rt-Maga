<script lang="ts">
  import {
    ArrowDownUp,
    ArrowLeft,
    BookOpen,
    Check,
    Play,
    RefreshCw,
    Search,
  } from "lucide-svelte";
  import { api, formatAppError, imgProxyUrl } from "../lib/api";
  import { router } from "../lib/stores/router.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import type { MangaView } from "../lib/types";

  let { id }: { id: number } = $props();

  let manga = $state<MangaView | null>(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let errorText = $state<string | null>(null);
  let query = $state("");
  let newestFirst = $state(true);

  async function load() {
    loading = true;
    errorText = null;
    try {
      manga = await api.getManga(id);
    } catch (err) {
      errorText = formatAppError(err).detail;
    } finally {
      loading = false;
    }
  }

  async function refresh() {
    if (refreshing) return;
    refreshing = true;
    try {
      manga = await api.refreshManga(id);
      toasts.push("Đã cập nhật danh sách chương mới nhất.", "success");
    } catch (err) {
      toasts.push(formatAppError(err).title, "error");
    } finally {
      refreshing = false;
    }
  }

  $effect(() => {
    void id;
    load();
  });

  const filteredChapters = $derived(() => {
    if (!manga) return [];
    const q = query.trim().toLowerCase();
    const list = q
      ? manga.chapters.filter((c) => c.name.toLowerCase().includes(q))
      : manga.chapters;
    return newestFirst ? [...list].reverse() : list;
  });

  function startPrimary() {
    if (!manga || manga.chapters.length === 0) return;
    if (manga.progress) {
      router.goReader(manga.id, manga.progress.chapterUrl, manga.progress.scrollRatio);
    } else {
      router.goReader(manga.id, manga.chapters[0].url, 0);
    }
  }
</script>

<div class="mx-auto flex w-full max-w-5xl flex-col gap-6 px-8 py-8">
  <div class="flex items-center justify-between">
    <button
      type="button"
      onclick={() => router.goHome()}
      class="inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-sm transition-colors hover:opacity-80"
      style="color: var(--text-muted);"
    >
      <ArrowLeft size={16} />
      <span>Trang chủ</span>
    </button>

    {#if manga}
      <button
        type="button"
        onclick={refresh}
        disabled={refreshing}
        class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium transition-colors"
        style="background-color: var(--surface); border-color: var(--border);"
      >
        <RefreshCw size={14} class={refreshing ? "animate-spin" : ""} />
        <span>{refreshing ? "Đang làm mới..." : "Làm mới chương"}</span>
      </button>
    {/if}
  </div>

  {#if loading}
    <div class="flex gap-6">
      <div
        class="h-60 w-44 shrink-0 animate-pulse rounded-xl"
        style="background-color: var(--surface-hover);"
      ></div>
      <div class="flex flex-1 flex-col gap-3 pt-2">
        <div class="h-7 w-2/3 animate-pulse rounded" style="background-color: var(--surface-hover);"></div>
        <div class="h-4 w-1/3 animate-pulse rounded" style="background-color: var(--surface-hover);"></div>
      </div>
    </div>
  {:else if errorText}
    <div
      class="rounded-xl border p-6 text-center text-sm"
      style="background-color: var(--surface); border-color: var(--danger);"
    >
      {errorText}
    </div>
  {:else if manga}
    <!-- Manga Header -->
    <section
      class="flex flex-col gap-6 rounded-2xl border p-6 sm:flex-row"
      style="background-color: var(--surface); border-color: var(--border); box-shadow: var(--shadow-sm);"
    >
      <div
        class="h-60 w-44 shrink-0 overflow-hidden rounded-xl"
        style="background-color: var(--surface-subtle);"
      >
        {#if manga.coverToken}
          <img
            src={imgProxyUrl(manga.coverToken)}
            alt={manga.title}
            class="h-full w-full object-cover"
          />
        {:else}
          <div class="flex h-full w-full items-center justify-center" style="color: var(--text-muted);">
            <BookOpen size={36} strokeWidth={1.5} />
          </div>
        {/if}
      </div>

      <div class="flex min-w-0 flex-1 flex-col justify-between gap-4">
        <div class="flex flex-col gap-2">
          <div class="flex flex-wrap items-center gap-2">
            <span
              class="rounded-md px-2 py-0.5 text-xs font-medium"
              style="background-color: var(--surface-hover); color: var(--text-muted);"
            >
              {manga.site}
            </span>
            <span class="text-xs" style="color: var(--text-muted);">
              {manga.chapters.length} chương
            </span>
          </div>

          <h1 class="text-2xl font-bold leading-tight tracking-tight">{manga.title}</h1>
          <div class="truncate text-xs" style="color: var(--text-muted);">{manga.url}</div>
        </div>

        <div class="flex flex-wrap items-center gap-3">
          <button
            type="button"
            onclick={startPrimary}
            disabled={manga.chapters.length === 0}
            class="inline-flex items-center gap-2 rounded-xl px-5 py-2.5 text-sm font-semibold text-white transition-opacity hover:opacity-90 disabled:opacity-45"
            style="background-color: var(--accent);"
          >
            <Play size={16} fill="currentColor" />
            <span>
              {#if manga.progress}
                Đọc tiếp · {manga.progress.chapterName}
              {:else}
                Đọc từ đầu
              {/if}
            </span>
          </button>

          {#if manga.progress && manga.chapters.length > 0}
            <button
              type="button"
              onclick={() => manga && router.goReader(manga.id, manga.chapters[0].url, 0)}
              class="rounded-xl border px-4 py-2.5 text-sm font-medium transition-colors"
              style="border-color: var(--border); background-color: var(--surface-subtle);"
            >
              Đọc từ chương đầu
            </button>
          {/if}
        </div>
      </div>
    </section>

    <!-- Chapter list controls -->
    <section class="flex flex-col gap-3">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <h2 class="text-base font-semibold">Danh sách chương</h2>

        <div class="flex items-center gap-2">
          <div
            class="flex items-center gap-2 rounded-xl border px-3 py-1.5 text-sm"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            <Search size={14} style="color: var(--text-muted);" />
            <input
              type="text"
              bind:value={query}
              placeholder="Tìm số chương..."
              class="w-40 bg-transparent text-xs outline-none"
            />
          </div>

          <button
            type="button"
            onclick={() => {
              newestFirst = !newestFirst;
            }}
            class="inline-flex items-center gap-1.5 rounded-xl border px-3 py-1.5 text-xs font-medium"
            style="background-color: var(--surface); border-color: var(--border);"
          >
            <ArrowDownUp size={14} />
            <span>{newestFirst ? "Mới nhất trước" : "Cũ nhất trước"}</span>
          </button>
        </div>
      </div>

      <div
        class="grid max-h-[55vh] grid-cols-1 gap-1.5 overflow-y-auto rounded-2xl border p-3 sm:grid-cols-2 lg:grid-cols-3"
        style="background-color: var(--surface); border-color: var(--border);"
      >
        {#each filteredChapters() as chapter (chapter.url)}
          {@const isCurrent = manga.progress?.chapterUrl === chapter.url}
          <button
            type="button"
            onclick={() =>
              manga &&
              router.goReader(
                manga.id,
                chapter.url,
                isCurrent ? manga.progress?.scrollRatio : 0
              )}
            class="flex items-center justify-between gap-2 rounded-lg px-3 py-2 text-left text-sm transition-colors hover:brightness-95"
            style="background-color: {isCurrent
              ? 'var(--accent-soft)'
              : 'var(--surface-subtle)'}; color: {isCurrent
              ? 'var(--accent)'
              : chapter.read
                ? 'var(--text-muted)'
                : 'var(--text)'};"
          >
            <span class="truncate font-medium">{chapter.name}</span>
            {#if chapter.read}
              <Check size={14} class="shrink-0 opacity-70" />
            {/if}
          </button>
        {/each}
      </div>
    </section>
  {/if}
</div>
