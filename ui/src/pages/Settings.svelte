<script lang="ts">
  import { RefreshCw, Trash2 } from "lucide-svelte";
  import { api } from "../lib/api";
  import { settingsStore } from "../lib/stores/settings.svelte";
  import { toasts } from "../lib/stores/toast.svelte";
  import { updatesStore } from "../lib/stores/updates.svelte";
  import type { CacheInfo, ReaderBg, Settings, Theme } from "../lib/types";

  let cache = $state<CacheInfo | null>(null);
  let clearingCache = $state(false);

  const s = $derived(settingsStore.value);

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
      toasts.push("Đã xoá bộ nhớ đệm ảnh.", "success");
    } catch {
      toasts.push("Không thể xoá bộ nhớ đệm.", "error");
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
    <h1 class="text-2xl font-bold tracking-tight">Cài đặt</h1>
    <p class="mt-1 text-xs" style="color: var(--text-muted);">
      Mọi thay đổi được lưu tự động và áp dụng ngay.
    </p>
  </div>

  <!-- Appearance -->
  <section class="settings-card">
    <h2 class="section-title">Giao diện</h2>
    <div class="row">
      <div>
        <div class="label">Chủ đề màu</div>
        <div class="desc">Chọn giao diện sáng, tối hoặc tự động theo Windows.</div>
      </div>
      <div class="flex gap-1.5">
        {#each [["system", "Hệ thống"], ["light", "Sáng"], ["dark", "Tối"]] as [val, label] (val)}
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
    <h2 class="section-title">Trình đọc cuộn dọc</h2>

    <div class="row">
      <div>
        <div class="label">Chế độ độ rộng ảnh</div>
        <div class="desc">Giữ cố định theo pixel, theo phần trăm hoặc vừa cửa sổ.</div>
      </div>
      <div class="flex gap-1.5">
        <button
          type="button"
          onclick={() => patch((d) => (d.reader.width = { mode: "px", value: 800 }))}
          class="pill"
          class:pill-active={s.reader.width.mode === "px"}
        >
          Pixel
        </button>
        <button
          type="button"
          onclick={() => patch((d) => (d.reader.width = { mode: "percent", value: 70 }))}
          class="pill"
          class:pill-active={s.reader.width.mode === "percent"}
        >
          Phần trăm
        </button>
        <button
          type="button"
          onclick={() => patch((d) => (d.reader.width = { mode: "fit_window" }))}
          class="pill"
          class:pill-active={s.reader.width.mode === "fit_window"}
        >
          Vừa khung
        </button>
      </div>
    </div>

    {#if s.reader.width.mode === "px"}
      <div class="row">
        <div class="label">Độ rộng ({s.reader.width.value} px)</div>
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
        <div class="label">Độ rộng ({s.reader.width.value}%)</div>
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
        <div class="label">Khoảng cách giữa các ảnh ({s.reader.gapPx} px)</div>
        <div class="desc">Để 0 px để ảnh nối liền mạch kiểu webtoon.</div>
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
        <div class="label">Số ảnh tải trước ({s.reader.preloadImages} ảnh)</div>
        <div class="desc">Tải sẵn ảnh phía dưới để cuộn mượt hơn.</div>
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
        <div class="label">Tự nối chương tiếp theo khi cuộn hết</div>
        <div class="desc">Tắt đi nếu bạn muốn hiện nút bấm chuyển chương ở cuối trang.</div>
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
      <div class="label">Nền trình đọc</div>
      <div class="flex gap-1.5">
        {#each [["dark", "Tối"], ["light", "Sáng"], ["follow_theme", "Theo app"]] as [val, label] (val)}
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
      <h2 class="section-title">Kiểm tra chap mới</h2>
      <button
        type="button"
        onclick={() => updatesStore.checkNow()}
        disabled={updatesStore.checking}
        class="inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 text-xs font-medium"
        style="border-color: var(--border); background-color: var(--surface-subtle);"
      >
        <RefreshCw size={13} class={updatesStore.checking ? "animate-spin" : ""} />
        <span>Kiểm tra ngay</span>
      </button>
    </div>

    <div class="row">
      <div>
        <div class="label">Tự động kiểm tra truyện trong lịch sử</div>
        <div class="desc">Chạy nền theo chu kỳ đã chọn.</div>
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
      <div class="label">Chu kỳ kiểm tra</div>
      <select
        value={s.updates.intervalHours}
        onchange={(e) => {
          const v = Number(e.currentTarget.value);
          patch((d) => (d.updates.intervalHours = v));
        }}
        class="rounded-lg border px-3 py-1.5 text-xs"
        style="background-color: var(--surface-subtle); border-color: var(--border); color: var(--text);"
      >
        <option value={1}>Mỗi 1 giờ</option>
        <option value={3}>Mỗi 3 giờ</option>
        <option value={6}>Mỗi 6 giờ</option>
        <option value={12}>Mỗi 12 giờ</option>
        <option value={24}>Mỗi 24 giờ</option>
      </select>
    </div>

    <div class="row">
      <div>
        <div class="label">Gửi thông báo Windows</div>
        <div class="desc">Hiện thông báo hệ thống khi phát hiện chương mới.</div>
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
    <h2 class="section-title">Nguồn dữ liệu</h2>

    <div class="row">
      <div>
        <div class="label">Trình duyệt chạy ngầm (Headless Edge/Chrome)</div>
        <div class="desc">
          Tự động bật khi gặp web tải ảnh bằng JavaScript mà bộ phân tích tĩnh không lấy được.
        </div>
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
    <h2 class="section-title">Bộ nhớ đệm ảnh</h2>

    <div class="row">
      <div>
        <div class="label">Dung lượng đang dùng</div>
        <div class="desc">
          {#if cache}
            {formatBytes(cache.bytes)} ({cache.files} ảnh đã lưu tạm)
          {:else}
            Đang tính...
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
        <span>{clearingCache ? "Đang xoá..." : "Xoá cache"}</span>
      </button>
    </div>

    <div class="row">
      <div class="label">Giới hạn tối đa ({s.cache.maxMb} MB)</div>
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
