//! Persisted user settings for appearance, reader modes, update checks, proxy routing, sources, and cache.

use mangacore::{HeadlessPolicy, ProxyConfig};
use serde::{Deserialize, Serialize};

/// Supported UI display languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum Language {
    /// English (default).
    #[default]
    En,
    /// Vietnamese.
    Vi,
    /// Chinese (Simplified).
    Zh,
    /// Japanese.
    Ja,
    /// Russian.
    Ru,
    /// Korean.
    Ko,
    /// Thai.
    Th,
}

/// Application UI color theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum Theme {
    /// Follow the OS light/dark system preference.
    #[default]
    System,
    /// Light appearance.
    Light,
    /// Dark appearance.
    Dark,
}

/// How manga page images are sized horizontally inside the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", content = "value", rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum ImageWidth {
    /// Stretch or shrink images to match the window width.
    FitWindow,
    /// Percentage of the viewport width (clamped to `30..=100`).
    Percent(u8),
    /// Fixed maximum width in CSS pixels (clamped to `400..=2000`).
    Px(u16),
}

impl Default for ImageWidth {
    fn default() -> Self {
        Self::Px(800)
    }
}

/// Background color behind manga images in the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum ReaderBg {
    /// Always dark background (comfortable for long reading sessions).
    #[default]
    Dark,
    /// Always light background.
    Light,
    /// Match the active application theme.
    FollowTheme,
}

/// Top-level reading scroll orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum ReadingOrientation {
    /// Top-to-bottom vertical reading (Continuous Strip or Vertical Grouped Scroll).
    #[default]
    Vertical,
    /// Left-to-right or right-to-left horizontal grouped reading.
    Horizontal,
}

/// Vertical reader layout sub-mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum VerticalLayoutMode {
    /// Continuous seamless vertical scrolling without page/group snapping.
    #[default]
    ContinuousStrip,
    /// Vertical scrolling partitioned into configurable page groups with optional group snap.
    VerticalGrouped,
}

/// Horizontal reading direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum HorizontalDirection {
    /// Left to right.
    #[default]
    Ltr,
    /// Right to left (traditional manga order).
    Rtl,
}

/// Scroll snap granularity in Horizontal reading mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum HorizontalSnapMode {
    /// Free horizontal scrolling.
    None,
    /// Snap per individual page.
    Page,
    /// Snap per configured page group.
    #[default]
    Group,
}

/// Group alignment inside the viewport for Vertical Grouped Scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum GroupAlignment {
    /// Center groups in the viewport.
    #[default]
    Center,
    /// Align groups to the top edge of the viewport.
    Start,
}

/// Persisted configuration specific to Vertical Reading Mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct VerticalModeSettings {
    /// Continuous strip vs grouped vertical scroll.
    pub layout_mode: VerticalLayoutMode,
    /// Number of pages per group in `VerticalGrouped` mode (`1..=8`).
    pub pages_per_group: u8,
    /// Gap between consecutive pages (`0..=48 px`).
    pub page_gap_px: u8,
    /// Gap between consecutive groups in `VerticalGrouped` mode (`8..=96 px`).
    pub group_gap_px: u8,
    /// Snap to group boundaries in `VerticalGrouped` mode (ignored in `ContinuousStrip`).
    pub snap_enabled: bool,
    /// Group alignment in viewport.
    pub alignment: GroupAlignment,
    /// Dim pages/groups outside the focal reading area.
    pub dim_unfocused: bool,
    /// Dimming strength (`0..=100%`).
    pub dim_level: u8,
}

impl Default for VerticalModeSettings {
    fn default() -> Self {
        Self {
            layout_mode: VerticalLayoutMode::ContinuousStrip,
            pages_per_group: 2,
            page_gap_px: 0,
            group_gap_px: 32,
            snap_enabled: true,
            alignment: GroupAlignment::Center,
            dim_unfocused: false,
            dim_level: 50,
        }
    }
}

