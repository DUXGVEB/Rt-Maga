//! Persisted user settings for appearance, device identity, reader modes, floating controls,
//! update checks, DNS-over-HTTPS protection, proxy routing, sources, and cache.

use mangacore::{DohConfig, HeadlessPolicy, MAX_SPEED_LIMIT_KBPS, ProxyConfig};
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
    /// Force zero gap (`0px`) between consecutive vertical pages.
    pub no_gap_mode: bool,
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
            no_gap_mode: false,
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

/// Mobile screen orientation lock mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum OrientationLockMode {
    /// Follow device rotation sensor automatically.
    #[default]
    Auto,
    /// Lock screen to portrait orientation.
    Portrait,
    /// Lock screen to landscape orientation.
    Landscape,
}

/// Edge anchoring for customizable floating controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum FloatingEdge {
    /// Anchor control near the left viewport edge.
    Left,
    /// Anchor control near the right viewport edge.
    #[default]
    Right,
}

/// Customizable position of a floating action button (auto-clamped against safe areas & collisions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct FloatingButtonPosition {
    /// Horizontal viewport edge anchor (`left` or `right`).
    pub edge: FloatingEdge,
    /// Vertical position along viewport height (`10..=90%`).
    pub vertical_percent: u8,
    /// Horizontal inset from the anchored edge in CSS pixels (`8..=48 px`).
    pub horizontal_offset_px: u8,
}

impl Default for FloatingButtonPosition {
    fn default() -> Self {
        Self {
            edge: FloatingEdge::Right,
            vertical_percent: 82,
            horizontal_offset_px: 16,
        }
    }
}

/// Persisted customization for floating Scroll-to-Top, Reader Settings, and Orientation Lock buttons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct FloatingButtonsSettings {
    /// Screen keys where the floating Scroll-to-Top button is enabled
    /// (`"home"`, `"bookmarks"`, `"history"`, `"manga"`, `"reader"`, `"settings"`).
    pub scroll_to_top_screens: Vec<String>,
    /// Position preference for the Scroll-to-Top button.
    pub scroll_to_top: FloatingButtonPosition,
    /// Show the floating Reader Settings button inside the reader.
    pub show_reader_settings: bool,
    /// Position preference for the floating Reader Settings button.
    pub reader_settings: FloatingButtonPosition,
    /// Show the floating Orientation Lock button inside the mobile reader.
    pub show_orientation_lock: bool,
    /// Position preference for the floating Orientation Lock button.
    pub orientation_lock: FloatingButtonPosition,
    /// Subtly dim floating controls in the reader when idle and restore opacity on interaction.
    pub auto_dim_in_reader: bool,
}

const VALID_FLOATING_SCREENS: [&str; 6] = [
    "home",
    "bookmarks",
    "history",
    "manga",
    "reader",
    "settings",
];

impl Default for FloatingButtonsSettings {
    fn default() -> Self {
        Self {
            scroll_to_top_screens: VALID_FLOATING_SCREENS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            scroll_to_top: FloatingButtonPosition {
                edge: FloatingEdge::Right,
                vertical_percent: 82,
                horizontal_offset_px: 16,
            },
            show_reader_settings: true,
            reader_settings: FloatingButtonPosition {
                edge: FloatingEdge::Right,
                vertical_percent: 68,
                horizontal_offset_px: 16,
            },
            show_orientation_lock: true,
            orientation_lock: FloatingButtonPosition {
                edge: FloatingEdge::Right,
                vertical_percent: 54,
                horizontal_offset_px: 16,
            },
            auto_dim_in_reader: true,
        }
    }
}

/// Customizable device identity persisted in Settings and used for LAN Sync.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct DeviceSettings {
    /// Optional user-chosen device display name (`None` = auto-detected OS/hardware device name).
    pub custom_name: Option<String>,
}

/// Configurable action executed on double-tap inside the reader viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum DoubleTapAction {
    /// Smoothly toggle 1x ↔ 1.8x zoom centered on the tap point (default).
    #[default]
    Zoom,
    /// Toggle top/bottom reader toolbars.
    ToggleToolbar,
    /// Advance to the next page (horizontal) or scroll down one viewport (vertical).
    NextPage,
    /// Disable double-tap gesture handling.
    Disabled,
}

