import type { BookmarkItemView, BookmarkSort, HistoryItemView } from "../types";

/**
 * Lightweight in-memory snapshot cache for Home, History, and Bookmarks.
 * Prevents the brief skeleton/blurred loading flicker when switching tabs while
 * every visit still performs a silent background refresh from SQLite.
 */
class LibraryCacheStore {
  history = $state<HistoryItemView[]>([]);
  historyLoaded = $state(false);
  bookmarksBySort = $state<Partial<Record<BookmarkSort, BookmarkItemView[]>>>({});
  bookmarksLoaded = $state<Partial<Record<BookmarkSort, boolean>>>({});

  setHistory(items: HistoryItemView[]) {
    this.history = items;
    this.historyLoaded = true;
  }

  invalidate() {
    this.historyLoaded = false;
    this.bookmarksLoaded = {};
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
}

export const libraryCache = new LibraryCacheStore();