/// Persisted configuration specific to Horizontal Reading Mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct HorizontalModeSettings {
    /// Number of pages displayed together in each horizontal group (`1..=8`).
    pub pages_per_group: u8,
    /// Spacing between pages inside a group (`0..=48 px`).
    pub page_gap_px: u8,
    /// Spacing between neighboring groups (`8..=96 px`).
    pub group_gap_px: u8,
    /// Left-to-right or right-to-left reading flow.
    pub direction: HorizontalDirection,
    /// Scroll snap mode (`none`, `page`, or `group`).
    pub snap_mode: HorizontalSnapMode,
    /// Center active page groups inside the viewport.
    pub center_groups: bool,
    /// Dim pages/groups outside the focused group.
    pub dim_unfocused: bool,
    /// Dimming strength (`0..=100%`).
    pub dim_level: u8,
}

impl Default for HorizontalModeSettings {
    fn default() -> Self {
        Self {
            pages_per_group: 1,
            page_gap_px: 8,
            group_gap_px: 36,
            direction: HorizontalDirection::Ltr,
            snap_mode: HorizontalSnapMode::Group,
            center_groups: true,
            dim_unfocused: true,
            dim_level: 55,
        }
    }
}

/// Reader display, orientation, and continuous-scroll preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ReaderSettings {
    /// Active reading orientation (`vertical` or `horizontal`).
    pub orientation: ReadingOrientation,
    /// Image horizontal sizing mode (used in Vertical mode).
    pub width: ImageWidth,
    /// Legacy vertical gap between pages in pixels (kept synchronized with `vertical.page_gap_px`).
    pub gap_px: u8,
    /// Number of upcoming images to preload ahead of the viewport.
    pub preload_images: u8,
    /// Automatically append the next chapter when approaching the end in Vertical mode.
    pub auto_next_chapter: bool,
    /// Reader backdrop preference.
    pub reader_bg: ReaderBg,
    /// Independent Vertical Reading Mode configuration.
    pub vertical: VerticalModeSettings,
    /// Independent Horizontal Reading Mode configuration.
    pub horizontal: HorizontalModeSettings,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            orientation: ReadingOrientation::Vertical,
            width: ImageWidth::default(),
            gap_px: 0,
            preload_images: 4,
            auto_next_chapter: true,
            reader_bg: ReaderBg::Dark,
            vertical: VerticalModeSettings::default(),
            horizontal: HorizontalModeSettings::default(),
        }
    }
}

/// Background new-chapter and app-release checker settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct UpdateSettings {
    /// Enable automatic background checking for manga in reading history.
    pub enabled: bool,
    /// Interval between background checks in hours (`1..=48`).
    pub interval_hours: u8,
    /// Emit a native desktop notification when new chapters are discovered.
    pub notify_windows: bool,
    /// Automatically check for newer `RtMaga` application releases when the app opens.
    pub check_app_updates: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_hours: 6,
            notify_windows: true,
            check_app_updates: true,
        }
    }
}

/// Scraper and headless browser preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SourceSettings {
    /// Allow falling back to a headless browser on JS-rendered websites.
    pub headless_enabled: bool,
    /// Maximum simultaneous requests per remote host (`1..=8`).
    pub max_conn_per_host: u8,
}

impl Default for SourceSettings {
    fn default() -> Self {
        Self {
            headless_enabled: true,
            max_conn_per_host: 4,
        }
    }
}

/// Local image disk cache limits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct CacheSettings {
    /// Maximum disk cache size in megabytes (`64..=10240`).
    pub max_mb: u32,
}

impl Default for CacheSettings {
    fn default() -> Self {
        Self { max_mb: 1024 }
    }
}

/// Root settings object persisted as JSON in SQLite.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct Settings {
    /// UI display language.
    pub language: Language,
    /// UI color theme.
    pub theme: Theme,
    /// Vertical & Horizontal reader settings.
    pub reader: ReaderSettings,
    /// Background chapter and app update check settings.
    pub updates: UpdateSettings,
    /// Scraper source settings.
    pub sources: SourceSettings,
    /// Application-internal proxy routing configuration (defaults to Direct Mode).
    pub proxy: ProxyConfig,
    /// Disk image cache settings.
    pub cache: CacheSettings,
}