/// Interactive reader behavior, gesture, and resilience preferences (v2.2.0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ReaderBehaviorSettings {
    /// Prevent device screen sleep while actively reading in the reader.
    pub keep_screen_on: bool,
    /// Collapse failed images into a compact inline retry pill instead of blocking reading.
    pub skip_broken_images: bool,
    /// Automatically mark a chapter as read immediately when opened.
    pub auto_mark_read_on_open: bool,
    /// Action performed on double-tap inside the reader viewport.
    pub double_tap_action: DoubleTapAction,
    /// Automatic toolbar hide delay in seconds (`0` = disabled, `2..=15` seconds).
    pub toolbar_auto_hide_secs: u8,
    /// Horizontal side margin in CSS pixels for vertical reading (`0..=64 px`).
    pub vertical_margin_px: u8,
}

impl Default for ReaderBehaviorSettings {
    fn default() -> Self {
        Self {
            keep_screen_on: true,
            skip_broken_images: false,
            auto_mark_read_on_open: false,
            double_tap_action: DoubleTapAction::Zoom,
            toolbar_auto_hide_secs: 3,
            vertical_margin_px: 0,
        }
    }
}

/// Reader display, orientation, auto-webtoon, preloading, and continuous-scroll preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ReaderSettings {
    /// Active reading orientation (`vertical` or `horizontal`).
    pub orientation: ReadingOrientation,
    /// Image horizontal sizing mode (legacy compatibility field).
    pub width: ImageWidth,
    /// Legacy vertical gap between pages in pixels (kept synchronized with `vertical.page_gap_px`).
    pub gap_px: u8,
    /// Controlled nearby image preload window (`5..=8`, default `6`).
    pub preload_images: u8,
    /// Automatically append the next chapter when approaching the end in Vertical mode.
    pub auto_next_chapter: bool,
    /// Passively detect Webtoon / Long-Strip chapters from aspect ratios and switch to seamless vertical strip.
    pub auto_webtoon_detect: bool,
    /// Enable edge tap zones (left/right page navigation, center toolbar toggle) in horizontal reading mode.
    pub tap_zones_enabled: bool,
    /// Preferred screen orientation lock inside the mobile reader.
    pub orientation_lock: OrientationLockMode,
    /// Reader backdrop preference.
    pub reader_bg: ReaderBg,
    /// Independent Vertical Reading Mode configuration.
    pub vertical: VerticalModeSettings,
    /// Independent Horizontal Reading Mode configuration.
    pub horizontal: HorizontalModeSettings,
    /// Interactive reader gesture, screen wake, auto-hide, margin, and broken-image behavior.
    pub behavior: ReaderBehaviorSettings,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            orientation: ReadingOrientation::Vertical,
            width: ImageWidth::default(),
            gap_px: 0,
            preload_images: 6,
            auto_next_chapter: true,
            auto_webtoon_detect: true,
            tap_zones_enabled: true,
            orientation_lock: OrientationLockMode::Auto,
            reader_bg: ReaderBg::Dark,
            vertical: VerticalModeSettings::default(),
            horizontal: HorizontalModeSettings::default(),
            behavior: ReaderBehaviorSettings::default(),
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

/// Scraper, headless browser, and global network speed limit preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SourceSettings {
    /// Allow falling back to a headless browser on JS-rendered websites.
    pub headless_enabled: bool,
    /// Maximum simultaneous requests per remote host (`1..=8`).
    pub max_conn_per_host: u8,
    /// Global aggregate download speed limit in KiB/s (`0` = unlimited, max `102400`).
    pub speed_limit_kbps: u32,
}

impl Default for SourceSettings {
    fn default() -> Self {
        Self {
            headless_enabled: true,
            max_conn_per_host: 4,
            speed_limit_kbps: 0,
        }
    }
}

