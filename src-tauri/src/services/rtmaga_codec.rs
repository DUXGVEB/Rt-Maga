//! Pure-Rust binary framing, `MessagePack` serialization, LZ4 compression, CRC32 integrity
//! validation, and string-interned delta payload encoding for native `.rtmaga` backups.
//!
//! # Binary Framing Specification (Big-Endian, 24-Byte Fixed Header)
//!
//! | Offset | Size | Field |
//! |---|---|---|
//! | `0..4` | 4B | Magic: `RTMG` (`0x52 0x54 0x4D 0x47`) |
//! | `4..6` | 2B | Format version (`u16`, currently `1`) |
//! | `6..8` | 2B | Flags bitmask (`u16`: compression + encryption bits) |
//! | `8..16` | 8B | Creation timestamp (`i64`, Unix milliseconds) |
//! | `16..20` | 4B | Compressed payload byte length (`u32`) |
//! | `20..24` | 4B | Checksum (`u32`, CRC32 computed over compressed payload) |
//! | `24..` | `N` | Payload bytes (MessagePack, LZ4-compressed when `FLAG_COMPRESSION_LZ4` is set) |

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Chunk size used when streaming `.rtmaga` payload bytes and computing rolling CRC32 (`64 KiB`).
pub(crate) const STREAM_CHUNK_BYTES: usize = 64 * 1024;

/// Fixed 4-byte `.rtmaga` file magic (`RTMG`).
pub(crate) const RTMAGA_MAGIC: [u8; 4] = *b"RTMG";

/// Fixed binary header length in bytes.
pub(crate) const RTMAGA_HEADER_LEN: usize = 24;

/// Currently supported `.rtmaga` binary format version.
pub(crate) const RTMAGA_VERSION_V1: u16 = 1;

/// MIME type associated with `.rtmaga` backups.
#[cfg(target_os = "android")]
pub(crate) const RTMAGA_MIME_TYPE: &str = "application/vnd.rtmaga.backup";

/// Compression bits (`0..=1`): `0` = uncompressed, `1` = LZ4 (`lz4_flex`).
pub(crate) const FLAG_COMPRESSION_MASK: u16 = 0x0003;

/// Payload is compressed with LZ4 (`lz4_flex::compress_prepend_size`).
pub(crate) const FLAG_COMPRESSION_LZ4: u16 = 0x0001;

/// Reserved bit indicating encrypted payload (rejected cleanly by v1 unencrypted reader).
pub(crate) const FLAG_ENCRYPTED: u16 = 0x0004;

/// Mask of all flag bits recognized by this codec version.
pub(crate) const SUPPORTED_FLAGS_MASK: u16 = FLAG_COMPRESSION_MASK | FLAG_ENCRYPTED;

/// Hard ceiling on compressed backup payload bytes (32 MiB).
pub(crate) const MAX_COMPRESSED_PAYLOAD_BYTES: usize = 32 * 1024 * 1024;

/// Hard ceiling on decompressed `MessagePack` payload bytes (64 MiB) to stop decompression bombs.
pub(crate) const MAX_DECOMPRESSED_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;

/// Bitmask constants for [`BackupBlockSelection`].
pub(crate) const BLOCK_MANGA: u8 = 1 << 0;
pub(crate) const BLOCK_TRACKERS: u8 = 1 << 1;
pub(crate) const BLOCK_PREFS: u8 = 1 << 2;

/// Structured codec and 3-stage pre-flight validation errors for `.rtmaga` files.
#[derive(Debug, Error)]
pub(crate) enum CodecError {
    #[error("backup file is truncated ({actual} bytes, expected at least {RTMAGA_HEADER_LEN})")]
    TruncatedHeader { actual: usize },

    #[error("invalid .rtmaga magic header (expected RTMG)")]
    InvalidMagic,

    #[error("unsupported .rtmaga format version {version} (supported: {RTMAGA_VERSION_V1})")]
    UnsupportedVersion { version: u16 },

    #[error("encrypted .rtmaga backups are not supported by this version")]
    EncryptedUnsupported,

    #[error("unsupported .rtmaga flags bitmask 0x{flags:04x}")]
    UnsupportedFlags { flags: u16 },

    #[error("payload size mismatch: header declares {declared} bytes, file has {actual} bytes")]
    PayloadSizeMismatch { declared: usize, actual: usize },

    #[error("backup payload exceeds maximum permitted size ({size} > {limit} bytes)")]
    PayloadTooLarge { size: usize, limit: usize },

    #[error(
        "backup checksum verification failed (expected 0x{expected:08x}, computed 0x{actual:08x})"
    )]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("LZ4 decompression failed: {0}")]
    DecompressFailed(String),

    #[error("MessagePack encoding failed: {0}")]
    EncodeFailed(String),

    #[error("MessagePack decoding failed: {0}")]
    DecodeFailed(String),

    #[error("backup stream I/O read failed: {0}")]
    IoError(String),
}

/// Decoded 24-byte big-endian `.rtmaga` header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RtmagaHeader {
    pub version: u16,
    pub flags: u16,
    pub timestamp_ms: i64,
    pub payload_size: u32,
    pub checksum: u32,
}

/// Which top-level backup blocks are included when exporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct BackupBlockSelection {
    /// Library entries, bookmarks, reading history, and delta chapter progress.
    pub manga: bool,
    /// Saved sources & tracker configurations.
    pub trackers: bool,
    /// Sanitized app & reader settings (auth tokens/cookies stripped).
    pub prefs: bool,
}

impl Default for BackupBlockSelection {
    fn default() -> Self {
        Self {
            manga: true,
            trackers: true,
            prefs: true,
        }
    }
}

