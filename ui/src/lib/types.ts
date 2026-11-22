export type Theme = "system" | "light" | "dark";

export type ImageWidth =
  | { mode: "fit_window" }
  | { mode: "percent"; value: number }
  | { mode: "px"; value: number };

export type ReaderBg = "dark" | "light" | "follow_theme";

export interface ReaderSettings {
  width: ImageWidth;
  gapPx: number;
  preloadImages: number;
  autoNextChapter: boolean;
  readerBg: ReaderBg;
}

export interface UpdateSettings {
  enabled: boolean;
  intervalHours: number;
  notifyWindows: boolean;
}

export interface SourceSettings {
  headlessEnabled: boolean;
  maxConnPerHost: number;
}

export interface CacheSettings {
  maxMb: number;
}

export interface Settings {
  theme: Theme;
  reader: ReaderSettings;
  updates: UpdateSettings;
  sources: SourceSettings;
  cache: CacheSettings;
}

export interface ChapterRefView {
  url: string;
  name: string;
  number: number | null;
  idx: number;
  read: boolean;
}

export interface ProgressView {
  chapterUrl: string;
  chapterName: string;
  scrollRatio: number;
  readAt: number;
}

export interface MangaView {
  id: number;
  url: string;
  site: string;
  title: string;
  coverToken: string | null;
  hasNew: boolean;
  latestChapterName: string | null;
  chapters: ChapterRefView[];
  progress: ProgressView | null;
}

export interface ResolveResponse {
  manga: MangaView;
  startAt: ChapterRefView | null;
}

export interface ChapterView {
  url: string;
  name: string;
  images: string[];
  prev: string | null;
  next: string | null;
  mangaId: number | null;
}

export interface HistoryItemView {
  mangaId: number;
  mangaUrl: string;
  site: string;
  title: string;
  coverToken: string | null;
  chapterUrl: string;
  chapterName: string;
  scrollRatio: number;
  readAt: number;
  latestChapterUrl: string | null;
  latestChapterName: string | null;
  hasNew: boolean;
  totalChapters: number;
  currentChapterIdx: number | null;
}

export interface CacheInfo {
  bytes: number;
  files: number;
}

export interface FoundUpdate {
  mangaId: number;
  title: string;
  latest: string;
}

export interface UpdateProgress {
  done: number;
  total: number;
}

export interface UpdateSummary {
  checked: number;
  found: FoundUpdate[];
}

export type AppError =
  | { kind: "invalid_url"; message: string }
  | { kind: "network"; message: string }
  | { kind: "blocked"; host: string; url: string }
  | { kind: "not_recognized"; message: string }
  | { kind: "not_found"; message: string }
  | { kind: "db"; message: string }
  | { kind: "internal"; message: string };