/// Identifies an individual settings group that can be reset to factory defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum SettingsSection {
    Appearance,
    Device,
    Reader,
    Floating,
    Network,
    Updates,
    Sources,
    Cache,
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
    /// Device identity settings (real + custom display name).
    pub device: DeviceSettings,
    /// Vertical & Horizontal reader settings.
    pub reader: ReaderSettings,
    /// Floating action buttons visibility & placement settings.
    pub floating: FloatingButtonsSettings,
    /// Background chapter and app update check settings.
    pub updates: UpdateSettings,
    /// Scraper source settings.
    pub sources: SourceSettings,
    /// In-app provider-based DNS-over-HTTPS (`DoH`) protection configuration.
    pub doh: DohConfig,
    /// Application-internal proxy routing configuration (defaults to Direct Mode).
    pub proxy: ProxyConfig,
    /// Disk image cache settings.
    pub cache: CacheSettings,
}

fn sanitize_floating_pos(pos: &mut FloatingButtonPosition) {
    pos.vertical_percent = pos.vertical_percent.clamp(10, 90);
    pos.horizontal_offset_px = pos.horizontal_offset_px.clamp(8, 48);
}

impl Settings {
    /// Clamps numeric fields into safe operational bounds and reconciles legacy fields.
    pub(crate) fn sanitized(mut self) -> Self {
        self.device.custom_name = self.device.custom_name.and_then(|raw| {
            let trimmed: String = raw.trim().chars().take(48).collect();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        self.reader.width = match self.reader.width {
            ImageWidth::FitWindow => ImageWidth::FitWindow,
            ImageWidth::Percent(p) => ImageWidth::Percent(p.clamp(30, 100)),
            ImageWidth::Px(px) => ImageWidth::Px(px.clamp(400, 2000)),
        };

        // Reconcile legacy `gap_px` if loaded from pre-2.0 JSON where `vertical.page_gap_px` was default 0.
        if self.reader.vertical.page_gap_px == 0 && self.reader.gap_px > 0 {
            self.reader.vertical.page_gap_px = self.reader.gap_px;
        }
        // Vertical reading always uses continuous vertical scroll (legacy `vertical_grouped` normalizes safely).
        self.reader.vertical.layout_mode = VerticalLayoutMode::ContinuousStrip;
        self.reader.vertical.pages_per_group = self.reader.vertical.pages_per_group.clamp(1, 8);
        self.reader.vertical.page_gap_px = self.reader.vertical.page_gap_px.min(48);
        self.reader.vertical.group_gap_px = self.reader.vertical.group_gap_px.clamp(8, 96);
        self.reader.vertical.dim_level = self.reader.vertical.dim_level.min(100);
        self.reader.gap_px = self.reader.vertical.page_gap_px;

        self.reader.horizontal.pages_per_group = self.reader.horizontal.pages_per_group.clamp(1, 8);
        self.reader.horizontal.page_gap_px = self.reader.horizontal.page_gap_px.min(48);
        self.reader.horizontal.group_gap_px = self.reader.horizontal.group_gap_px.clamp(8, 96);
        self.reader.horizontal.dim_level = self.reader.horizontal.dim_level.min(100);

        // Enforce controlled 5-8 image preload window to balance smoothness and low RAM usage.
        self.reader.preload_images = self.reader.preload_images.clamp(5, 8);
        self.reader.behavior.toolbar_auto_hide_secs =
            if self.reader.behavior.toolbar_auto_hide_secs == 0 {
                0
            } else {
                self.reader.behavior.toolbar_auto_hide_secs.clamp(2, 15)
            };
        self.reader.behavior.vertical_margin_px = self.reader.behavior.vertical_margin_px.min(64);
        if self.reader.vertical.no_gap_mode {
            self.reader.vertical.page_gap_px = 0;
            self.reader.gap_px = 0;
        }

        // Sanitize floating controls screens & bounds.
        let mut clean_screens: Vec<String> = Vec::with_capacity(VALID_FLOATING_SCREENS.len());
        for screen in &self.floating.scroll_to_top_screens {
            let lower = screen.trim().to_ascii_lowercase();
            if VALID_FLOATING_SCREENS.contains(&lower.as_str()) && !clean_screens.contains(&lower) {
                clean_screens.push(lower);
            }
        }
        self.floating.scroll_to_top_screens = clean_screens;
        sanitize_floating_pos(&mut self.floating.scroll_to_top);
        sanitize_floating_pos(&mut self.floating.reader_settings);
        sanitize_floating_pos(&mut self.floating.orientation_lock);

        self.updates.interval_hours = self.updates.interval_hours.clamp(1, 48);
        self.sources.max_conn_per_host = self.sources.max_conn_per_host.clamp(1, 8);
        self.sources.speed_limit_kbps = self.sources.speed_limit_kbps.min(MAX_SPEED_LIMIT_KBPS);
        self.doh = self.doh.sanitized();
        self.proxy = self.proxy.sanitized();
        self.cache.max_mb = self.cache.max_mb.clamp(64, 10_240);
        self
    }

    /// Resets only a single settings section back to defaults without touching other sections.
    pub(crate) fn reset_section(&mut self, section: SettingsSection) {
        match section {
            SettingsSection::Appearance => {
                self.language = Language::default();
                self.theme = Theme::default();
            }
            SettingsSection::Device => {
                self.device = DeviceSettings::default();
            }
            SettingsSection::Reader => {
                self.reader = ReaderSettings::default();
            }
            SettingsSection::Floating => {
                self.floating = FloatingButtonsSettings::default();
            }
            SettingsSection::Network => {
                self.doh = DohConfig::default();
                self.proxy = ProxyConfig::default();
            }
            SettingsSection::Updates => {
                self.updates = UpdateSettings::default();
            }
            SettingsSection::Sources => {
                self.sources = SourceSettings::default();
            }
            SettingsSection::Cache => {
                self.cache = CacheSettings::default();
            }
        }
    }

    /// Produces a safe, credential-free JSON snapshot suitable for diagnostic log export.
    pub(crate) fn redacted_for_diagnostics(&self) -> serde_json::Value {
        serde_json::json!({
            "language": self.language,
            "theme": self.theme,
            "reader": {
                "orientation": self.reader.orientation,
                "preloadImages": self.reader.preload_images,
                "autoNextChapter": self.reader.auto_next_chapter,
                "autoWebtoonDetect": self.reader.auto_webtoon_detect,
                "tapZonesEnabled": self.reader.tap_zones_enabled,
                "orientationLock": self.reader.orientation_lock,
                "verticalNoGap": self.reader.vertical.no_gap_mode,
                "verticalMarginPx": self.reader.behavior.vertical_margin_px,
                "doubleTapAction": self.reader.behavior.double_tap_action,
                "toolbarAutoHideSecs": self.reader.behavior.toolbar_auto_hide_secs,
                "keepScreenOn": self.reader.behavior.keep_screen_on,
                "skipBrokenImages": self.reader.behavior.skip_broken_images,
                "autoMarkReadOnOpen": self.reader.behavior.auto_mark_read_on_open,
            },
            "sources": {
                "headlessEnabled": self.sources.headless_enabled,
                "maxConnPerHost": self.sources.max_conn_per_host,
                "speedLimitKbps": self.sources.speed_limit_kbps,
            },
            "doh": {
                "enabled": self.doh.enabled,
                "providerId": self.doh.provider_id,
                "profileId": self.doh.profile_id,
                "hasCustomAccountId": self.doh.custom_profile_id.is_some(),
                "allowSystemFallback": self.doh.allow_system_fallback,
            },
            "proxy": {
                "enabled": self.proxy.enabled,
                "protocol": self.proxy.default_proxy.protocol,
                "hasCustomHost": !self.proxy.default_proxy.host.trim().is_empty(),
                "hasAuth": self.proxy.default_proxy.username.is_some() || self.proxy.default_proxy.password.is_some(),
                "domainRuleCount": self.proxy.domain_rules.len(),
                "allowDirectFallback": self.proxy.allow_direct_fallback,
            },
            "cache": {
                "maxMb": self.cache.max_mb,
            },
        })
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