impl BackupBlockSelection {
    /// Packs selection flags into a compact bitmask stored in [`BackupMeta`].
    pub(crate) const fn to_mask(self) -> u8 {
        let mut mask = 0_u8;
        if self.manga {
            mask |= BLOCK_MANGA;
        }
        if self.trackers {
            mask |= BLOCK_TRACKERS;
        }
        if self.prefs {
            mask |= BLOCK_PREFS;
        }
        mask
    }

    /// Returns `true` when at least one block is selected.
    pub(crate) const fn any_selected(self) -> bool {
        self.manga || self.trackers || self.prefs
    }
}

/// Compact backup metadata header inside the `MessagePack` payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BackupMeta {
    /// Application version that produced the backup.
    #[serde(rename = "av")]
    pub app_version: String,
    /// Source device display name.
    #[serde(rename = "dv")]
    pub device_name: String,
    /// Creation timestamp in Unix milliseconds.
    #[serde(rename = "ts")]
    pub exported_at_ms: i64,
    /// Bitmask of blocks included (`BLOCK_MANGA | BLOCK_TRACKERS | BLOCK_PREFS`).
    #[serde(rename = "bl")]
    pub blocks_mask: u8,
}

/// Interned string tables deduplicating source identifiers and categories/genres across entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BackupDict {
    /// Deduplicated source IDs / site keys referenced by numeric index `si`.
    #[serde(rename = "s", default)]
    pub sources: Vec<String>,
    /// Deduplicated category / genre / tag names referenced by numeric index `ci` / `ti`.
    #[serde(rename = "c", default)]
    pub categories: Vec<String>,
}

/// Helper builder for deterministic string interning into [`BackupDict`].
#[derive(Debug, Default)]
pub(crate) struct DictInterner {
    sources_map: HashMap<String, u32>,
    categories_map: HashMap<String, u32>,
    dict: BackupDict,
}

impl DictInterner {
    /// Creates an empty interner.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Interns a source identifier and returns its compact numeric index.
    pub(crate) fn intern_source(&mut self, source: &str) -> u32 {
        let key = source.trim();
        if let Some(&idx) = self.sources_map.get(key) {
            return idx;
        }
        let idx = u32::try_from(self.dict.sources.len()).unwrap_or(u32::MAX);
        self.dict.sources.push(key.to_owned());
        self.sources_map.insert(key.to_owned(), idx);
        idx
    }

    /// Interns a category/genre/tag label and returns its compact numeric index.
    pub(crate) fn intern_category(&mut self, label: &str) -> u32 {
        let key = label.trim();
        if let Some(&idx) = self.categories_map.get(key) {
            return idx;
        }
        let idx = u32::try_from(self.dict.categories.len()).unwrap_or(u32::MAX);
        self.dict.categories.push(key.to_owned());
        self.categories_map.insert(key.to_owned(), idx);
        idx
    }

    /// Consumes the interner and yields the finalized [`BackupDict`].
    pub(crate) fn finish(self) -> BackupDict {
        self.dict
    }
}

/// Compact delta chapter tuple `[chapterKey, isRead, lastPageRead, lastReadAt]`.
///
/// Only interacted chapters (marked read, currently in progress, or active) are stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct DeltaChapter(
    /// Chapter key / canonical chapter URL.
    pub String,
    /// `true` if chapter has been marked read.
    pub bool,
    /// Zero-based last page read (`>= 0`).
    pub i64,
    /// Last-read Unix timestamp in milliseconds (`>= 0`).
    pub i64,
);

/// Optional active reading history pointer for restoring scroll position and chapter title.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BackupHistoryPointer {
    /// Active chapter key / URL.
    #[serde(rename = "k")]
    pub chapter_key: String,
    /// Active chapter display name.
    #[serde(rename = "n")]
    pub chapter_name: String,
    /// Normalized scroll ratio (`0.0..=1.0`).
    #[serde(rename = "r")]
    pub scroll_ratio: f64,
    /// Zero-based page index (`>= 0`).
    #[serde(rename = "p")]
    pub last_page_read: i64,
    /// Last-read Unix timestamp in milliseconds.
    #[serde(rename = "t")]
    pub last_read_at_ms: i64,
}

/// Compact manga record stored inside the `manga` block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BackupMangaEntry {
    /// Interned index into `dict.sources` (`sourceId`).
    #[serde(rename = "si")]
    pub source_idx: u32,
    /// Primary manga URL (composite key with `sourceId`).
    #[serde(rename = "u")]
    pub url: String,
    /// Canonical URL if distinct.
    #[serde(rename = "cu", default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    /// Optional upstream source-specific ID.
    #[serde(rename = "sid", default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Manga display title.
    #[serde(rename = "t")]
    pub title: String,
    /// Cover URL reference only (never binary blobs).
    #[serde(rename = "cov", default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    /// Latest known chapter URL.
    #[serde(rename = "lcu", default, skip_serializing_if = "Option::is_none")]
    pub latest_chapter_url: Option<String>,
    /// Latest known chapter display name.
    #[serde(rename = "lcn", default, skip_serializing_if = "Option::is_none")]
    pub latest_chapter_name: Option<String>,
    /// Whether the manga is bookmarked in library.
    #[serde(rename = "bm", default)]
    pub bookmarked: bool,
    /// Bookmark creation timestamp (Unix ms).
    #[serde(rename = "bma", default, skip_serializing_if = "Option::is_none")]
    pub bookmarked_at_ms: Option<i64>,
    /// Whether the manga is hidden from standard listings.
    #[serde(rename = "hid", default)]
    pub hidden: bool,
    /// Manual per-manga reader mode override, if set.
    #[serde(rename = "rmo", default, skip_serializing_if = "Option::is_none")]
    pub reader_mode_override: Option<String>,
    /// Serialized per-manga reader settings JSON, if set.
    #[serde(rename = "rsj", default, skip_serializing_if = "Option::is_none")]
    pub reader_settings_json: Option<String>,
    /// Persisted auto-detected webtoon flag.
    #[serde(rename = "dw", default)]
    pub detected_webtoon: bool,
    /// Persisted multi-signal layout classification (`"manga"` | `"webtoon"`), backward-compatible.
    #[serde(rename = "dl", default, skip_serializing_if = "Option::is_none")]
    pub detected_layout: Option<String>,
    /// Interned genre indices into `dict.categories`.
    #[serde(rename = "gi", default, skip_serializing_if = "Vec::is_empty")]
    pub genre_indices: Vec<u32>,
    /// Interned tag indices into `dict.categories`.
    #[serde(rename = "ti", default, skip_serializing_if = "Vec::is_empty")]
    pub tag_indices: Vec<u32>,
    /// Authors list.
    #[serde(rename = "au", default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    /// Artists list.
    #[serde(rename = "ar", default, skip_serializing_if = "Vec::is_empty")]
    pub artists: Vec<String>,
    /// Synopsis text.
    #[serde(rename = "ds", default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Publication status.
    #[serde(rename = "st", default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Language code.
    #[serde(rename = "lg", default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Creation timestamp (Unix ms).
    #[serde(rename = "ca", default, skip_serializing_if = "Option::is_none")]
    pub created_at_ms: Option<i64>,
    /// Delta interacted chapter tuples `[chapterKey, isRead, lastPageRead, lastReadAt]`.
    #[serde(rename = "ch", default, skip_serializing_if = "Vec::is_empty")]
    pub chapters: Vec<DeltaChapter>,
    /// Active reading progress pointer, if present.
    #[serde(rename = "h", default, skip_serializing_if = "Option::is_none")]
    pub history: Option<BackupHistoryPointer>,
}

