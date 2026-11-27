import type { CachedMangaMeta } from "../depthCarouselMath";
import type { BookmarkItemView, BookmarkSort, HistoryItemView } from "../types";

const MAX_META_CACHE_ENTRIES = 320;

/**
 * Lightweight in-memory snapshot cache for Home, History, Bookmarks, and compact Manga metadata.
 * Prevents the brief skeleton/blurred loading flicker when switching tabs while
 * every visit still performs a silent background refresh from SQLite.
 */
class LibraryCacheStore {
  history = $state<HistoryItemView[]>([]);
  historyLoaded = $state(false);
  bookmarksBySort = $state<Partial<Record<BookmarkSort, BookmarkItemView[]>>>({});
  bookmarksLoaded = $state<Partial<Record<BookmarkSort, boolean>>>({});
  mangaMetaById = $state<Record<number, CachedMangaMeta>>({});

  setHistory(items: HistoryItemView[]) {
    this.history = items;
    this.historyLoaded = true;
  }

  invalidate() {
    this.historyLoaded = false;
    this.bookmarksLoaded = {};
    this.mangaMetaById = {};
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
  }
}

export const libraryCache = new LibraryCacheStore();