impl Settings {
    /// Clamps numeric fields into safe operational bounds and reconciles legacy fields.
    pub(crate) fn sanitized(mut self) -> Self {
        self.reader.width = match self.reader.width {
            ImageWidth::FitWindow => ImageWidth::FitWindow,
            ImageWidth::Percent(p) => ImageWidth::Percent(p.clamp(30, 100)),
            ImageWidth::Px(px) => ImageWidth::Px(px.clamp(400, 2000)),
        };

        // Reconcile legacy `gap_px` if loaded from pre-2.0 JSON where `vertical.page_gap_px` was default 0.
        if self.reader.vertical.page_gap_px == 0 && self.reader.gap_px > 0 {
            self.reader.vertical.page_gap_px = self.reader.gap_px;
        }
        self.reader.vertical.pages_per_group = self.reader.vertical.pages_per_group.clamp(1, 8);
        self.reader.vertical.page_gap_px = self.reader.vertical.page_gap_px.min(48);
        self.reader.vertical.group_gap_px = self.reader.vertical.group_gap_px.clamp(8, 96);
        self.reader.vertical.dim_level = self.reader.vertical.dim_level.min(100);
        self.reader.gap_px = self.reader.vertical.page_gap_px;

        self.reader.horizontal.pages_per_group = self.reader.horizontal.pages_per_group.clamp(1, 8);
        self.reader.horizontal.page_gap_px = self.reader.horizontal.page_gap_px.min(48);
        self.reader.horizontal.group_gap_px = self.reader.horizontal.group_gap_px.clamp(8, 96);
        self.reader.horizontal.dim_level = self.reader.horizontal.dim_level.min(100);

        self.reader.preload_images = self.reader.preload_images.clamp(1, 16);
        self.updates.interval_hours = self.updates.interval_hours.clamp(1, 48);
        self.sources.max_conn_per_host = self.sources.max_conn_per_host.clamp(1, 8);
        self.proxy = self.proxy.sanitized();
        self.cache.max_mb = self.cache.max_mb.clamp(64, 10_240);
        self
    }

    /// Converts the headless setting into a [`HeadlessPolicy`] for `mangacore`.
    pub(crate) fn headless_policy(&self) -> HeadlessPolicy {
        if self.sources.headless_enabled {
            HeadlessPolicy::Enabled
        } else {
            HeadlessPolicy::Disabled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_legacy_and_v2_frontend_json_payloads() {
        // Pre-2.0 JSON lacking `orientation`, `vertical`, `horizontal`, `proxy`, and `checkAppUpdates`.
        let legacy_json = r#"{
            "theme": "dark",
            "reader": {
                "width": { "mode": "px", "value": 920 },
                "gapPx": 12,
                "preloadImages": 6,
                "autoNextChapter": false,
                "readerBg": "follow_theme"
            },
            "updates": { "enabled": true, "intervalHours": 3, "notifyWindows": false },
            "sources": { "headlessEnabled": false, "maxConnPerHost": 4 },
            "cache": { "maxMb": 512 }
        }"#;
        let parsed = serde_json::from_str::<Settings>(legacy_json)
            .unwrap()
            .sanitized();
        assert_eq!(parsed.language, Language::En);
        assert_eq!(parsed.theme, Theme::Dark);
        assert_eq!(parsed.reader.orientation, ReadingOrientation::Vertical);
        assert_eq!(
            parsed.reader.vertical.layout_mode,
            VerticalLayoutMode::ContinuousStrip
        );
        assert_eq!(parsed.reader.vertical.page_gap_px, 12);
        assert_eq!(parsed.reader.gap_px, 12);
        assert_eq!(parsed.reader.horizontal.pages_per_group, 1);
        assert!(!parsed.proxy.enabled);
        assert!(!parsed.proxy.allow_direct_fallback);
        assert!(parsed.updates.check_app_updates);

        // Out-of-bounds values must clamp cleanly without panicking.
        let mut wild = Settings::default();
        wild.reader.horizontal.pages_per_group = 0;
        wild.reader.horizontal.dim_level = 250;
        wild.reader.vertical.pages_per_group = 99;
        wild.reader.vertical.group_gap_px = 2;
        let clamped = wild.sanitized();
        assert_eq!(clamped.reader.horizontal.pages_per_group, 1);
        assert_eq!(clamped.reader.horizontal.dim_level, 100);
        assert_eq!(clamped.reader.vertical.pages_per_group, 8);
        assert_eq!(clamped.reader.vertical.group_gap_px, 8);
    }
}
