import type { CachedMangaMeta } from "../depthCarouselMath";
import type {
  BookmarkItemView,
  BookmarkSort,
  HistoryItemView,
  MangaView,
  SavedSourceView,
  VisibilityFilter,
} from "../types";

const MAX_META_CACHE_ENTRIES = 320;
const MAX_MANGA_DETAIL_CACHE = 24;

export interface MangaPreviewSeed {
  id: number;
  title: string;
  coverToken: string | null;
  site: string;
  isCompleted?: boolean;
  unreadCount?: number;
  bookmarked?: boolean;
  hidden?: boolean;
}

/**
 * Lightweight in-memory snapshot cache for Home, Sources (SWR), History, Bookmarks,
 * compact Manga metadata/previews, and recent MangaView detail snapshots.
 * Eliminates skeleton/loading flicker and cover jumps when switching tabs or opening manga detail,
 * while every visit still performs a silent background refresh from SQLite.
 */
class LibraryCacheStore {
  history = $state<HistoryItemView[]>([]);
  historyLoaded = $state(false);
  bookmarksBySort = $state<Partial<Record<BookmarkSort, BookmarkItemView[]>>>({});
  bookmarksLoaded = $state<Partial<Record<BookmarkSort, boolean>>>({});
  sourcesByScope = $state<Partial<Record<VisibilityFilter, SavedSourceView[]>>>({});
  sourcesLoaded = $state<Partial<Record<VisibilityFilter, boolean>>>({});
  mangaMetaById = $state<Record<number, CachedMangaMeta>>({});
  mangaPreviewById = $state<Record<number, MangaPreviewSeed>>({});
  mangaDetailById = $state<Record<number, MangaView>>({});
  private mangaDetailOrder: number[] = [];

  setHistory(items: HistoryItemView[]) {
    this.history = items;
    this.historyLoaded = true;
    const nextPreviews: Record<number, MangaPreviewSeed> = { ...this.mangaPreviewById };
    for (const item of items) {
      nextPreviews[item.mangaId] = {
        id: item.mangaId,
        title: item.title,
        coverToken: item.coverToken,
        site: item.site,
        isCompleted: item.isCompleted,
        unreadCount: item.unreadCount,
        bookmarked: item.bookmarked,
      };
    }
    this.mangaPreviewById = nextPreviews;
  }

  setSources(filter: VisibilityFilter, items: SavedSourceView[]) {
    this.sourcesByScope = { ...this.sourcesByScope, [filter]: items };
    this.sourcesLoaded = { ...this.sourcesLoaded, [filter]: true };
  }

  invalidateSources() {
    this.sourcesLoaded = {};
  }

  setMangaPreview(seed: MangaPreviewSeed) {
    this.mangaPreviewById = { ...this.mangaPreviewById, [seed.id]: seed };
  }

  setMangaDetail(detail: MangaView) {
    const next: Record<number, MangaView> = { ...this.mangaDetailById, [detail.id]: detail };
    this.mangaDetailOrder = [...this.mangaDetailOrder.filter((k) => k !== detail.id), detail.id];
    while (this.mangaDetailOrder.length > MAX_MANGA_DETAIL_CACHE) {
      const oldest = this.mangaDetailOrder.shift();
      if (oldest !== undefined) {
        delete next[oldest];
      }
    }
    this.mangaDetailById = next;
    this.mangaPreviewById = {
      ...this.mangaPreviewById,
      [detail.id]: {
        id: detail.id,
        title: detail.title,
        coverToken: detail.coverToken,
        site: detail.site,
        isCompleted: detail.isCompleted,
        unreadCount: detail.unreadCount,
        bookmarked: detail.bookmarked,
        hidden: detail.hidden,
      },
    };
  }

  getMangaDetail(id: number): MangaView | null {
    return this.mangaDetailById[id] ?? null;
  }

