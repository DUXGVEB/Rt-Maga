import type { DialogFilter } from "@tauri-apps/plugin-dialog";

export const COMIC_ARCHIVE_EXTENSIONS = [
  "cbz",
  "zip",
  "cbr",
  "rar",
  "cb7",
  "7z",
  "cbt",
  "tar",
] as const;

export const COMIC_DOC_EXTENSIONS = ["epub", "pdf"] as const;

export const BACKUP_EXTENSIONS = ["tachibk", "gz"] as const;

export const COMIC_IMAGE_EXTENSIONS = [
  "jpg",
  "jpeg",
  "png",
  "webp",
  "avif",
  "gif",
  "bmp",
] as const;

export const ALL_SUPPORTED_COMIC_EXTENSIONS = [
  ...COMIC_ARCHIVE_EXTENSIONS,
  ...COMIC_DOC_EXTENSIONS,
  ...COMIC_IMAGE_EXTENSIONS,
] as const;

export const ALL_SUPPORTED_UPLOAD_EXTENSIONS = [
  ...ALL_SUPPORTED_COMIC_EXTENSIONS,
  ...BACKUP_EXTENSIONS,
] as const;

export const COMIC_DIALOG_FILTERS: DialogFilter[] = [
  {
    name: "All Supported Comics, Images & Backups",
    extensions: [...ALL_SUPPORTED_COMIC_EXTENSIONS, "tachibk"],
  },
  {
    name: "Mihon / Tachiyomi Backups (TACHIBK, PROTO.GZ)",
    extensions: [...BACKUP_EXTENSIONS],
  },
  {
    name: "Comic Archives (CBZ, CBR, CB7, CBT, ZIP, RAR, 7z, TAR)",
    extensions: [...COMIC_ARCHIVE_EXTENSIONS],
  },
  {
    name: "Ebooks & Documents (EPUB, PDF)",
    extensions: [...COMIC_DOC_EXTENSIONS],
  },
  {
    name: "Images (JPG, PNG, WebP, AVIF, GIF, BMP)",
    extensions: [...COMIC_IMAGE_EXTENSIONS],
  },
];

/**
 * Broad accept string for WebView `<input type="file">`.
 * Includes `application/octet-stream` and `image/*` so Android document pickers never gray out
 * `.cbz`, `.cbr`, `.cb7`, `.cbt`, or `.tachibk` files whose MIME types are unknown to the OS.
 */
export const BROWSER_FILE_ACCEPT = [
  ...ALL_SUPPORTED_COMIC_EXTENSIONS.map((ext) => `.${ext}`),
  ".tachibk",
  ".proto.gz",
  "application/gzip",
  "application/x-gzip",
  "application/vnd.comicbook+zip",
  "application/vnd.comicbook-rar",
  "application/zip",
  "application/x-rar-compressed",
  "application/vnd.rar",
  "application/x-7z-compressed",
  "application/x-tar",
  "application/epub+zip",
  "application/pdf",
  "application/octet-stream",
  "image/*",
].join(",");

/** Returns `true` for locally imported offline comics (including legacy `local:cbz` and catalog entries). */
export function isLocalComicSite(site: string): boolean {
  return site.startsWith("local:");
}

/** Returns `true` for metadata-only catalog entries imported from a `.tachibk` backup without local files yet. */
export function isTachibkCatalogSite(site: string): boolean {
  return site === "local:tachibk";
}

/** Returns `true` when a chapter URL is a metadata-only `.tachibk` stub without linked local pages. */
export function isMetadataOnlyChapterUrl(url: string): boolean {
  return url.startsWith("https://tachibk.local/");
}

/** Returns `true` when a filename carries a Mihon / Tachiyomi backup extension (`.tachibk` or `.proto.gz`). */
export function isBackupFilename(rawName: string): boolean {
  const lower = rawName.trim().toLowerCase();
  return lower.endsWith(".tachibk") || lower.endsWith(".proto.gz") || lower.endsWith(".tachibk.gz");
}

/** Filters OS junk metadata files (`__MACOSX`, `.DS_Store`, `Thumbs.db`, AppleDouble `._*`). */
export function isJunkClientPath(rawPath: string): boolean {
  const normalized = rawPath.replace(/\\/g, "/").trim();
  if (!normalized) return true;
  const segments = normalized.split("/").filter(Boolean);
  for (const seg of segments) {
    const lower = seg.toLowerCase();
    if (
      seg === ".." ||
      seg === "." ||
      lower === "__macosx" ||
      lower === ".ds_store" ||
      lower === "thumbs.db" ||
      lower === "desktop.ini" ||
      lower.startsWith("._") ||
      lower.startsWith(".")
    ) {
      return true;
    }
  }
  return false;
}

/**
 * Checks whether a browser-picked `File` should be streamed to the Rust backend for magic-byte
 * inspection. Allows metadata files (`ComicInfo.xml`, `details.json`) inside folder uploads so
 * series metadata survives Android/WebView folder imports.
 */
export function shouldUploadBrowserFile(file: File, includeFolderMeta = false): boolean {
  const rel = (file.webkitRelativePath || file.name).trim();
  if (isJunkClientPath(rel)) return false;

  const lower = file.name.toLowerCase();
  if (includeFolderMeta && (lower === "comicinfo.xml" || lower === "details.json")) {
    return true;
  }
  if (!includeFolderMeta && isBackupFilename(lower)) {
    return true;
  }
  if (file.type.startsWith("image/")) return true;

  const dotIdx = lower.lastIndexOf(".");
  if (dotIdx < 0) {
    // Extensionless files are still handed to Rust magic-byte detection.
    return true;
  }
  const ext = lower.slice(dotIdx + 1);
  if (!includeFolderMeta && ext === "gz") {
    return true;
  }
  return (ALL_SUPPORTED_COMIC_EXTENSIONS as readonly string[]).includes(ext);
}