/// Saved source / tracker definition inside the `trackers` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BackupTrackerEntry {
    #[serde(rename = "sk")]
    pub source_key: String,
    #[serde(rename = "dn")]
    pub display_name: String,
    #[serde(rename = "bu")]
    pub base_url: String,
    #[serde(rename = "kd")]
    pub source_kind: String,
    #[serde(rename = "pr", default, skip_serializing_if = "Option::is_none")]
    pub parser_ref: Option<String>,
    #[serde(rename = "st", default, skip_serializing_if = "Option::is_none")]
    pub search_template: Option<String>,
    #[serde(rename = "ic", default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(rename = "en", default)]
    pub is_enabled: bool,
    #[serde(rename = "sp", default)]
    pub is_supplemental: bool,
    #[serde(rename = "mv", default)]
    pub manual_visibility: bool,
}

/// Sanitized preferences payload (strictly free of unencrypted auth tokens, passwords, and cookies).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BackupPrefs {
    /// Serialized, credential-stripped [`crate::settings::Settings`] JSON.
    #[serde(rename = "sj")]
    pub settings_json: String,
}

/// Root `MessagePack` payload carrying all 5 logical `.rtmaga` blocks (`meta`, `dict`, `manga`,
/// `trackers`, `prefs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct RtmagaPayload {
    #[serde(rename = "meta")]
    pub meta: BackupMeta,
    #[serde(rename = "dict", default)]
    pub dict: BackupDict,
    #[serde(rename = "manga", default, skip_serializing_if = "Vec::is_empty")]
    pub manga: Vec<BackupMangaEntry>,
    #[serde(rename = "trackers", default, skip_serializing_if = "Vec::is_empty")]
    pub trackers: Vec<BackupTrackerEntry>,
    #[serde(rename = "prefs", default, skip_serializing_if = "Option::is_none")]
    pub prefs: Option<BackupPrefs>,
}

/// Complete validated and decoded `.rtmaga` archive.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DecodedRtmagaBackup {
    pub header: RtmagaHeader,
    pub payload: RtmagaPayload,
    pub compressed_bytes: usize,
    pub uncompressed_bytes: usize,
}

/// Encodes [`RtmagaPayload`] into the complete 24-byte-framed, LZ4-compressed, CRC32-checksummed
/// `.rtmaga` binary format.
///
/// # Errors
///
/// Returns [`CodecError`] if `MessagePack` encoding fails or the payload exceeds hard size limits.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "in-memory encoder companion to write_encoded_to used in codec and backup tests"
    )
)]
pub(crate) fn encode_backup(
    payload: &RtmagaPayload,
) -> Result<(Vec<u8>, RtmagaHeader), CodecError> {
    let raw_msgpack =
        rmp_serde::to_vec_named(payload).map_err(|e| CodecError::EncodeFailed(e.to_string()))?;

    if raw_msgpack.len() > MAX_DECOMPRESSED_PAYLOAD_BYTES {
        return Err(CodecError::PayloadTooLarge {
            size: raw_msgpack.len(),
            limit: MAX_DECOMPRESSED_PAYLOAD_BYTES,
        });
    }

    let compressed = lz4_flex::compress_prepend_size(&raw_msgpack);
    if compressed.len() > MAX_COMPRESSED_PAYLOAD_BYTES {
        return Err(CodecError::PayloadTooLarge {
            size: compressed.len(),
            limit: MAX_COMPRESSED_PAYLOAD_BYTES,
        });
    }

    let payload_size =
        u32::try_from(compressed.len()).map_err(|_| CodecError::PayloadTooLarge {
            size: compressed.len(),
            limit: MAX_COMPRESSED_PAYLOAD_BYTES,
        })?;
    let checksum = crc32fast::hash(&compressed);

    let header = RtmagaHeader {
        version: RTMAGA_VERSION_V1,
        flags: FLAG_COMPRESSION_LZ4,
        timestamp_ms: payload.meta.exported_at_ms,
        payload_size,
        checksum,
    };

    let mut out = Vec::with_capacity(RTMAGA_HEADER_LEN + compressed.len());
    write_header_to_buf(&header, &mut out);
    out.extend_from_slice(&compressed);

    Ok((out, header))
}

