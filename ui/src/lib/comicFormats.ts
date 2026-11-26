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

export const COMIC_DIALOG_FILTERS: DialogFilter[] = [
  {
    name: "All Supported Comics & Images",
    extensions: [...ALL_SUPPORTED_COMIC_EXTENSIONS],
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
 * `.cbz`, `.cbr`, `.cb7`, or `.cbt` files whose MIME types are unknown to the OS.
 */
export const BROWSER_FILE_ACCEPT = [
  ...ALL_SUPPORTED_COMIC_EXTENSIONS.map((ext) => `.${ext}`),
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

/** Returns `true` for locally imported offline comics (including legacy `local:cbz` entries). */
export function isLocalComicSite(site: string): boolean {
  return site.startsWith("local:");
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
  if (file.type.startsWith("image/")) return true;

  const dotIdx = lower.lastIndexOf(".");
  if (dotIdx < 0) {
    // Extensionless files are still handed to Rust magic-byte detection.
    return true;
  }
  const ext = lower.slice(dotIdx + 1);
  return (ALL_SUPPORTED_COMIC_EXTENSIONS as readonly string[]).includes(ext);
}