  getMangaPreview(id: number): MangaPreviewSeed | null {
    const direct = this.mangaPreviewById[id];
    if (direct) return direct;
    const fromDetail = this.mangaDetailById[id];
    if (fromDetail) {
      return {
        id: fromDetail.id,
        title: fromDetail.title,
        coverToken: fromDetail.coverToken,
        site: fromDetail.site,
        isCompleted: fromDetail.isCompleted,
        unreadCount: fromDetail.unreadCount,
        bookmarked: fromDetail.bookmarked,
        hidden: fromDetail.hidden,
      };
    }
    const fromHistory = this.history.find((h) => h.mangaId === id);
    if (fromHistory) {
      return {
        id: fromHistory.mangaId,
        title: fromHistory.title,
        coverToken: fromHistory.coverToken,
        site: fromHistory.site,
        isCompleted: fromHistory.isCompleted,
        unreadCount: fromHistory.unreadCount,
        bookmarked: fromHistory.bookmarked,
      };
    }
    for (const list of Object.values(this.bookmarksBySort)) {
      const found = list?.find((b) => b.mangaId === id);
      if (found) {
        return {
          id: found.mangaId,
          title: found.title,
          coverToken: found.coverToken,
          site: found.site,
          isCompleted: found.isCompleted,
          unreadCount: found.unreadCount,
          bookmarked: true,
        };
      }
    }
    return null;
  }

  removeMangaDetail(id: number) {
    if (this.mangaDetailById[id]) {
      const next = { ...this.mangaDetailById };
      delete next[id];
      this.mangaDetailById = next;
      this.mangaDetailOrder = this.mangaDetailOrder.filter((k) => k !== id);
    }
    if (this.mangaPreviewById[id]) {
      const nextPrev = { ...this.mangaPreviewById };
      delete nextPrev[id];
      this.mangaPreviewById = nextPrev;
    }
  }

  invalidate() {
    this.historyLoaded = false;
    this.bookmarksLoaded = {};
    this.sourcesLoaded = {};
    this.mangaMetaById = {};
    this.mangaDetailById = {};
    this.mangaDetailOrder = [];
  }

  setMangaMetaBatch(entries: readonly CachedMangaMeta[]) {
    if (entries.length === 0) return;
    const next: Record<number, CachedMangaMeta> = { ...this.mangaMetaById };
    for (const meta of entries) {
      next[meta.id] = meta;
    }
    const keys = Object.keys(next);
    if (keys.length > MAX_META_CACHE_ENTRIES) {
      const overflow = keys.length - MAX_META_CACHE_ENTRIES;
      for (let i = 0; i < overflow; i += 1) {
        delete next[Number(keys[i])];
      }
    }
    this.mangaMetaById = next;
  }

  removeHistoryItem(mangaId: number) {
    this.history = this.history.filter((i) => i.mangaId !== mangaId);
  }

  clearHistory() {
    this.history = [];
    this.historyLoaded = true;
  }

  setBookmarks(sort: BookmarkSort, items: BookmarkItemView[]) {
    this.bookmarksBySort = { ...this.bookmarksBySort, [sort]: items };
    this.bookmarksLoaded = { ...this.bookmarksLoaded, [sort]: true };
    const nextPreviews: Record<number, MangaPreviewSeed> = { ...this.mangaPreviewById };
    for (const item of items) {
      if (!nextPreviews[item.mangaId]) {
        nextPreviews[item.mangaId] = {
          id: item.mangaId,
          title: item.title,
          coverToken: item.coverToken,
          site: item.site,
          isCompleted: item.isCompleted,
          unreadCount: item.unreadCount,
          bookmarked: true,
        };
      }
    }
    this.mangaPreviewById = nextPreviews;
  }

  removeBookmarkItem(mangaId: number) {
    const next: Partial<Record<BookmarkSort, BookmarkItemView[]>> = {};
    for (const [k, list] of Object.entries(this.bookmarksBySort)) {
      if (list) {
        next[k as BookmarkSort] = list.filter((i) => i.mangaId !== mangaId);
      }
    }
    this.bookmarksBySort = next;
  }

  updateMangaCover(mangaId: number, coverToken: string) {
    this.history = this.history.map((item) =>
      item.mangaId === mangaId ? { ...item, coverToken } : item
    );
    const next: Partial<Record<BookmarkSort, BookmarkItemView[]>> = {};
    for (const [k, list] of Object.entries(this.bookmarksBySort)) {
      if (list) {
        next[k as BookmarkSort] = list.map((item) =>
          item.mangaId === mangaId ? { ...item, coverToken } : item
        );
      }
    }
    this.bookmarksBySort = next;
    if (this.mangaPreviewById[mangaId]) {
      this.mangaPreviewById = {
        ...this.mangaPreviewById,
        [mangaId]: { ...this.mangaPreviewById[mangaId], coverToken },
      };
    }
    if (this.mangaDetailById[mangaId]) {
      this.mangaDetailById = {
        ...this.mangaDetailById,
        [mangaId]: { ...this.mangaDetailById[mangaId], coverToken },
      };
    }
  }
}

export const libraryCache = new LibraryCacheStore();