/// Writes an encoded `.rtmaga` backup directly to a buffered writer without duplicating the buffer.
///
/// # Errors
///
/// Returns [`CodecError`] on encoding limits or I/O write failure.
pub(crate) fn write_encoded_to<W: Write>(
    payload: &RtmagaPayload,
    writer: &mut W,
) -> Result<(RtmagaHeader, usize, usize), CodecError> {
    let raw_msgpack =
        rmp_serde::to_vec_named(payload).map_err(|e| CodecError::EncodeFailed(e.to_string()))?;

    if raw_msgpack.len() > MAX_DECOMPRESSED_PAYLOAD_BYTES {
        return Err(CodecError::PayloadTooLarge {
            size: raw_msgpack.len(),
            limit: MAX_DECOMPRESSED_PAYLOAD_BYTES,
        });
    }

    let compressed = lz4_flex::compress_prepend_size(&raw_msgpack);
    if compressed.len() > MAX_COMPRESSED_PAYLOAD_BYTES {
        return Err(CodecError::PayloadTooLarge {
            size: compressed.len(),
            limit: MAX_COMPRESSED_PAYLOAD_BYTES,
        });
    }

    let payload_size =
        u32::try_from(compressed.len()).map_err(|_| CodecError::PayloadTooLarge {
            size: compressed.len(),
            limit: MAX_COMPRESSED_PAYLOAD_BYTES,
        })?;
    let checksum = crc32fast::hash(&compressed);
    let header = RtmagaHeader {
        version: RTMAGA_VERSION_V1,
        flags: FLAG_COMPRESSION_LZ4,
        timestamp_ms: payload.meta.exported_at_ms,
        payload_size,
        checksum,
    };

    let mut hdr_bytes = [0_u8; RTMAGA_HEADER_LEN];
    encode_header_array(&header, &mut hdr_bytes);

    writer
        .write_all(&hdr_bytes)
        .and_then(|()| writer.write_all(&compressed))
        .and_then(|()| writer.flush())
        .map_err(|e| CodecError::EncodeFailed(format!("stream write failed: {e}")))?;

    Ok((header, compressed.len(), raw_msgpack.len()))
}

/// Parses and validates the fixed 24-byte big-endian `.rtmaga` header **before** allocating any
/// payload buffer:
/// 1. Magic === `RTMG`
/// 2. Supported `Version` & `Flags`
/// 3. Declared payload size within [`MAX_COMPRESSED_PAYLOAD_BYTES`]
///
/// # Errors
///
/// Returns [`CodecError`] on invalid magic, unsupported version/flags, or oversized payload length.
pub(crate) fn parse_and_validate_header(
    bytes: &[u8; RTMAGA_HEADER_LEN],
) -> Result<RtmagaHeader, CodecError> {
    // Stage 1: Magic === RTMG
    if bytes[0..4] != RTMAGA_MAGIC {
        return Err(CodecError::InvalidMagic);
    }

    let version = u16::from_be_bytes([bytes[4], bytes[5]]);
    let flags = u16::from_be_bytes([bytes[6], bytes[7]]);
    let timestamp_ms = i64::from_be_bytes([
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    ]);
    let payload_size = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let checksum = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);

    // Stage 2: Supported Version & Flags
    if version != RTMAGA_VERSION_V1 {
        return Err(CodecError::UnsupportedVersion { version });
    }
    if (flags & FLAG_ENCRYPTED) != 0 {
        return Err(CodecError::EncryptedUnsupported);
    }
    if (flags & !SUPPORTED_FLAGS_MASK) != 0 {
        return Err(CodecError::UnsupportedFlags { flags });
    }
    let comp_mode = flags & FLAG_COMPRESSION_MASK;
    if comp_mode != 0 && comp_mode != FLAG_COMPRESSION_LZ4 {
        return Err(CodecError::UnsupportedFlags { flags });
    }

    // Stage 3a: Validate declared payload size ceiling BEFORE allocating any payload buffer.
    let declared = usize::try_from(payload_size).map_err(|_| CodecError::PayloadTooLarge {
        size: usize::MAX,
        limit: MAX_COMPRESSED_PAYLOAD_BYTES,
    })?;
    if declared > MAX_COMPRESSED_PAYLOAD_BYTES {
        return Err(CodecError::PayloadTooLarge {
            size: declared,
            limit: MAX_COMPRESSED_PAYLOAD_BYTES,
        });
    }

    Ok(RtmagaHeader {
        version,
        flags,
        timestamp_ms,
        payload_size,
        checksum,
    })
}

/// Runs the strict 3-stage pre-flight validation on raw `.rtmaga` bytes **before** any
/// decompression or database mutation:
/// 1. Magic === `RTMG`
/// 2. Supported `Version` & `Flags`
/// 3. Exact `Payload Size` & `Checksum` match
///
/// # Errors
///
/// Returns [`CodecError`] describing the exact stage that failed.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "in-memory pre-flight companion to decode_backup_from_reader used in tests"
    )
)]
pub(crate) fn preflight_validate(bytes: &[u8]) -> Result<(RtmagaHeader, &[u8]), CodecError> {
    if bytes.len() < RTMAGA_HEADER_LEN {
        return Err(CodecError::TruncatedHeader {
            actual: bytes.len(),
        });
    }

    let hdr_arr: &[u8; RTMAGA_HEADER_LEN] =
        bytes[0..RTMAGA_HEADER_LEN]
            .try_into()
            .map_err(|_| CodecError::TruncatedHeader {
                actual: bytes.len(),
            })?;
    let header = parse_and_validate_header(hdr_arr)?;
    let declared =
        usize::try_from(header.payload_size).map_err(|_| CodecError::PayloadTooLarge {
            size: usize::MAX,
            limit: MAX_COMPRESSED_PAYLOAD_BYTES,
        })?;

    let payload_slice = &bytes[RTMAGA_HEADER_LEN..];
    if payload_slice.len() != declared {
        return Err(CodecError::PayloadSizeMismatch {
            declared,
            actual: payload_slice.len(),
        });
    }

    let computed = crc32fast::hash(payload_slice);
    if computed != header.checksum {
        return Err(CodecError::ChecksumMismatch {
            expected: header.checksum,
            actual: computed,
        });
    }

    Ok((header, payload_slice))
}

