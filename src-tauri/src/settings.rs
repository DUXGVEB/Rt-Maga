//! Persisted user settings for appearance, reader behavior, update checks, sources, and cache.

use mangacore::HeadlessPolicy;
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
    /// Follow the Windows light/dark system preference.
    #[default]
    System,
    /// Light appearance.
    Light,
    /// Dark appearance.
    Dark,
}

/// How manga page images are sized horizontally inside the vertical reader.
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

/// Background color behind manga images in the vertical reader.
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

/// Reader display and continuous-scroll preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReaderSettings {
    /// Image horizontal sizing mode.
    pub width: ImageWidth,
    /// Vertical gap between consecutive pages in pixels.
    pub gap_px: u8,
    /// Number of upcoming images to preload ahead of the viewport.
    pub preload_images: u8,
    /// Automatically append the next chapter when approaching the end of the current chapter.
    pub auto_next_chapter: bool,
    /// Reader backdrop preference.
    pub reader_bg: ReaderBg,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            width: ImageWidth::default(),
            gap_px: 0,
            preload_images: 4,
            auto_next_chapter: true,
            reader_bg: ReaderBg::Dark,
        }
    }
}

/// Background new-chapter checker settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateSettings {
    /// Enable automatic background checking for manga in reading history.
    pub enabled: bool,
    /// Interval between background checks in hours (`1..=48`).
    pub interval_hours: u8,
    /// Emit a native Windows desktop notification when new chapters are discovered.
    pub notify_windows: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_hours: 6,
            notify_windows: true,
        }
    }
}

/// Scraper and headless browser preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceSettings {
    /// Allow falling back to headless Microsoft Edge / Chrome on JS-rendered websites.
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub(crate) struct Settings {
    /// UI display language.
    #[serde(default)]
    pub language: Language,
    /// UI color theme.
    pub theme: Theme,
    /// Vertical reader settings.
    pub reader: ReaderSettings,
    /// Background chapter update check settings.
    pub updates: UpdateSettings,
    /// Scraper source settings.
    pub sources: SourceSettings,
    /// Disk image cache settings.
    pub cache: CacheSettings,
}

impl Settings {
    /// Clamps numeric fields into safe operational bounds.
    pub(crate) fn sanitized(mut self) -> Self {
        self.reader.width = match self.reader.width {
            ImageWidth::FitWindow => ImageWidth::FitWindow,
            ImageWidth::Percent(p) => ImageWidth::Percent(p.clamp(30, 100)),
            ImageWidth::Px(px) => ImageWidth::Px(px.clamp(400, 2000)),
        };
        self.reader.gap_px = self.reader.gap_px.min(48);
        self.reader.preload_images = self.reader.preload_images.clamp(1, 16);
        self.updates.interval_hours = self.updates.interval_hours.clamp(1, 48);
        self.sources.max_conn_per_host = self.sources.max_conn_per_host.clamp(1, 8);
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
    fn deserializes_frontend_json_payloads() {
        let px_json = r#"{
            "theme": "dark",
            "reader": {
                "width": { "mode": "px", "value": 920 },
                "gapPx": 8,
                "preloadImages": 6,
                "autoNextChapter": false,
                "readerBg": "follow_theme"
            },
            "updates": { "enabled": true, "intervalHours": 3, "notifyWindows": false },
            "sources": { "headlessEnabled": false, "maxConnPerHost": 4 },
            "cache": { "maxMb": 512 }
        }"#;
        let parsed: Settings = serde_json::from_str(px_json).unwrap();
        assert_eq!(parsed.language, Language::En);
        assert_eq!(parsed.theme, Theme::Dark);
        assert_eq!(parsed.reader.width, ImageWidth::Px(920));
        assert!(!parsed.reader.auto_next_chapter);
        assert_eq!(parsed.reader.reader_bg, ReaderBg::FollowTheme);
        assert_eq!(parsed.updates.interval_hours, 3);
        assert!(!parsed.sources.headless_enabled);

        let lang_json = px_json.replace(
            r#""theme": "dark","#,
            r#""language": "ja", "theme": "dark","#,
        );
        let parsed_ja: Settings = serde_json::from_str(&lang_json).unwrap();
        assert_eq!(parsed_ja.language, Language::Ja);

        let fit_json = px_json.replace(
            r#"{ "mode": "px", "value": 920 }"#,
            r#"{ "mode": "fit_window" }"#,
        );
        let parsed_fit: Settings = serde_json::from_str(&fit_json).unwrap();
        assert_eq!(parsed_fit.reader.width, ImageWidth::FitWindow);
    }
}