/// Fail-fast transactional streaming reader for `.rtmaga` archives:
/// 1. Reads the exact 24-byte header FIRST (failing on short/truncated header without allocation).
/// 2. Validates magic `RTMG`, format version, flags, and payload size ceiling BEFORE allocating.
/// 3. Streams the exact declared payload bytes in 64 KiB chunks while updating a rolling CRC32.
/// 4. Immediately rejects unexpected EOF, trailing garbage bytes, I/O errors, or CRC32 mismatches,
///    dropping buffers immediately on failure.
///
/// # Errors
///
/// Returns [`CodecError`] on any header, size, EOF, trailing-data, I/O, CRC32, decompression,
/// or `MessagePack` decoding failure.
pub(crate) fn decode_backup_from_reader<R: Read>(
    reader: &mut R,
) -> Result<DecodedRtmagaBackup, CodecError> {
    // Step 1: Read exact 24-byte header before any payload allocation.
    let mut hdr_bytes = [0_u8; RTMAGA_HEADER_LEN];
    let mut hdr_read = 0_usize;
    while hdr_read < RTMAGA_HEADER_LEN {
        match reader.read(&mut hdr_bytes[hdr_read..]) {
            Ok(0) => {
                return Err(CodecError::TruncatedHeader { actual: hdr_read });
            }
            Ok(n) => {
                hdr_read += n;
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                return Err(CodecError::IoError(err.to_string()));
            }
        }
    }

    // Step 2: Validate header magic, version, flags, and payload size limit before allocation.
    let header = parse_and_validate_header(&hdr_bytes)?;
    let declared =
        usize::try_from(header.payload_size).map_err(|_| CodecError::PayloadTooLarge {
            size: usize::MAX,
            limit: MAX_COMPRESSED_PAYLOAD_BYTES,
        })?;

    // Step 3: Stream-read exact declared payload bytes in 64 KiB chunks with rolling CRC32.
    // Cap initial reservation to one chunk so forged large headers on truncated files never OOM.
    let mut payload_buf = Vec::with_capacity(declared.min(STREAM_CHUNK_BYTES));
    let mut chunk_buf = vec![0_u8; declared.clamp(1, STREAM_CHUNK_BYTES)];
    let mut hasher = crc32fast::Hasher::new();

    while payload_buf.len() < declared {
        let remaining = declared - payload_buf.len();
        let to_read = remaining.min(chunk_buf.len());
        match reader.read(&mut chunk_buf[..to_read]) {
            Ok(0) => {
                let actual = payload_buf.len();
                drop(payload_buf);
                drop(chunk_buf);
                return Err(CodecError::PayloadSizeMismatch { declared, actual });
            }
            Ok(n) => {
                hasher.update(&chunk_buf[..n]);
                payload_buf.extend_from_slice(&chunk_buf[..n]);
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                drop(payload_buf);
                drop(chunk_buf);
                return Err(CodecError::IoError(err.to_string()));
            }
        }
    }
    drop(chunk_buf);

    // Step 4: Verify exact EOF (fail immediately on any trailing garbage bytes).
    let mut trailing = [0_u8; 1];
    loop {
        match reader.read(&mut trailing) {
            Ok(0) => break,
            Ok(n) => {
                drop(payload_buf);
                return Err(CodecError::PayloadSizeMismatch {
                    declared,
                    actual: declared.saturating_add(n),
                });
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) => {
                drop(payload_buf);
                return Err(CodecError::IoError(err.to_string()));
            }
        }
    }

    // Step 5: Verify rolling CRC32 checksum before any decompression or MessagePack parsing.
    let computed = hasher.finalize();
    if computed != header.checksum {
        drop(payload_buf);
        return Err(CodecError::ChecksumMismatch {
            expected: header.checksum,
            actual: computed,
        });
    }

    decode_verified_payload(header, &payload_buf)
}

/// Validates, decompresses (with decompression-bomb ceiling), and decodes a `.rtmaga` archive.
///
/// # Errors
///
/// Returns [`CodecError`] on any pre-flight failure, decompression bomb, or corrupt `MessagePack`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "in-memory decoder companion to decode_backup_from_reader used in tests"
    )
)]
pub(crate) fn decode_backup(bytes: &[u8]) -> Result<DecodedRtmagaBackup, CodecError> {
    let (header, compressed_slice) = preflight_validate(bytes)?;
    decode_verified_payload(header, compressed_slice)
}

fn decode_verified_payload(
    header: RtmagaHeader,
    compressed_slice: &[u8],
) -> Result<DecodedRtmagaBackup, CodecError> {
    let raw_msgpack: Vec<u8> = match header.flags & FLAG_COMPRESSION_MASK {
        0 => {
            if compressed_slice.len() > MAX_DECOMPRESSED_PAYLOAD_BYTES {
                return Err(CodecError::PayloadTooLarge {
                    size: compressed_slice.len(),
                    limit: MAX_DECOMPRESSED_PAYLOAD_BYTES,
                });
            }
            compressed_slice.to_vec()
        }
        FLAG_COMPRESSION_LZ4 => {
            if compressed_slice.len() < 4 {
                return Err(CodecError::DecompressFailed(
                    "LZ4 payload missing 4-byte uncompressed length prefix".to_owned(),
                ));
            }
            let declared_uncompressed = u32::from_le_bytes([
                compressed_slice[0],
                compressed_slice[1],
                compressed_slice[2],
                compressed_slice[3],
            ]);
            let declared_usize = usize::try_from(declared_uncompressed)
                .unwrap_or(MAX_DECOMPRESSED_PAYLOAD_BYTES + 1);
            if declared_usize > MAX_DECOMPRESSED_PAYLOAD_BYTES {
                return Err(CodecError::PayloadTooLarge {
                    size: declared_usize,
                    limit: MAX_DECOMPRESSED_PAYLOAD_BYTES,
                });
            }
            lz4_flex::decompress_size_prepended(compressed_slice)
                .map_err(|e| CodecError::DecompressFailed(e.to_string()))?
        }
        _ => {
            return Err(CodecError::UnsupportedFlags {
                flags: header.flags,
            });
        }
    };

    let uncompressed_bytes = raw_msgpack.len();
    let payload: RtmagaPayload =
        rmp_serde::from_slice(&raw_msgpack).map_err(|e| CodecError::DecodeFailed(e.to_string()))?;

    Ok(DecodedRtmagaBackup {
        header,
        payload,
        compressed_bytes: compressed_slice.len(),
        uncompressed_bytes,
    })
}

/// Formats the canonical `.rtmaga` export filename `rtmaga_YYYYMMDD_HHmm.rtmaga` from Unix ms (UTC).
pub(crate) fn format_backup_filename(timestamp_ms: i64) -> String {
    let secs = timestamp_ms.div_euclid(1000);
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let (year, month, day) = civil_from_days(days);
    format!("rtmaga_{year:04}{month:02}{day:02}_{hour:02}{minute:02}.rtmaga")
}

/// Howard Hinnant's civil-date conversion from days since `1970-01-01` (zero external deps).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u64::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = i64::try_from(yoe).unwrap_or(0) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (
        year,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

fn encode_header_array(header: &RtmagaHeader, dst: &mut [u8; RTMAGA_HEADER_LEN]) {
    dst[0..4].copy_from_slice(&RTMAGA_MAGIC);
    dst[4..6].copy_from_slice(&header.version.to_be_bytes());
    dst[6..8].copy_from_slice(&header.flags.to_be_bytes());
    dst[8..16].copy_from_slice(&header.timestamp_ms.to_be_bytes());
    dst[16..20].copy_from_slice(&header.payload_size.to_be_bytes());
    dst[20..24].copy_from_slice(&header.checksum.to_be_bytes());
}

fn write_header_to_buf(header: &RtmagaHeader, buf: &mut Vec<u8>) {
    let mut arr = [0_u8; RTMAGA_HEADER_LEN];
    encode_header_array(header, &mut arr);
    buf.extend_from_slice(&arr);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> RtmagaPayload {
        let mut interner = DictInterner::new();
        let src_idx = interner.intern_source("example.com");
        let cat_action = interner.intern_category("Action");
        let cat_webtoon = interner.intern_category("Webtoon");

        RtmagaPayload {
            meta: BackupMeta {
                app_version: "3.2.0".to_owned(),
                device_name: "TestRig".to_owned(),
                exported_at_ms: 1_764_220_800_000,
                blocks_mask: BackupBlockSelection::default().to_mask(),
            },
            dict: interner.finish(),
            manga: vec![BackupMangaEntry {
                source_idx: src_idx,
                url: "https://example.com/series/alpha".to_owned(),
                canonical_url: None,
                source_id: Some("alpha-1".to_owned()),
                title: "Alpha Chronicle".to_owned(),
                cover_url: Some("https://cdn.example.com/alpha.jpg".to_owned()),
                latest_chapter_url: Some("https://example.com/series/alpha/ch-2".to_owned()),
                latest_chapter_name: Some("Chapter 2".to_owned()),
                bookmarked: true,
                bookmarked_at_ms: Some(1_764_220_000_000),
                hidden: false,
                reader_mode_override: None,
                reader_settings_json: None,
                detected_webtoon: true,
                detected_layout: Some("webtoon".to_owned()),
                genre_indices: vec![cat_action, cat_webtoon],
                tag_indices: vec![],
                authors: vec!["Author A".to_owned()],
                artists: vec![],
                description: Some("Synthetic test series".to_owned()),
                status: Some("ongoing".to_owned()),
                language: Some("en".to_owned()),
                created_at_ms: Some(1_764_200_000_000),
                chapters: vec![
                    DeltaChapter(
                        "https://example.com/series/alpha/ch-1".to_owned(),
                        true,
                        18,
                        1_764_210_000_000,
                    ),
                    DeltaChapter(
                        "https://example.com/series/alpha/ch-2".to_owned(),
                        false,
                        7,
                        1_764_220_000_000,
                    ),
                ],
                history: Some(BackupHistoryPointer {
                    chapter_key: "https://example.com/series/alpha/ch-2".to_owned(),
                    chapter_name: "Chapter 2".to_owned(),
                    scroll_ratio: 0.45,
                    last_page_read: 7,
                    last_read_at_ms: 1_764_220_000_000,
                }),
            }],
            trackers: Vec::new(),
            prefs: Some(BackupPrefs {
                settings_json: "{}".to_owned(),
            }),
        }
    }

    #[test]
    fn encodes_big_endian_24_byte_header_and_round_trips() {
        let payload = sample_payload();
        let (bytes, hdr) = encode_backup(&payload).expect("encode must succeed");

        assert_eq!(&bytes[0..4], b"RTMG", "magic bytes must equal RTMG");
        assert_eq!(
            u16::from_be_bytes([bytes[4], bytes[5]]),
            1,
            "version must be 1 big-endian"
        );
        assert_eq!(
            u16::from_be_bytes([bytes[6], bytes[7]]),
            FLAG_COMPRESSION_LZ4,
            "LZ4 flag must be set"
        );
        assert_eq!(
            i64::from_be_bytes(bytes[8..16].try_into().expect("8B slice")),
            payload.meta.exported_at_ms,
            "timestamp must match big-endian unix ms"
        );
        assert_eq!(
            usize::try_from(hdr.payload_size).expect("u32->usize") + RTMAGA_HEADER_LEN,
            bytes.len(),
            "header payload size must match exact file length"
        );

        let decoded = decode_backup(&bytes).expect("decode must round-trip cleanly");
        assert_eq!(decoded.header, hdr, "headers must match");
        assert_eq!(decoded.payload, payload, "decoded payload must match");
    }

    #[test]
    fn preflight_rejects_bad_magic_version_flags_size_and_single_bit_tamper() {
        let (valid, _) = encode_backup(&sample_payload()).expect("encode must succeed");

        // Stage 1: Bad magic
        let mut bad_magic = valid.clone();
        bad_magic[0] = b'X';
        assert!(
            matches!(decode_backup(&bad_magic), Err(CodecError::InvalidMagic)),
            "mutated magic must fail Stage 1"
        );

        // Stage 2: Unsupported version
        let mut bad_ver = valid.clone();
        bad_ver[4..6].copy_from_slice(&99_u16.to_be_bytes());
        assert!(
            matches!(
                decode_backup(&bad_ver),
                Err(CodecError::UnsupportedVersion { version: 99 })
            ),
            "unknown version must fail Stage 2"
        );

        // Stage 2: Encrypted flag rejected cleanly
        let mut enc_flag = valid.clone();
        enc_flag[6..8].copy_from_slice(&(FLAG_COMPRESSION_LZ4 | FLAG_ENCRYPTED).to_be_bytes());
        assert!(
            matches!(
                decode_backup(&enc_flag),
                Err(CodecError::EncryptedUnsupported)
            ),
            "encrypted flag must fail Stage 2"
        );

        // Stage 3: Payload truncated
        let truncated = &valid[..valid.len() - 1];
        assert!(
            matches!(
                decode_backup(truncated),
                Err(CodecError::PayloadSizeMismatch { .. })
            ),
            "truncated payload must fail Stage 3 size check"
        );

        // Stage 3: Single-byte payload tamper
        let mut tampered = valid.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(
            matches!(
                decode_backup(&tampered),
                Err(CodecError::ChecksumMismatch { .. })
            ),
            "1-byte payload tamper must fail Stage 3 CRC32 checksum"
        );
    }

    #[test]
    fn formats_canonical_backup_filename_utc() {
        // 2026-03-15 08:05:00 UTC -> 1773561900000 ms
        assert_eq!(
            format_backup_filename(1_773_561_900_000),
            "rtmaga_20260315_0805.rtmaga",
            "filename must follow rtmaga_YYYYMMDD_HHmm.rtmaga"
        );
    }

    struct HeaderOnlyGuardReader {
        data: Vec<u8>,
        pos: usize,
        max_allowed: usize,
        exceeded_limit: bool,
    }

    impl Read for HeaderOnlyGuardReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.max_allowed {
                self.exceeded_limit = true;
                return Err(std::io::Error::other(
                    "reader attempted to read past 24-byte header",
                ));
            }
            let avail = self.data.len().saturating_sub(self.pos);
            let allowed = self.max_allowed.saturating_sub(self.pos);
            let n = buf.len().min(avail).min(allowed);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    struct FailAfterBytesReader {
        data: Vec<u8>,
        pos: usize,
        fail_at: usize,
    }

    impl Read for FailAfterBytesReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.pos >= self.fail_at {
                return Err(std::io::Error::new(
                    ErrorKind::BrokenPipe,
                    "simulated mid-stream I/O error",
                ));
            }
            let remaining = self
                .fail_at
                .saturating_sub(self.pos)
                .min(self.data.len() - self.pos);
            let n = buf.len().min(remaining);
            buf[..n].copy_from_slice(&self.data[self.pos..self.pos + n]);
            self.pos += n;
            Ok(n)
        }
    }

    #[test]
    fn streaming_reader_validates_header_first_and_fails_fast_on_eof_garbage_crc_and_io() {
        let payload = sample_payload();
        let (valid, hdr) = encode_backup(&payload).expect("encode succeeds");

        // 1. Valid streaming round-trip (via small reads to exercise loop accumulation).
        let mut cursor = std::io::Cursor::new(&valid);
        let decoded = decode_backup_from_reader(&mut cursor).expect("stream decode must succeed");
        assert_eq!(decoded.header, hdr, "stream header must match");
        assert_eq!(decoded.payload, payload, "stream payload must match");

        // 2. Truncated header (< 24 bytes) fails immediately with TruncatedHeader.
        let mut short_hdr = std::io::Cursor::new(&valid[..10]);
        assert!(
            matches!(
                decode_backup_from_reader(&mut short_hdr),
                Err(CodecError::TruncatedHeader { actual: 10 })
            ),
            "10-byte file must fail with TruncatedHeader"
        );

        // 3. Invalid magic fails strictly after reading 24-byte header without touching payload.
        let mut bad_magic = valid.clone();
        bad_magic[0] = b'Z';
        let mut guarded_magic = HeaderOnlyGuardReader {
            data: bad_magic,
            pos: 0,
            max_allowed: RTMAGA_HEADER_LEN,
            exceeded_limit: false,
        };
        assert!(
            matches!(
                decode_backup_from_reader(&mut guarded_magic),
                Err(CodecError::InvalidMagic)
            ),
            "bad magic must fail before reading payload"
        );
        assert!(
            !guarded_magic.exceeded_limit,
            "bad magic must not read past 24-byte header"
        );

        // 4. Forged oversized payload length fails strictly after 24-byte header before allocation!
        let mut oversized_hdr = valid.clone();
        let too_big = u32::try_from(MAX_COMPRESSED_PAYLOAD_BYTES + 1).expect("fits u32");
        oversized_hdr[16..20].copy_from_slice(&too_big.to_be_bytes());
        let mut guarded_size = HeaderOnlyGuardReader {
            data: oversized_hdr,
            pos: 0,
            max_allowed: RTMAGA_HEADER_LEN,
            exceeded_limit: false,
        };
        assert!(
            matches!(
                decode_backup_from_reader(&mut guarded_size),
                Err(CodecError::PayloadTooLarge { .. })
            ),
            "oversized payload header must fail before allocating payload buffer"
        );
        assert!(
            !guarded_size.exceeded_limit,
            "oversized header must not read past 24-byte header"
        );

        // 5. Unexpected EOF mid-payload fails immediately with PayloadSizeMismatch.
        let mut eof_cursor = std::io::Cursor::new(&valid[..valid.len() - 5]);
        assert!(
            matches!(
                decode_backup_from_reader(&mut eof_cursor),
                Err(CodecError::PayloadSizeMismatch { .. })
            ),
            "unexpected EOF mid-payload must fail with PayloadSizeMismatch"
        );

        // 6. Trailing garbage bytes after valid payload fail immediately with PayloadSizeMismatch.
        let mut with_garbage = valid.clone();
        with_garbage.extend_from_slice(b"EXTRA");
        let mut garbage_cursor = std::io::Cursor::new(&with_garbage);
        assert!(
            matches!(
                decode_backup_from_reader(&mut garbage_cursor),
                Err(CodecError::PayloadSizeMismatch { .. })
            ),
            "trailing garbage bytes must fail with PayloadSizeMismatch"
        );

        // 7. Single-bit payload corruption fails rolling CRC32 check.
        let mut bad_crc = valid.clone();
        let mid = RTMAGA_HEADER_LEN + (bad_crc.len() - RTMAGA_HEADER_LEN) / 2;
        bad_crc[mid] ^= 0x40;
        let mut crc_cursor = std::io::Cursor::new(&bad_crc);
        assert!(
            matches!(
                decode_backup_from_reader(&mut crc_cursor),
                Err(CodecError::ChecksumMismatch { .. })
            ),
            "corrupted payload byte must fail rolling CRC32"
        );

        // 8. Mid-stream I/O failure propagates IoError immediately.
        let mut io_fail = FailAfterBytesReader {
            data: valid,
            pos: 0,
            fail_at: RTMAGA_HEADER_LEN + 8,
        };
        assert!(
            matches!(
                decode_backup_from_reader(&mut io_fail),
                Err(CodecError::IoError(_))
            ),
            "mid-stream I/O error must fail immediately with IoError"
        );
    }

    #[test]
    fn streaming_reader_handles_multi_chunk_payload_exceeding_64kib() {
        let mut payload = sample_payload();
        // Add enough distinct high-entropy entries so LZ4 compressed payload exceeds 64 KiB.
        for i in 0..1800_u32 {
            let mut entry = payload.manga[0].clone();
            entry.url = format!(
                "https://example.com/series/item-{i:04}-{:08x}",
                i.wrapping_mul(2_654_435_761)
            );
            entry.title = format!("Series #{i:04} {:08x}", i.wrapping_mul(1_597_334_677));
            entry.description = Some(format!(
                "Unique chunk verification text {:08x} {:08x} {:08x} {:08x}",
                i.wrapping_mul(73_856_093),
                i.wrapping_mul(19_349_663),
                i.wrapping_mul(83_492_791),
                i.wrapping_mul(49_979_687),
            ));
            payload.manga.push(entry);
        }

        let (encoded, hdr) = encode_backup(&payload).expect("encode large payload");
        assert!(
            usize::try_from(hdr.payload_size).expect("fits usize") > STREAM_CHUNK_BYTES,
            "test payload must exceed 64 KiB to exercise multi-chunk rolling CRC32"
        );

        let mut reader =
            std::io::BufReader::with_capacity(STREAM_CHUNK_BYTES, std::io::Cursor::new(&encoded));
        let decoded =
            decode_backup_from_reader(&mut reader).expect("multi-chunk stream decode succeeds");
        assert_eq!(
            decoded.payload.manga.len(),
            payload.manga.len(),
            "all manga entries must round-trip across multiple 64 KiB chunks"
        );
    }

    #[test]
    fn legacy_backup_without_detected_layout_field_decodes_cleanly() {
        // Construct a legacy v1 MessagePack payload omitting `"dl"` (`detected_layout`) completely.
        let legacy_val = serde_json::json!({
            "meta": {
                "av": "3.2.1",
                "dv": "LegacyRig",
                "ts": 1_764_220_800_000_i64,
                "bl": BLOCK_MANGA
            },
            "dict": { "s": ["example.com"], "c": ["Action"] },
            "manga": [{
                "si": 0,
                "u": "https://example.com/series/legacy",
                "t": "Legacy Title",
                "dw": true,
                "gi": [0]
            }]
        });
        let raw_mp = rmp_serde::to_vec_named(&legacy_val).expect("legacy msgpack encodes");
        let compressed = lz4_flex::compress_prepend_size(&raw_mp);
        let hdr = RtmagaHeader {
            version: RTMAGA_VERSION_V1,
            flags: FLAG_COMPRESSION_LZ4,
            timestamp_ms: 1_764_220_800_000,
            payload_size: u32::try_from(compressed.len()).expect("fits u32"),
            checksum: crc32fast::hash(&compressed),
        };
        let mut frame = Vec::with_capacity(RTMAGA_HEADER_LEN + compressed.len());
        write_header_to_buf(&hdr, &mut frame);
        frame.extend_from_slice(&compressed);

        let mut reader = std::io::Cursor::new(&frame);
        let decoded =
            decode_backup_from_reader(&mut reader).expect("legacy backup without dl must decode");
        assert_eq!(decoded.payload.manga.len(), 1, "legacy manga must decode");
        assert!(
            decoded.payload.manga[0].detected_webtoon,
            "legacy detected_webtoon flag must survive"
        );
        assert_eq!(
            decoded.payload.manga[0].detected_layout, None,
            "absent legacy dl field must default to None"
        );
    }
}
