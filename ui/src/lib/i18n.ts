import type { Language } from "./types";

export const LANGUAGE_OPTIONS: { code: Language; label: string; locale: string }[] = [
  { code: "en", label: "English", locale: "en-US" },
  { code: "vi", label: "Tiếng Việt", locale: "vi-VN" },
  { code: "zh", label: "中文", locale: "zh-CN" },
  { code: "ja", label: "日本語", locale: "ja-JP" },
  { code: "ru", label: "Русский", locale: "ru-RU" },
  { code: "ko", label: "한국어", locale: "ko-KR" },
  { code: "th", label: "ไทย", locale: "th-TH" },
];

export interface Dictionary {
  brandSubtitle: string;
  navHome: string;
  navHistory: string;
  navSettings: string;
  checkUpdates: string;
  checking: string;
  checkingProgress: (done: number, total: number) => string;
  checkUpdatesTooltip: string;
  lightMode: string;
  darkMode: string;

  // Home
  homeTitle: string;
  newChapters: string;
  continueReading: string;
  viewAllHistory: string;
  noRecentTitle: string;
  noRecentDesc: string;

  // Link input
  linkPlaceholder: string;
  fetching: string;
  open: string;
  openVerification: string;
  waitingVerification: string;
  toastOpeningVerify: string;
  toastVerifyDone: string;

  // Manga card & detail
  badgeNew: string;
  chaptersBtn: string;
  backHome: string;
  refreshChapters: string;
  refreshing: string;
  toastChaptersUpdated: string;
  chapterCount: (n: number) => string;
  continueChapter: (name: string) => string;
  startReading: string;
  readFromFirst: string;
  chaptersHeading: string;
  filterChapters: string;
  newestFirst: string;
  oldestFirst: string;

  // Reader
  back: string;
  backTooltip: string;
  readingFallback: string;
  prevChapterTooltip: string;
  nextChapterTooltip: string;
  quickSettingsTooltip: string;
  fullscreenTooltip: string;
  imageWidth: string;
  fitWindow: string;
  autoAppendNext: string;
  readerBg: string;
  bgDark: string;
  bgLight: string;
  bgAppTheme: string;
  loadingPages: string;
  retry: string;
  loadingNextChapter: string;
  nextChapter: string;
  reachedLatest: string;
  failedPage: (n: number) => string;
  pageAlt: (n: number) => string;

  // History
  historyTitle: string;
  historySubtitle: string;
  clearAll: string;
  confirmClear: string;
  cancel: string;
  historyEmptyTitle: string;
  historyEmptyDesc: string;
  today: string;
  yesterday: string;
  earlier: string;
  removeHistoryTooltip: string;
  toastRemovedHistory: string;
  toastClearedHistory: string;

  // Settings
  settingsTitle: string;
  settingsSubtitle: string;
  appearanceSection: string;
  languageLabel: string;
  languageDesc: string;
  colorThemeLabel: string;
  colorThemeDesc: string;
  themeSystem: string;
  themeLight: string;
  themeDark: string;
  readerSection: string;
  widthModeLabel: string;
  widthModeDesc: string;
  widthPixels: string;
  widthPercent: string;
  widthLabelPx: (px: number) => string;
  widthLabelPct: (pct: number) => string;
  gapLabel: (px: number) => string;
  gapDesc: string;
  preloadLabel: (n: number) => string;
  preloadDesc: string;
  autoAppendDesc: string;
  updatesSection: string;
  checkNow: string;
  autoCheckLabel: string;
  autoCheckDesc: string;
  checkIntervalLabel: string;
  everyHours: (h: number) => string;
  notificationsLabel: string;
  notificationsDesc: string;
  sourcesSection: string;
  headlessLabel: string;
  headlessDesc: string;
  cacheSection: string;
  cacheUsageLabel: string;
  cacheUsageDesc: (size: string, files: number) => string;
  calculating: string;
  clearCache: string;
  clearing: string;
  maxCacheLabel: (mb: number) => string;
  toastCacheCleared: string;
  toastCacheClearFailed: string;
  toastSettingsSaveFailed: string;
  dismissNotification: string;

  // Updates toasts
  toastFoundOne: (title: string, latest: string) => string;
  toastFoundMany: (n: number) => string;
  toastNoHistoryToCheck: string;
  toastNoNewChapters: (checked: number) => string;
  toastCheckFailed: string;

  // App release updates
  appUpdatesSection: string;
  appVersionLabel: (v: string) => string;
  checkAppUpdatesLabel: string;
  checkAppUpdatesDesc: string;
  checkReleaseNow: string;
  releaseAvailableBanner: (version: string) => string;
  downloadUpdate: string;
  viewRelease: string;
  dismissUpdate: string;
  releaseUpToDate: (version: string) => string;
  releaseCheckFailed: string;
  releaseOpenFailed: string;
  updatePromptQuestion: string;
  updateDataSafeNote: string;
  updateDownloadAndInstall: string;
  updateInstallNow: string;
  updateLater: string;
  updateStageDownloading: string;
  updateStageVerifying: string;
  updateStageBackingUp: string;
  updateStageInstalling: string;
  updateStatusRestarting: string;
  updateAndroidPermissionHint: string;
  updateAndroidInstallerOpened: string;
  toastHistoryUpdateFailed: string;

  // Errors
  errInvalidUrlTitle: string;
  errInvalidUrlDetail: string;
  errBlockedTitle: (host: string) => string;
  errBlockedDetail: string;
  errNotRecognizedTitle: string;
  errNotRecognizedDetail: string;
  errNetworkTitle: string;
  errNotFoundTitle: string;
  errAppTitle: string;
  errGenericTitle: string;

  // Reader modes (v2.0)
  readingModeLabel: string;
  readingModeDesc: string;
  modeVertical: string;
  modeHorizontal: string;
  verticalStyleLabel: string;
  verticalStyleDesc: string;
  verticalContinuous: string;
  verticalGrouped: string;
  horizontalDirLabel: string;
  dirLtr: string;
  dirRtl: string;
  pagesPerGroupLabel: string;
  pagesPerGroupDesc: string;
  pageGapLabel: (px: number) => string;
  groupGapLabel: (px: number) => string;
  snapModeLabel: string;
  snapNone: string;
  snapPage: string;
  snapGroup: string;
  groupSnapLabel: string;
  groupAlignLabel: string;
  alignCenter: string;
  alignStart: string;
  centerGroupsLabel: string;
  centerGroupsDesc: string;
  dimUnfocusedLabel: string;
  dimUnfocusedDesc: string;
  dimLevelLabel: (pct: number) => string;

  // In-app Proxy Mode (v2.0)
  proxySection: string;
  proxySubtitle: string;
  proxyDirect: string;
  proxyEnabled: string;
  proxyProtocolLabel: string;
  proxyHostLabel: string;
  proxyPortLabel: string;
  proxyUsernameLabel: string;
  proxyPasswordLabel: string;
  proxyFailClosedLabel: string;
  proxyFailClosedDesc: string;
  proxyTestUrlLabel: string;
  proxyTestBtn: string;
  proxyTesting: string;
  proxyTestOk: (ms: number, status: number | null) => string;
  proxyTestFailed: (msg: string) => string;
  proxyDomainRulesTitle: string;
  proxyDomainRulesDesc: string;
  proxyDomainPlaceholder: string;
  proxyRuleForceProxy: string;
  proxyRuleForceDirect: string;
  proxyRuleCustomProxy: string;
  proxyAddRule: string;
  proxyRemoveRule: string;
  proxyNoRules: string;

  // Serverless LAN Sync (v2.0)
  syncSection: string;
  syncSubtitle: string;
  syncDeviceLabel: (name: string) => string;
  syncPcSendTab: string;
  syncPcReceiveTab: string;
  syncPhoneReceiveTab: string;
  syncPhoneSendTab: string;
  syncStartQrBtn: string;
  syncStartPinBtn: string;
  syncCancelBtn: string;
  syncExpiresIn: (mmss: string) => string;
  syncWaitingPeer: string;
  syncCopyCode: string;
  syncCopied: string;
  syncFallbackAddr: (addr: string) => string;
  syncEnterPinLabel: string;
  syncEnterPinPlaceholder: string;
  syncManualAddrToggle: string;
  syncManualAddrPlaceholder: string;
  syncConnectBtn: string;
  syncConnecting: string;
  syncStartCamera: string;
  syncStopCamera: string;
  syncScanImageBtn: string;
  syncPasteCodePlaceholder: string;
  syncCameraError: string;
  syncQrNotFound: string;
  syncConfirmTitle: string;
  syncConfirmSendDesc: (peer: string, ip: string, count: number) => string;
  syncConfirmReceiveDesc: (peer: string, ip: string, count: number) => string;
  syncApproveBtn: string;
  syncDeclineBtn: string;
  syncWaitingOtherConfirm: string;
  syncTransferring: string;
  syncCompletedTitle: string;
  syncReportSummary: (added: number, updated: number, skipped: number, kept: number) => string;
  syncResetBtn: string;
  syncSecurityNote: string;
}

const en: Dictionary = {
  brandSubtitle: "Clean Manga Reader",
  navHome: "Home",
  navHistory: "History",
  navSettings: "Settings",
  checkUpdates: "Check for updates",
  checking: "Checking...",
  checkingProgress: (d, t) => `Checking (${d}/${t})`,
  checkUpdatesTooltip: "Check for new chapters across manga in history",
  lightMode: "Light mode",
  darkMode: "Dark mode",

  homeTitle: "Paste a manga link to start reading",
  newChapters: "New Chapters",
  continueReading: "Continue Reading",
  viewAllHistory: "View all history",
  noRecentTitle: "No recent manga yet",
  noRecentDesc:
    "Paste any manga link above to start reading. Your reading progress will appear here automatically.",

  linkPlaceholder: "Paste a manga or chapter URL",
  fetching: "Fetching",
  open: "Open",
  openVerification: "Open verification window",
  waitingVerification: "Waiting for verification...",
  toastOpeningVerify: "Opening browser window for manual verification...",
  toastVerifyDone: "Verification complete. Retrying manga fetch...",

  badgeNew: "New",
  chaptersBtn: "Chapters",
  backHome: "Home",
  refreshChapters: "Refresh chapters",
  refreshing: "Refreshing...",
  toastChaptersUpdated: "Chapter list updated.",
  chapterCount: (n) => `${n} chapters`,
  continueChapter: (name) => `Continue · ${name}`,
  startReading: "Start reading",
  readFromFirst: "Read from Chapter 1",
  chaptersHeading: "Chapters",
  filterChapters: "Filter chapters...",
  newestFirst: "Newest first",
  oldestFirst: "Oldest first",

  back: "Back",
  backTooltip: "Back (Esc)",
  readingFallback: "Reading",
  prevChapterTooltip: "Previous chapter (Left Arrow)",
  nextChapterTooltip: "Next chapter (Right Arrow)",
  quickSettingsTooltip: "Quick reader settings",
  fullscreenTooltip: "Toggle fullscreen (F)",
  imageWidth: "Image width",
  fitWindow: "Fit window",
  autoAppendNext: "Auto-append next chapter",
  readerBg: "Reader background",
  bgDark: "Dark",
  bgLight: "Light",
  bgAppTheme: "App theme",
  loadingPages: "Loading chapter pages...",
  retry: "Retry",
  loadingNextChapter: "Loading next chapter...",
  nextChapter: "Next Chapter",
  reachedLatest: "You have reached the latest chapter",
  failedPage: (n) => `Failed to load page ${n}`,
  pageAlt: (n) => `Page ${n}`,

  historyTitle: "Reading History",
  historySubtitle:
    "New chapters are checked automatically for manga listed here. Removing an entry stops update checks.",
  clearAll: "Clear all",
  confirmClear: "Confirm clear",
  cancel: "Cancel",
  historyEmptyTitle: "History is empty",
  historyEmptyDesc: "Manga will appear here as soon as you open your first chapter.",
  today: "Today",
  yesterday: "Yesterday",
  earlier: "Earlier",
  removeHistoryTooltip: "Remove from history",
  toastRemovedHistory: "Removed from history and stopped checking for updates.",
  toastClearedHistory: "Reading history cleared.",

  settingsTitle: "Settings",
  settingsSubtitle: "All changes are saved automatically and take effect immediately.",
  appearanceSection: "Appearance",
  languageLabel: "Language",
  languageDesc: "Choose the display language for menus, settings, and notifications.",
  colorThemeLabel: "Color theme",
  colorThemeDesc: "Choose light, dark, or follow your OS preference.",
  themeSystem: "System",
  themeLight: "Light",
  themeDark: "Dark",
  readerSection: "Reader",
  widthModeLabel: "Image width mode",
  widthModeDesc: "Set a fixed pixel width, viewport percentage, or fit to window.",
  widthPixels: "Pixels",
  widthPercent: "Percent",
  widthLabelPx: (px) => `Width (${px} px)`,
  widthLabelPct: (pct) => `Width (${pct}%)`,
  gapLabel: (px) => `Gap between pages (${px} px)`,
  gapDesc: "Set to 0 px for seamless vertical strips.",
  preloadLabel: (n) => `Preload ahead (${n} images)`,
  preloadDesc: "Prefetch upcoming pages below the viewport for smoother scrolling.",
  autoAppendDesc: "Disable to show a Next Chapter button at the end instead.",
  updatesSection: "Chapter Updates",
  checkNow: "Check now",
  autoCheckLabel: "Auto-check manga in history",
  autoCheckDesc: "Runs in the background at your selected interval.",
  checkIntervalLabel: "Check interval",
  everyHours: (h) => (h === 1 ? "Every 1 hour" : `Every ${h} hours`),
  notificationsLabel: "Desktop notifications",
  notificationsDesc: "Show an OS notification when new chapters are found.",
  sourcesSection: "Sources",
  headlessLabel: "Headless browser fallback",
  headlessDesc: "Automatically used on Windows when a site renders page images via JavaScript.",
  cacheSection: "Image Cache",
  cacheUsageLabel: "Current disk usage",
  cacheUsageDesc: (size, files) => `${size} (${files} cached images)`,
  calculating: "Calculating...",
  clearCache: "Clear cache",
  clearing: "Clearing...",
  maxCacheLabel: (mb) => `Max cache size (${mb} MB)`,
  toastCacheCleared: "Image cache cleared.",
  toastCacheClearFailed: "Failed to clear image cache.",
  toastSettingsSaveFailed: "Failed to save settings.",
  dismissNotification: "Dismiss notification",

  toastFoundOne: (title, latest) => `New chapter: ${title} (${latest})`,
  toastFoundMany: (n) => `${n} manga have new chapters`,
  toastNoHistoryToCheck: "No manga in history to check yet.",
  toastNoNewChapters: (c) => `Checked ${c} manga - no new chapters yet.`,
  toastCheckFailed: "Failed to check for updates right now.",

  appUpdatesSection: "Application Updates",
  appVersionLabel: (v) => `Current version: v${v}`,
  checkAppUpdatesLabel: "Check for new releases on startup",
  checkAppUpdatesDesc: "Automatically detect when a newer version of RtMaga is available.",
  checkReleaseNow: "Check for release",
  releaseAvailableBanner: (v) => `RtMaga v${v} is available`,
  downloadUpdate: "Download",
  viewRelease: "View release",
  dismissUpdate: "Dismiss",
  releaseUpToDate: (v) => `You are on the latest version (v${v}).`,
  releaseCheckFailed: "Could not check for new releases right now.",
  releaseOpenFailed: "Could not open the release link.",
  updatePromptQuestion:
    "Would you like RtMaga to download and install this update automatically now?",
  updateDataSafeNote:
    "Your reading history, progress, library, and settings are automatically backed up and preserved.",
  updateDownloadAndInstall: "Download & Install",
  updateInstallNow: "Install now",
  updateLater: "Later",
  updateStageDownloading: "Downloading update...",
  updateStageVerifying: "Verifying integrity...",
  updateStageBackingUp: "Backing up history & settings...",
  updateStageInstalling: "Launching installer...",
  updateStatusRestarting: "Installer started. RtMaga will close and relaunch automatically.",
  updateAndroidPermissionHint:
    "Please enable 'Install unknown apps' for RtMaga in the opened Settings screen, then return and tap 'Install now' (file is already downloaded).",
  updateAndroidInstallerOpened:
    "System installer opened. Confirm installation to upgrade without losing your data.",
  toastHistoryUpdateFailed: "Could not update reading history.",

  errInvalidUrlTitle: "Invalid URL",
  errInvalidUrlDetail: "Please paste a valid manga or chapter link (starting with https://...).",
  errBlockedTitle: (host) => `${host} requires verification`,
  errBlockedDetail:
    "This site enabled anti-bot protection. Open a browser window to complete verification, then try again.",
  errNotRecognizedTitle: "Could not recognize manga content",
  errNotRecognizedDetail:
    "Try pasting a direct manga overview or chapter link, or enable Headless Browser fallback in Settings.",
  errNetworkTitle: "Could not connect to the website",
  errNotFoundTitle: "Not found",
  errAppTitle: "Application error",
  errGenericTitle: "Something went wrong",

  readingModeLabel: "Reading mode",
  readingModeDesc: "Switch between vertical scrolling and horizontal page or spread scrolling.",
  modeVertical: "Vertical",
  modeHorizontal: "Horizontal",
  verticalStyleLabel: "Vertical layout",
  verticalStyleDesc: "Choose seamless continuous scrolling or grouped pages per view.",
  verticalContinuous: "Continuous strip",
  verticalGrouped: "Grouped pages",
  horizontalDirLabel: "Reading direction",
  dirLtr: "Left to Right (LTR)",
  dirRtl: "Right to Left (RTL)",
  pagesPerGroupLabel: "Pages per group",
  pagesPerGroupDesc: "Display 1, 2, 3, or a custom number of pages together (1-8).",
  pageGapLabel: (px) => `Spacing inside group (${px} px)`,
  groupGapLabel: (px) => `Spacing between groups (${px} px)`,
  snapModeLabel: "Snap behavior",
  snapNone: "Free",
  snapPage: "Page",
  snapGroup: "Group",
  groupSnapLabel: "Snap to group",
  groupAlignLabel: "Group alignment",
  alignCenter: "Center",
  alignStart: "Top / Start",
  centerGroupsLabel: "Center groups in viewport",
  centerGroupsDesc: "Center active page spreads horizontally in the viewport.",
  dimUnfocusedLabel: "Dim unfocused pages",
  dimUnfocusedDesc: "Softly dim pages or groups outside your current focus area.",
  dimLevelLabel: (pct) => `Dim intensity (${pct}%)`,

  proxySection: "Network Proxy (Anti-Geo-Block)",
  proxySubtitle:
    "In-app proxy only — does not alter system VPN settings or route other applications.",
  proxyDirect: "Direct",
  proxyEnabled: "Proxy",
  proxyProtocolLabel: "Protocol",
  proxyHostLabel: "Proxy host",
  proxyPortLabel: "Port",
  proxyUsernameLabel: "Username (optional)",
  proxyPasswordLabel: "Password (optional)",
  proxyFailClosedLabel: "Allow direct fallback on proxy error",
  proxyFailClosedDesc:
    "Off by default (recommended) to prevent accidental real-IP leaks if the proxy fails.",
  proxyTestUrlLabel: "Test URL",
  proxyTestBtn: "Test connection",
  proxyTesting: "Testing...",
  proxyTestOk: (ms, status) =>
    status ? `Connected via proxy (${ms} ms, HTTP ${status})` : `Connected via proxy (${ms} ms)`,
  proxyTestFailed: (msg) => `Proxy test failed: ${msg}`,
  proxyDomainRulesTitle: "Per-source routing rules",
  proxyDomainRulesDesc: "Override proxy routing for specific manga site domains.",
  proxyDomainPlaceholder: "Domain (e.g. example.com)",
  proxyRuleForceProxy: "Force Proxy",
  proxyRuleForceDirect: "Force Direct",
  proxyRuleCustomProxy: "Custom Proxy",
  proxyAddRule: "Add rule",
  proxyRemoveRule: "Remove",
  proxyNoRules: "No per-source overrides configured.",

  syncSection: "LAN Sync (Serverless)",
  syncSubtitle:
    "Direct device-to-device sync over local Wi-Fi/LAN. No backend server or account required.",
  syncDeviceLabel: (name) => `This device: ${name}`,
  syncPcSendTab: "Send to Phone (QR)",
  syncPcReceiveTab: "Receive from Phone (Code)",
  syncPhoneReceiveTab: "Receive from PC (Scan QR)",
  syncPhoneSendTab: "Send to PC (Code)",
  syncStartQrBtn: "Generate QR Code",
  syncStartPinBtn: "Generate Pairing Code",
  syncCancelBtn: "End Session",
  syncExpiresIn: (mmss) => `Expires in ${mmss}`,
  syncWaitingPeer: "Waiting for the other device on the same LAN...",
  syncCopyCode: "Copy fallback code",
  syncCopied: "Copied!",
  syncFallbackAddr: (addr) => `LAN address: ${addr}`,
  syncEnterPinLabel: "6-digit Pairing Code",
  syncEnterPinPlaceholder: "Enter 6-digit code",
  syncManualAddrToggle: "Auto-discovery blocked? Enter IP:Port manually",
  syncManualAddrPlaceholder: "e.g. 192.168.1.42:54321",
  syncConnectBtn: "Connect",
  syncConnecting: "Connecting...",
  syncStartCamera: "Scan QR with Camera",
  syncStopCamera: "Stop Camera",
  syncScanImageBtn: "Scan QR from Image",
  syncPasteCodePlaceholder: "Or paste fallback code from PC...",
  syncCameraError: "Could not access camera. Use 'Scan QR from Image' or paste the fallback code.",
  syncQrNotFound: "No valid RtMaga QR code found in the selected image.",
  syncConfirmTitle: "Confirm Data Transfer",
  syncConfirmSendDesc: (peer, ip, count) =>
    `Send ${count} manga entries and reading progress to "${peer}" (${ip})?`,
  syncConfirmReceiveDesc: (peer, ip, count) =>
    `Receive and merge ${count} manga entries from "${peer}" (${ip})? Newer chapters win automatically.`,
  syncApproveBtn: "Confirm & Sync",
  syncDeclineBtn: "Decline",
  syncWaitingOtherConfirm: "Waiting for the other device to confirm...",
  syncTransferring: "Transferring & merging reading history...",
  syncCompletedTitle: "Sync Completed",
  syncReportSummary: (added, updated, skipped, kept) =>
    `Added ${added} new • Updated ${updated} newer chapters • Skipped ${skipped} same • Kept ${kept} local ahead`,
  syncResetBtn: "Done",
  syncSecurityNote:
    "One-time session expires in 5 min • Max 5 failed attempts • Same LAN only • Data sync only, no remote control.",
};

const vi: Dictionary = {
  brandSubtitle: "Đọc truyện gọn nhẹ",
  navHome: "Trang chủ",
  navHistory: "Lịch sử",
  navSettings: "Cài đặt",
  checkUpdates: "Kiểm tra chap mới",
  checking: "Đang kiểm tra...",
  checkingProgress: (d, t) => `Đang kiểm tra (${d}/${t})`,
  checkUpdatesTooltip: "Kiểm tra chap mới cho các truyện trong lịch sử",
  lightMode: "Chế độ sáng",
  darkMode: "Chế độ tối",

  homeTitle: "Dán link truyện để đọc ngay",
  newChapters: "Có chap mới",
  continueReading: "Đọc tiếp",
  viewAllHistory: "Xem tất cả lịch sử",
  noRecentTitle: "Chưa có truyện nào gần đây",
  noRecentDesc:
    "Dán một link truyện vào ô phía trên để bắt đầu. Tiến độ đọc sẽ tự động lưu tại đây.",

  linkPlaceholder: "Dán link truyện hoặc link chương",
  fetching: "Đang lấy",
  open: "Mở truyện",
  openVerification: "Mở cửa sổ để tự xác minh",
  waitingVerification: "Đang chờ xác minh...",
  toastOpeningVerify: "Đang mở cửa sổ trình duyệt để bạn tự xác minh...",
  toastVerifyDone: "Đã xác minh xong, đang thử tải lại truyện...",

  badgeNew: "Chap mới",
  chaptersBtn: "Chương",
  backHome: "Trang chủ",
  refreshChapters: "Làm mới chương",
  refreshing: "Đang làm mới...",
  toastChaptersUpdated: "Đã cập nhật danh sách chương mới nhất.",
  chapterCount: (n) => `${n} chương`,
  continueChapter: (name) => `Đọc tiếp · ${name}`,
  startReading: "Đọc từ đầu",
  readFromFirst: "Đọc từ chương đầu",
  chaptersHeading: "Danh sách chương",
  filterChapters: "Tìm số chương...",
  newestFirst: "Mới nhất trước",
  oldestFirst: "Cũ nhất trước",

  back: "Quay lại",
  backTooltip: "Quay lại (Esc)",
  readingFallback: "Đang đọc",
  prevChapterTooltip: "Chap trước (←)",
  nextChapterTooltip: "Chap sau (→)",
  quickSettingsTooltip: "Tuỳ chỉnh nhanh",
  fullscreenTooltip: "Toàn màn hình (F)",
  imageWidth: "Độ rộng ảnh",
  fitWindow: "Vừa khung",
  autoAppendNext: "Tự nối chap tiếp theo",
  readerBg: "Nền trình đọc",
  bgDark: "Tối",
  bgLight: "Sáng",
  bgAppTheme: "Theo app",
  loadingPages: "Đang tải ảnh chương...",
  retry: "Thử lại",
  loadingNextChapter: "Đang nối chương tiếp theo...",
  nextChapter: "Đọc chương tiếp theo",
  reachedLatest: "Bạn đã đọc đến chương mới nhất",
  failedPage: (n) => `Không tải được trang ${n}`,
  pageAlt: (n) => `Trang ${n}`,

  historyTitle: "Lịch sử đọc",
  historySubtitle:
    "App tự động kiểm tra chap mới cho các truyện có mặt ở đây. Xoá khỏi lịch sử sẽ ngừng kiểm tra.",
  clearAll: "Xoá tất cả",
  confirmClear: "Xác nhận xoá hết",
  cancel: "Huỷ",
  historyEmptyTitle: "Lịch sử trống",
  historyEmptyDesc: "Truyện sẽ xuất hiện tại đây ngay khi bạn mở đọc chương đầu tiên.",
  today: "Hôm nay",
  yesterday: "Hôm qua",
  earlier: "Cũ hơn",
  removeHistoryTooltip: "Xoá khỏi lịch sử",
  toastRemovedHistory: "Đã xoá khỏi lịch sử và ngừng kiểm tra chap mới.",
  toastClearedHistory: "Đã xoá toàn bộ lịch sử đọc.",

  settingsTitle: "Cài đặt",
  settingsSubtitle: "Mọi thay đổi được lưu tự động và áp dụng ngay.",
  appearanceSection: "Giao diện",
  languageLabel: "Ngôn ngữ",
  languageDesc: "Chọn ngôn ngữ hiển thị cho giao diện, cài đặt và thông báo.",
  colorThemeLabel: "Chủ đề màu",
  colorThemeDesc: "Chọn giao diện sáng, tối hoặc tự động theo hệ thống.",
  themeSystem: "Hệ thống",
  themeLight: "Sáng",
  themeDark: "Tối",
  readerSection: "Trình đọc truyện",
  widthModeLabel: "Chế độ độ rộng ảnh",
  widthModeDesc: "Giữ cố định theo pixel, theo phần trăm hoặc vừa cửa sổ.",
  widthPixels: "Pixel",
  widthPercent: "Phần trăm",
  widthLabelPx: (px) => `Độ rộng (${px} px)`,
  widthLabelPct: (pct) => `Độ rộng (${pct}%)`,
  gapLabel: (px) => `Khoảng cách giữa các ảnh (${px} px)`,
  gapDesc: "Để 0 px để ảnh nối liền mạch theo chiều dọc.",
  preloadLabel: (n) => `Số ảnh tải trước (${n} ảnh)`,
  preloadDesc: "Tải sẵn ảnh phía dưới để cuộn mượt hơn.",
  autoAppendDesc: "Tắt đi nếu bạn muốn hiện nút bấm chuyển chương ở cuối trang.",
  updatesSection: "Kiểm tra chap mới",
  checkNow: "Kiểm tra ngay",
  autoCheckLabel: "Tự động kiểm tra truyện trong lịch sử",
  autoCheckDesc: "Chạy nền theo chu kỳ đã chọn.",
  checkIntervalLabel: "Chu kỳ kiểm tra",
  everyHours: (h) => `Mỗi ${h} giờ`,
  notificationsLabel: "Gửi thông báo hệ thống",
  notificationsDesc: "Hiện thông báo khi phát hiện chương mới.",
  sourcesSection: "Nguồn dữ liệu",
  headlessLabel: "Trình duyệt chạy ngầm",
  headlessDesc: "Tự động bật trên Windows khi gặp web tải ảnh bằng JavaScript.",
  cacheSection: "Bộ nhớ đệm ảnh",
  cacheUsageLabel: "Dung lượng đang dùng",
  cacheUsageDesc: (size, files) => `${size} (${files} ảnh đã lưu tạm)`,
  calculating: "Đang tính...",
  clearCache: "Xoá cache",
  clearing: "Đang xoá...",
  maxCacheLabel: (mb) => `Giới hạn tối đa (${mb} MB)`,
  toastCacheCleared: "Đã xoá bộ nhớ đệm ảnh.",
  toastCacheClearFailed: "Không thể xoá bộ nhớ đệm.",
  toastSettingsSaveFailed: "Không lưu được cài đặt.",
  dismissNotification: "Đóng thông báo",

  toastFoundOne: (title, latest) => `Chap mới: ${title} (${latest})`,
  toastFoundMany: (n) => `Có ${n} truyện vừa ra chap mới`,
  toastNoHistoryToCheck: "Chưa có truyện nào trong lịch sử để kiểm tra.",
  toastNoNewChapters: (c) => `Đã kiểm tra ${c} truyện — chưa có chap mới.`,
  toastCheckFailed: "Không thể kiểm tra chap mới lúc này.",

  appUpdatesSection: "Cập nhật ứng dụng",
  appVersionLabel: (v) => `Phiên bản hiện tại: v${v}`,
  checkAppUpdatesLabel: "Tự kiểm tra bản phát hành mới khi mở app",
  checkAppUpdatesDesc: "Tự động thông báo khi có phiên bản RtMaga mới hơn.",
  checkReleaseNow: "Kiểm tra bản mới",
  releaseAvailableBanner: (v) => `Đã có phiên bản RtMaga v${v}`,
  downloadUpdate: "Tải xuống",
  viewRelease: "Xem chi tiết",
  dismissUpdate: "Bỏ qua",
  releaseUpToDate: (v) => `Bạn đang dùng phiên bản mới nhất (v${v}).`,
  releaseCheckFailed: "Không thể kiểm tra bản phát hành mới lúc này.",
  releaseOpenFailed: "Không thể mở đường dẫn tải xuống.",
  updatePromptQuestion: "Bạn có muốn RtMaga tự động tải về và cài đặt bản cập nhật này ngay bây giờ không?",
  updateDataSafeNote:
    "Lịch sử đọc, tiến độ, danh sách truyện và cài đặt của bạn sẽ được tự động sao lưu và giữ nguyên.",
  updateDownloadAndInstall: "Tải và cài đặt",
  updateInstallNow: "Cài đặt ngay",
  updateLater: "Để sau",
  updateStageDownloading: "Đang tải bản cập nhật...",
  updateStageVerifying: "Đang kiểm tra toàn vẹn tệp...",
  updateStageBackingUp: "Đang sao lưu lịch sử & cài đặt...",
  updateStageInstalling: "Đang khởi chạy trình cài đặt...",
  updateStatusRestarting: "Đã mở trình cài đặt. RtMaga sẽ đóng và tự khởi động lại sau khi nâng cấp.",
  updateAndroidPermissionHint:
    "Hãy bật quyền 'Cài đặt ứng dụng không rõ nguồn gốc' cho RtMaga trong màn hình Cài đặt vừa mở, sau đó quay lại và bấm 'Cài đặt ngay' (tệp đã tải sẵn).",
  updateAndroidInstallerOpened:
    "Đã mở trình cài đặt hệ thống. Xác nhận cài đặt để nâng cấp mà không mất dữ liệu.",
  toastHistoryUpdateFailed: "Không thể cập nhật lịch sử đọc.",

  errInvalidUrlTitle: "Đường dẫn không hợp lệ",
  errInvalidUrlDetail: "Hãy dán link truyện hoặc link chương hợp lệ (bắt đầu bằng https://...).",
  errBlockedTitle: (host) => `Trang ${host} yêu cầu xác minh`,
  errBlockedDetail:
    "Trang web đang bật kiểm tra chống bot. Hãy mở cửa sổ để tự xác minh rồi thử lại.",
  errNotRecognizedTitle: "Chưa nhận diện được truyện từ link này",
  errNotRecognizedDetail:
    "Hãy thử dán trực tiếp link trang truyện hoặc trang chương, hoặc bật Headless trong Cài đặt.",
  errNetworkTitle: "Không kết nối được tới trang truyện",
  errNotFoundTitle: "Không tìm thấy dữ liệu",
  errAppTitle: "Lỗi ứng dụng",
  errGenericTitle: "Đã xảy ra lỗi",

  readingModeLabel: "Chế độ đọc",
  readingModeDesc: "Chuyển đổi giữa cuộn dọc và cuộn ngang theo trang hoặc nhóm trang.",
  modeVertical: "Cuộn dọc",
  modeHorizontal: "Cuộn ngang",
  verticalStyleLabel: "Kiểu cuộn dọc",
  verticalStyleDesc: "Chọn cuộn liên tục liền mạch hoặc gom nhiều trang thành từng nhóm.",
  verticalContinuous: "Cuộn liên tục",
  verticalGrouped: "Gom nhóm trang",
  horizontalDirLabel: "Hướng đọc ngang",
  dirLtr: "Trái sang phải (LTR)",
  dirRtl: "Phải sang trái (RTL)",
  pagesPerGroupLabel: "Số trang mỗi nhóm",
  pagesPerGroupDesc: "Hiển thị 1, 2, 3 hoặc số trang tuỳ chỉnh trong mỗi nhóm (1-8).",
  pageGapLabel: (px) => `Khoảng cách trong nhóm (${px} px)`,
  groupGapLabel: (px) => `Khoảng cách giữa các nhóm (${px} px)`,
  snapModeLabel: "Chế độ căn dừng (Snap)",
  snapNone: "Tự do",
  snapPage: "Theo trang",
  snapGroup: "Theo nhóm",
  groupSnapLabel: "Tự căn dừng theo nhóm",
  groupAlignLabel: "Căn lề nhóm trang",
  alignCenter: "Giữa khung",
  alignStart: "Đầu khung",
  centerGroupsLabel: "Căn giữa nhóm trang",
  centerGroupsDesc: "Căn giữa nhóm trang đang đọc theo chiều ngang màn hình.",
  dimUnfocusedLabel: "Làm tối trang ngoài vùng đọc",
  dimUnfocusedDesc: "Giảm độ sáng nhẹ cho các trang/nhóm nằm ngoài vùng đang tập trung.",
  dimLevelLabel: (pct) => `Mức độ làm tối (${pct}%)`,

  proxySection: "Proxy nội bộ (Vượt chặn IP vùng)",
  proxySubtitle:
    "Chỉ áp dụng trong RtMaga — không phải VPN hệ thống và không ảnh hưởng ứng dụng khác.",
  proxyDirect: "Trực tiếp",
  proxyEnabled: "Bật Proxy",
  proxyProtocolLabel: "Giao thức",
  proxyHostLabel: "Địa chỉ Proxy",
  proxyPortLabel: "Cổng (Port)",
  proxyUsernameLabel: "Tài khoản (tuỳ chọn)",
  proxyPasswordLabel: "Mật khẩu (tuỳ chọn)",
  proxyFailClosedLabel: "Cho phép chuyển sang kết nối trực tiếp khi Proxy lỗi",
  proxyFailClosedDesc:
    "Mặc định tắt (khuyên dùng) để tránh vô tình lộ IP thật khi Proxy gặp sự cố.",
  proxyTestUrlLabel: "Link kiểm tra",
  proxyTestBtn: "Kiểm tra kết nối",
  proxyTesting: "Đang kiểm tra...",
  proxyTestOk: (ms, status) =>
    status ? `Kết nối thành công (${ms} ms, HTTP ${status})` : `Kết nối thành công (${ms} ms)`,
  proxyTestFailed: (msg) => `Kiểm tra Proxy thất bại: ${msg}`,
  proxyDomainRulesTitle: "Quy tắc Proxy theo từng nguồn truyện",
  proxyDomainRulesDesc: "Thiết lập định tuyến riêng cho từng tên miền trang truyện.",
  proxyDomainPlaceholder: "Tên miền (vd: example.com)",
  proxyRuleForceProxy: "Luôn dùng Proxy",
  proxyRuleForceDirect: "Luôn trực tiếp",
  proxyRuleCustomProxy: "Proxy riêng",
  proxyAddRule: "Thêm quy tắc",
  proxyRemoveRule: "Xoá",
  proxyNoRules: "Chưa có quy tắc riêng theo nguồn nào.",

  syncSection: "Đồng bộ qua LAN (Không cần server)",
  syncSubtitle:
    "Truyền lịch sử đọc trực tiếp giữa các thiết bị cùng Wi-Fi/LAN. Không có backend hay tài khoản.",
  syncDeviceLabel: (name) => `Thiết bị này: ${name}`,
  syncPcSendTab: "Gửi sang điện thoại (QR)",
  syncPcReceiveTab: "Nhận từ điện thoại (Mã ghép nối)",
  syncPhoneReceiveTab: "Nhận từ máy tính (Quét QR)",
  syncPhoneSendTab: "Gửi sang máy tính (Mã ghép nối)",
  syncStartQrBtn: "Tạo mã QR đồng bộ",
  syncStartPinBtn: "Tạo mã ghép nối 6 số",
  syncCancelBtn: "Kết thúc phiên",
  syncExpiresIn: (mmss) => `Hết hạn sau ${mmss}`,
  syncWaitingPeer: "Đang chờ thiết bị kia kết nối trong cùng mạng LAN...",
  syncCopyCode: "Sao chép mã dự phòng",
  syncCopied: "Đã sao chép!",
  syncFallbackAddr: (addr) => `Địa chỉ LAN: ${addr}`,
  syncEnterPinLabel: "Mã ghép nối 6 chữ số",
  syncEnterPinPlaceholder: "Nhập mã 6 số",
  syncManualAddrToggle: "Không tự tìm thấy? Nhập IP:Port thủ công",
  syncManualAddrPlaceholder: "vd: 192.168.1.42:54321",
  syncConnectBtn: "Kết nối",
  syncConnecting: "Đang kết nối...",
  syncStartCamera: "Quét QR bằng Camera",
  syncStopCamera: "Tắt Camera",
  syncScanImageBtn: "Quét QR từ ảnh",
  syncPasteCodePlaceholder: "Hoặc dán mã dự phòng từ máy tính...",
  syncCameraError:
    "Không truy cập được camera. Hãy dùng 'Quét QR từ ảnh' hoặc dán mã dự phòng.",
  syncQrNotFound: "Không tìm thấy mã QR RtMaga hợp lệ trong ảnh đã chọn.",
  syncConfirmTitle: "Xác nhận truyền dữ liệu",
  syncConfirmSendDesc: (peer, ip, count) =>
    `Gửi ${count} truyện và tiến độ đọc sang "${peer}" (${ip})?`,
  syncConfirmReceiveDesc: (peer, ip, count) =>
    `Nhận và gộp ${count} truyện từ "${peer}" (${ip})? Truyện trùng sẽ giữ chương đọc mới hơn.`,
  syncApproveBtn: "Xác nhận & Đồng bộ",
  syncDeclineBtn: "Từ chối",
  syncWaitingOtherConfirm: "Đang chờ thiết bị kia bấm xác nhận...",
  syncTransferring: "Đang truyền và gộp lịch sử đọc...",
  syncCompletedTitle: "Đồng bộ hoàn tất",
  syncReportSummary: (added, updated, skipped, kept) =>
    `Thêm mới ${added} • Cập nhật ${updated} chap mới hơn • Bỏ qua ${skipped} trùng chap • Giữ ${kept} bản máy này mới hơn`,
  syncResetBtn: "Hoàn tất",
  syncSecurityNote:
    "Mã dùng 1 lần, tự huỷ sau 5 phút • Tối đa 5 lần nhập sai • Chỉ hoạt động cùng mạng LAN • Chỉ đồng bộ dữ liệu, không điều khiển thiết bị.",
};

const zh: Dictionary = {
  brandSubtitle: "轻量漫画阅读器",
  navHome: "首页",
  navHistory: "历史",
  navSettings: "设置",
  checkUpdates: "检查更新",
  checking: "正在检查...",
  checkingProgress: (d, t) => `正在检查 (${d}/${t})`,
  checkUpdatesTooltip: "检查历史记录中漫画的新章节",
  lightMode: "浅色模式",
  darkMode: "深色模式",

  homeTitle: "粘贴漫画链接即可开始阅读",
  newChapters: "有新章节",
  continueReading: "继续阅读",
  viewAllHistory: "查看全部历史",
  noRecentTitle: "暂无阅读记录",
  noRecentDesc: "在上方粘贴漫画链接开始阅读，您的进度会自动保存在这里。",

  linkPlaceholder: "粘贴漫画或章节链接",
  fetching: "获取中",
  open: "打开",
  openVerification: "打开验证窗口",
  waitingVerification: "等待验证中...",
  toastOpeningVerify: "正在打开浏览器窗口以便手动验证...",
  toastVerifyDone: "验证完成，正在重新获取漫画...",

  badgeNew: "更新",
  chaptersBtn: "章节",
  backHome: "首页",
  refreshChapters: "刷新章节",
  refreshing: "刷新中...",
  toastChaptersUpdated: "章节列表已更新。",
  chapterCount: (n) => `共 ${n} 话`,
  continueChapter: (name) => `继续阅读 · ${name}`,
  startReading: "开始阅读",
  readFromFirst: "从第一话开始",
  chaptersHeading: "章节列表",
  filterChapters: "搜索章节...",
  newestFirst: "最新优先",
  oldestFirst: "最早优先",

  back: "返回",
  backTooltip: "返回 (Esc)",
  readingFallback: "阅读中",
  prevChapterTooltip: "上一话 (←)",
  nextChapterTooltip: "下一话 (→)",
  quickSettingsTooltip: "快捷阅读设置",
  fullscreenTooltip: "切换全屏 (F)",
  imageWidth: "图片宽度",
  fitWindow: "适应窗口",
  autoAppendNext: "自动衔接下一话",
  readerBg: "阅读器背景",
  bgDark: "深色",
  bgLight: "浅色",
  bgAppTheme: "跟随应用",
  loadingPages: "正在加载章节图片...",
  retry: "重试",
  loadingNextChapter: "正在加载下一话...",
  nextChapter: "下一话",
  reachedLatest: "已阅读至最新一话",
  failedPage: (n) => `第 ${n} 页加载失败`,
  pageAlt: (n) => `第 ${n} 页`,

  historyTitle: "阅读历史",
  historySubtitle: "系统会自动检查此处漫画的更新，删除记录将停止检查。",
  clearAll: "全部清空",
  confirmClear: "确认清空",
  cancel: "取消",
  historyEmptyTitle: "暂无历史记录",
  historyEmptyDesc: "打开任意章节后，漫画会自动出现在这里。",
  today: "今天",
  yesterday: "昨天",
  earlier: "更早",
  removeHistoryTooltip: "从历史中删除",
  toastRemovedHistory: "已从历史记录移除并停止检查更新。",
  toastClearedHistory: "已清空全部阅读历史。",

  settingsTitle: "设置",
  settingsSubtitle: "所有更改会自动保存并立即生效。",
  appearanceSection: "外观",
  languageLabel: "语言",
  languageDesc: "选择界面、设置与通知的显示语言。",
  colorThemeLabel: "颜色主题",
  colorThemeDesc: "选择浅色、深色或跟随系统主题。",
  themeSystem: "跟随系统",
  themeLight: "浅色",
  themeDark: "深色",
  readerSection: "阅读器",
  widthModeLabel: "图片宽度模式",
  widthModeDesc: "按固定像素、窗口百分比或自适应窗口显示。",
  widthPixels: "像素",
  widthPercent: "百分比",
  widthLabelPx: (px) => `宽度 (${px} px)`,
  widthLabelPct: (pct) => `宽度 (${pct}%)`,
  gapLabel: (px) => `图片间距 (${px} px)`,
  gapDesc: "设为 0 px 可实现无间隙条漫阅读。",
  preloadLabel: (n) => `预加载页数 (${n} 张)`,
  preloadDesc: "提前加载下方图片以获得更顺滑的滚动体验。",
  autoAppendDesc: "关闭后将在章末显示“下一话”按钮。",
  updatesSection: "章节更新",
  checkNow: "立即检查",
  autoCheckLabel: "自动检查历史漫画更新",
  autoCheckDesc: "按选定周期在后台自动运行。",
  checkIntervalLabel: "检查周期",
  everyHours: (h) => `每 ${h} 小时`,
  notificationsLabel: "系统通知",
  notificationsDesc: "发现新章节时发送系统通知。",
  sourcesSection: "数据源",
  headlessLabel: "无头浏览器兜底",
  headlessDesc: "在 Windows 上遇到需要 JS 渲染的漫画网站时自动启用。",
  cacheSection: "图片缓存",
  cacheUsageLabel: "当前占用空间",
  cacheUsageDesc: (size, files) => `${size}（已缓存 ${files} 张图片）`,
  calculating: "计算中...",
  clearCache: "清除缓存",
  clearing: "清除中...",
  maxCacheLabel: (mb) => `最大缓存上限 (${mb} MB)`,
  toastCacheCleared: "图片缓存已清除。",
  toastCacheClearFailed: "无法清除图片缓存。",
  toastSettingsSaveFailed: "保存设置失败。",
  dismissNotification: "关闭通知",

  toastFoundOne: (title, latest) => `新章节：${title} (${latest})`,
  toastFoundMany: (n) => `有 ${n} 部漫画更新了新章节`,
  toastNoHistoryToCheck: "历史记录中暂无可检查的漫画。",
  toastNoNewChapters: (c) => `已检查 ${c} 部漫画，暂无更新。`,
  toastCheckFailed: "暂时无法检查更新。",

  appUpdatesSection: "应用更新",
  appVersionLabel: (v) => `当前版本：v${v}`,
  checkAppUpdatesLabel: "启动时自动检查新版本",
  checkAppUpdatesDesc: "发现更新的 RtMaga 正式版本时自动提示。",
  checkReleaseNow: "检查新版本",
  releaseAvailableBanner: (v) => `发现新版本 RtMaga v${v}`,
  downloadUpdate: "立即下载",
  viewRelease: "查看详情",
  dismissUpdate: "忽略",
  releaseUpToDate: (v) => `当前已是最新版本 (v${v})。`,
  releaseCheckFailed: "暂时无法检查新版本。",
  releaseOpenFailed: "无法打开下载链接。",
  updatePromptQuestion: "是否现在让 RtMaga 自动下载并安装此更新？",
  updateDataSafeNote: "您的阅读历史、进度、书架和设置将自动备份并完整保留。",
  updateDownloadAndInstall: "下载并安装",
  updateInstallNow: "立即安装",
  updateLater: "稍后",
  updateStageDownloading: "正在下载更新...",
  updateStageVerifying: "正在校验文件完整性...",
  updateStageBackingUp: "正在备份阅读历史与设置...",
  updateStageInstalling: "正在启动安装程序...",
  updateStatusRestarting: "安装程序已启动，RtMaga 将自动关闭并在更新后重新启动。",
  updateAndroidPermissionHint:
    "请在打开的设置页中允许 RtMaga“安装未知应用”，然后返回点击“立即安装”（安装包已下载完成）。",
  updateAndroidInstallerOpened: "已打开系统安装界面，确认安装即可保留数据升级。",
  toastHistoryUpdateFailed: "无法更新阅读历史。",

  errInvalidUrlTitle: "链接无效",
  errInvalidUrlDetail: "请粘贴有效的漫画或章节链接（以 https:// 开头）。",
  errBlockedTitle: (host) => `${host} 需要人机验证`,
  errBlockedDetail: "该网站启用了防爬虫验证，请打开浏览器窗口完成验证后重试。",
  errNotRecognizedTitle: "无法识别漫画内容",
  errNotRecognizedDetail: "请尝试粘贴漫画详情页或章节直链，或在设置中开启无头浏览器。",
  errNetworkTitle: "无法连接到网站",
  errNotFoundTitle: "未找到数据",
  errAppTitle: "应用错误",
  errGenericTitle: "发生错误",

  readingModeLabel: "阅读模式",
  readingModeDesc: "在竖向滚动与横向分页/分组滚动之间切换。",
  modeVertical: "竖向滚动",
  modeHorizontal: "横向滚动",
  verticalStyleLabel: "竖向排版",
  verticalStyleDesc: "选择无缝连续长图滚动或按页分组显示。",
  verticalContinuous: "连续长图",
  verticalGrouped: "分组页面",
  horizontalDirLabel: "翻页方向",
  dirLtr: "从左到右 (LTR)",
  dirRtl: "从右到左 (RTL)",
  pagesPerGroupLabel: "每组页数",
  pagesPerGroupDesc: "每组同时显示 1、2、3 或自定义页数 (1-8)。",
  pageGapLabel: (px) => `组内页距 (${px} px)`,
  groupGapLabel: (px) => `组间距离 (${px} px)`,
  snapModeLabel: "吸附对齐",
  snapNone: "自由",
  snapPage: "按页",
  snapGroup: "按组",
  groupSnapLabel: "按组吸附对齐",
  groupAlignLabel: "分组对齐方式",
  alignCenter: "居中",
  alignStart: "靠前/顶部",
  centerGroupsLabel: "页面组水平居中",
  centerGroupsDesc: "在视口中水平居中当前显示的页面组。",
  dimUnfocusedLabel: "调暗非聚焦页面",
  dimUnfocusedDesc: "柔和压暗当前阅读焦点之外的页面或分组。",
  dimLevelLabel: (pct) => `调暗强度 (${pct}%)`,

  proxySection: "应用内代理（突破地区限制）",
  proxySubtitle: "仅作用于 RtMaga 内部请求，不更改系统网络或影响其他应用。",
  proxyDirect: "直连",
  proxyEnabled: "代理模式",
  proxyProtocolLabel: "协议",
  proxyHostLabel: "代理地址",
  proxyPortLabel: "端口",
  proxyUsernameLabel: "用户名（可选）",
  proxyPasswordLabel: "密码（可选）",
  proxyFailClosedLabel: "代理失败时允许回退直连",
  proxyFailClosedDesc: "默认关闭（推荐），防止代理异常时意外暴露真实 IP。",
  proxyTestUrlLabel: "测试网址",
  proxyTestBtn: "测试连接",
  proxyTesting: "测试中...",
  proxyTestOk: (ms, status) =>
    status ? `代理连接成功 (${ms} ms, HTTP ${status})` : `代理连接成功 (${ms} ms)`,
  proxyTestFailed: (msg) => `代理测试失败：${msg}`,
  proxyDomainRulesTitle: "按来源域名路由规则",
  proxyDomainRulesDesc: "为特定漫画网站域名设置独立代理规则。",
  proxyDomainPlaceholder: "域名（如 example.com）",
  proxyRuleForceProxy: "强制代理",
  proxyRuleForceDirect: "强制直连",
  proxyRuleCustomProxy: "自定义代理",
  proxyAddRule: "添加规则",
  proxyRemoveRule: "移除",
  proxyNoRules: "暂未配置独立域名规则。",

  syncSection: "局域网同步（无服务器）",
  syncSubtitle: "在同一 Wi-Fi/局域网内设备间直连同步，无需后端或账号。",
  syncDeviceLabel: (name) => `当前设备：${name}`,
  syncPcSendTab: "发送到手机（二维码）",
  syncPcReceiveTab: "从手机接收（配对码）",
  syncPhoneReceiveTab: "从电脑接收（扫码）",
  syncPhoneSendTab: "发送到电脑（配对码）",
  syncStartQrBtn: "生成同步二维码",
  syncStartPinBtn: "生成 6 位配对码",
  syncCancelBtn: "结束会话",
  syncExpiresIn: (mmss) => `${mmss} 后过期`,
  syncWaitingPeer: "正在等待同一局域网内的另一台设备连接...",
  syncCopyCode: "复制备用码",
  syncCopied: "已复制！",
  syncFallbackAddr: (addr) => `局域网地址：${addr}`,
  syncEnterPinLabel: "6 位配对码",
  syncEnterPinPlaceholder: "输入 6 位配对码",
  syncManualAddrToggle: "无法自动发现？手动输入 IP:端口",
  syncManualAddrPlaceholder: "例：192.168.1.42:54321",
  syncConnectBtn: "连接",
  syncConnecting: "连接中...",
  syncStartCamera: "使用相机扫码",
  syncStopCamera: "关闭相机",
  syncScanImageBtn: "从相册图片扫码",
  syncPasteCodePlaceholder: "或粘贴来自电脑的备用同步码...",
  syncCameraError: "无法访问相机，请使用“从相册图片扫码”或粘贴备用码。",
  syncQrNotFound: "所选图片中未识别到有效的 RtMaga 二维码。",
  syncConfirmTitle: "确认数据传输",
  syncConfirmSendDesc: (peer, ip, count) =>
    `确定将 ${count} 部漫画的阅读记录发送到 "${peer}" (${ip}) 吗？`,
  syncConfirmReceiveDesc: (peer, ip, count) =>
    `确定接收并合并来自 "${peer}" (${ip}) 的 ${count} 部漫画记录吗？重复漫画会自动保留进度更新的章节。`,
  syncApproveBtn: "确认并同步",
  syncDeclineBtn: "拒绝",
  syncWaitingOtherConfirm: "正在等待对方设备确认...",
  syncTransferring: "正在传输并合并阅读历史...",
  syncCompletedTitle: "同步完成",
  syncReportSummary: (added, updated, skipped, kept) =>
    `新增 ${added} • 更新 ${updated} 个新章节 • 跳过 ${skipped} 个相同章节 • 保留本地较新 ${kept} 部`,
  syncResetBtn: "完成",
  syncSecurityNote:
    "一次性会话 5 分钟后自动销毁 • 最多允许 5 次错误 • 仅限同局域网 • 仅同步数据，无远程控制权限。",
};

const ja: Dictionary = {
  brandSubtitle: "軽量マンガリーダー",
  navHome: "ホーム",
  navHistory: "履歴",
  navSettings: "設定",
  checkUpdates: "更新を確認",
  checking: "確認中...",
  checkingProgress: (d, t) => `確認中 (${d}/${t})`,
  checkUpdatesTooltip: "履歴にある作品の最新話を確認します",
  lightMode: "ライトモード",
  darkMode: "ダークモード",

  homeTitle: "マンガのリンクを貼ってすぐ読む",
  newChapters: "新着チャプター",
  continueReading: "続きを読む",
  viewAllHistory: "履歴をすべて見る",
  noRecentTitle: "最近読んだ作品はありません",
  noRecentDesc: "上の入力欄にリンクを貼ると読み始められ、進捗がここに保存されます。",

  linkPlaceholder: "作品またはチャプターの URL を貼り付け",
  fetching: "取得中",
  open: "開く",
  openVerification: "認証ウィンドウを開く",
  waitingVerification: "認証を待機中...",
  toastOpeningVerify: "手動認証用のブラウザウィンドウを開いています...",
  toastVerifyDone: "認証が完了しました。再取得しています...",

  badgeNew: "新着",
  chaptersBtn: "話一覧",
  backHome: "ホーム",
  refreshChapters: "話一覧を更新",
  refreshing: "更新中...",
  toastChaptersUpdated: "チャプター一覧を更新しました。",
  chapterCount: (n) => `全 ${n} 話`,
  continueChapter: (name) => `続きを読む · ${name}`,
  startReading: "最初から読む",
  readFromFirst: "第1話から読む",
  chaptersHeading: "チャプター一覧",
  filterChapters: "チャプターを検索...",
  newestFirst: "新しい順",
  oldestFirst: "古い順",

  back: "戻る",
  backTooltip: "戻る (Esc)",
  readingFallback: "閲覧中",
  prevChapterTooltip: "前の話 (←)",
  nextChapterTooltip: "次の話 (→)",
  quickSettingsTooltip: "クイック設定",
  fullscreenTooltip: "全画面切替 (F)",
  imageWidth: "画像の幅",
  fitWindow: "ウィンドウに合わせる",
  autoAppendNext: "次の話を自動で繋げる",
  readerBg: "リーダー背景",
  bgDark: "ダーク",
  bgLight: "ライト",
  bgAppTheme: "アプリに合わせる",
  loadingPages: "ページを読み込み中...",
  retry: "再試行",
  loadingNextChapter: "次の話を読み込み中...",
  nextChapter: "次の話へ",
  reachedLatest: "最新話まで読み終わりました",
  failedPage: (n) => `${n} ページ目の読み込みに失敗しました`,
  pageAlt: (n) => `ページ ${n}`,

  historyTitle: "閲覧履歴",
  historySubtitle: "ここにある作品は自動で新着確認されます。履歴から削除すると確認を停止します。",
  clearAll: "すべて削除",
  confirmClear: "削除を確定",
  cancel: "キャンセル",
  historyEmptyTitle: "履歴はありません",
  historyEmptyDesc: "チャプターを開くと自動的にここへ記録されます。",
  today: "今日",
  yesterday: "昨日",
  earlier: "それ以前",
  removeHistoryTooltip: "履歴から削除",
  toastRemovedHistory: "履歴から削除し、更新確認を停止しました。",
  toastClearedHistory: "すべての履歴を削除しました。",

  settingsTitle: "設定",
  settingsSubtitle: "すべての変更は自動保存され、すぐに反映されます。",
  appearanceSection: "外観",
  languageLabel: "言語",
  languageDesc: "画面・設定・通知の表示言語を選択します。",
  colorThemeLabel: "カラーテーマ",
  colorThemeDesc: "ライト、ダーク、またはシステム設定に従います。",
  themeSystem: "システム",
  themeLight: "ライト",
  themeDark: "ダーク",
  readerSection: "リーダー",
  widthModeLabel: "画像幅モード",
  widthModeDesc: "固定ピクセル、画面比率、またはウィンドウ幅に合わせます。",
  widthPixels: "ピクセル",
  widthPercent: "パーセント",
  widthLabelPx: (px) => `幅 (${px} px)`,
  widthLabelPct: (pct) => `幅 (${pct}%)`,
  gapLabel: (px) => `画像の間隔 (${px} px)`,
  gapDesc: "0 px にすると縦スクロールで隙間なく表示されます。",
  preloadLabel: (n) => `先読み枚数 (${n} 枚)`,
  preloadDesc: "スクロール先の画像を事前に読み込んで滑らかにします。",
  autoAppendDesc: "オフにすると末尾に「次の話へ」ボタンを表示します。",
  updatesSection: "新着確認",
  checkNow: "今すぐ確認",
  autoCheckLabel: "履歴の作品を自動確認",
  autoCheckDesc: "指定した間隔でバックグラウンド確認します。",
  checkIntervalLabel: "確認間隔",
  everyHours: (h) => `${h} 時間ごと`,
  notificationsLabel: "デスクトップ通知",
  notificationsDesc: "新着チャプターが見つかった時に通知を表示します。",
  sourcesSection: "データソース",
  headlessLabel: "ヘッドレスブラウザ補助",
  headlessDesc: "Windows で JS 描画が必要なサイトの場合に自動使用します。",
  cacheSection: "画像キャッシュ",
  cacheUsageLabel: "現在の使用容量",
  cacheUsageDesc: (size, files) => `${size}（${files} 枚の画像）`,
  calculating: "計算中...",
  clearCache: "キャッシュ削除",
  clearing: "削除中...",
  maxCacheLabel: (mb) => `最大キャッシュ容量 (${mb} MB)`,
  toastCacheCleared: "画像キャッシュを削除しました。",
  toastCacheClearFailed: "キャッシュの削除に失敗しました。",
  toastSettingsSaveFailed: "設定の保存に失敗しました。",
  dismissNotification: "通知を閉じる",

  toastFoundOne: (title, latest) => `新着: ${title} (${latest})`,
  toastFoundMany: (n) => `${n} 作品に新しいチャプターがあります`,
  toastNoHistoryToCheck: "確認対象の履歴がまだありません。",
  toastNoNewChapters: (c) => `${c} 作品を確認しました — 新着はありません。`,
  toastCheckFailed: "更新確認に失敗しました。",

  appUpdatesSection: "アプリの更新",
  appVersionLabel: (v) => `現在のバージョン: v${v}`,
  checkAppUpdatesLabel: "起動時に新しいリリースを自動確認",
  checkAppUpdatesDesc: "新しい RtMaga が公開されたときに自動で通知します。",
  checkReleaseNow: "リリースを確認",
  releaseAvailableBanner: (v) => `RtMaga v${v} が利用可能です`,
  downloadUpdate: "ダウンロード",
  viewRelease: "リリースを見る",
  dismissUpdate: "閉じる",
  releaseUpToDate: (v) => `最新バージョン (v${v}) を使用中です。`,
  releaseCheckFailed: "現在リリース情報を確認できません。",
  releaseOpenFailed: "リリースリンクを開けませんでした。",
  updatePromptQuestion: "RtMaga でこのアップデートを自動ダウンロードしてインストールしますか？",
  updateDataSafeNote: "閲覧履歴・進捗・設定は自動的にバックアップされ、そのまま保持されます。",
  updateDownloadAndInstall: "ダウンロードしてインストール",
  updateInstallNow: "今すぐインストール",
  updateLater: "あとで",
  updateStageDownloading: "アップデートをダウンロード中...",
  updateStageVerifying: "ファイルの整合性を検証中...",
  updateStageBackingUp: "履歴と設定をバックアップ中...",
  updateStageInstalling: "インストーラーを起動中...",
  updateStatusRestarting: "インストーラーを起動しました。完了後に自動で再起動します。",
  updateAndroidPermissionHint:
    "開いた設定画面で RtMaga に「不明なアプリのインストール」を許可してから戻り、「今すぐインストール」を押してください（ダウンロード済み）。",
  updateAndroidInstallerOpened:
    "システムのインストーラーを開きました。そのまま更新してもデータは保持されます。",
  toastHistoryUpdateFailed: "履歴を更新できませんでした。",

  errInvalidUrlTitle: "無効な URL",
  errInvalidUrlDetail: "https:// から始まる有効な作品・話リンクを貼り付けてください。",
  errBlockedTitle: (host) => `${host} で認証が必要です`,
  errBlockedDetail:
    "ボット対策が有効です。ブラウザウィンドウで認証後に再試行してください。",
  errNotRecognizedTitle: "マンガを認識できませんでした",
  errNotRecognizedDetail:
    "作品ページまたは話ページを直接貼るか、設定でヘッドレスブラウザを有効にしてください。",
  errNetworkTitle: "サイトに接続できませんでした",
  errNotFoundTitle: "データが見つかりません",
  errAppTitle: "アプリエラー",
  errGenericTitle: "エラーが発生しました",

  readingModeLabel: "閲覧モード",
  readingModeDesc: "縦スクロールと横スクロール（見開き/グループ表示）を切り替えます。",
  modeVertical: "縦スクロール",
  modeHorizontal: "横スクロール",
  verticalStyleLabel: "縦レイアウト",
  verticalStyleDesc: "連続スクロールまたは複数ページをまとめたグループ表示を選択します。",
  verticalContinuous: "連続表示",
  verticalGrouped: "グループ表示",
  horizontalDirLabel: "読む方向",
  dirLtr: "左から右 (LTR)",
  dirRtl: "右から左 (RTL)",
  pagesPerGroupLabel: "1グループあたりのページ数",
  pagesPerGroupDesc: "1、2、3 ページまたはカスタム数 (1-8) をまとめて表示します。",
  pageGapLabel: (px) => `グループ内の間隔 (${px} px)`,
  groupGapLabel: (px) => `グループ間の間隔 (${px} px)`,
  snapModeLabel: "スナップ動作",
  snapNone: "自由",
  snapPage: "ページ",
  snapGroup: "グループ",
  groupSnapLabel: "グループ単位でスナップ",
  groupAlignLabel: "グループ配置",
  alignCenter: "中央",
  alignStart: "先頭",
  centerGroupsLabel: "グループを中央揃え",
  centerGroupsDesc: "現在のページグループを画面中央に配置します。",
  dimUnfocusedLabel: "フォーカス外を暗くする",
  dimUnfocusedDesc: "現在読んでいる領域以外のページを控えめに暗くします。",
  dimLevelLabel: (pct) => `暗さレベル (${pct}%)`,

  proxySection: "アプリ内プロキシ（地域制限回避）",
  proxySubtitle: "RtMaga 内の通信のみ対象。OS の VPN 設定や他アプリには影響しません。",
  proxyDirect: "直接接続",
  proxyEnabled: "プロキシ",
  proxyProtocolLabel: "プロトコル",
  proxyHostLabel: "プロキシホスト",
  proxyPortLabel: "ポート",
  proxyUsernameLabel: "ユーザー名（任意）",
  proxyPasswordLabel: "パスワード（任意）",
  proxyFailClosedLabel: "プロキシ失敗時に直接接続へフォールバック",
  proxyFailClosedDesc:
    "既定ではオフ（推奨）。プロキシ障害時に実 IP が漏れるのを防ぎます。",
  proxyTestUrlLabel: "テスト URL",
  proxyTestBtn: "接続テスト",
  proxyTesting: "テスト中...",
  proxyTestOk: (ms, status) =>
    status ? `プロキシ接続成功 (${ms} ms, HTTP ${status})` : `プロキシ接続成功 (${ms} ms)`,
  proxyTestFailed: (msg) => `プロキシテスト失敗: ${msg}`,
  proxyDomainRulesTitle: "ドメイン別ルーティング",
  proxyDomainRulesDesc: "特定のマンガサイトのドメインごとに接続方法を指定します。",
  proxyDomainPlaceholder: "ドメイン (例: example.com)",
  proxyRuleForceProxy: "常にプロキシ",
  proxyRuleForceDirect: "常に直接",
  proxyRuleCustomProxy: "カスタムプロキシ",
  proxyAddRule: "ルール追加",
  proxyRemoveRule: "削除",
  proxyNoRules: "ドメイン別の個別ルールはありません。",

  syncSection: "LAN 同期（サーバーレス）",
  syncSubtitle: "同じ Wi-Fi/LAN 内の端末同士で直接同期。外部サーバーやアカウント不要。",
  syncDeviceLabel: (name) => `この端末: ${name}`,
  syncPcSendTab: "スマホへ送信 (QR)",
  syncPcReceiveTab: "スマホから受信 (コード)",
  syncPhoneReceiveTab: "PC から受信 (QR スキャン)",
  syncPhoneSendTab: "PC へ送信 (コード)",
  syncStartQrBtn: "QR コードを生成",
  syncStartPinBtn: "6 桁のペアリングコードを生成",
  syncCancelBtn: "セッション終了",
  syncExpiresIn: (mmss) => `残り ${mmss}`,
  syncWaitingPeer: "同じ LAN 内の相手端末を待機中...",
  syncCopyCode: "予備コードをコピー",
  syncCopied: "コピーしました！",
  syncFallbackAddr: (addr) => `LAN アドレス: ${addr}`,
  syncEnterPinLabel: "6 桁のペアリングコード",
  syncEnterPinPlaceholder: "6 桁のコードを入力",
  syncManualAddrToggle: "自動検出できない場合は IP:ポートを手動入力",
  syncManualAddrPlaceholder: "例: 192.168.1.42:54321",
  syncConnectBtn: "接続",
  syncConnecting: "接続中...",
  syncStartCamera: "カメラで QR をスキャン",
  syncStopCamera: "カメラ停止",
  syncScanImageBtn: "画像から QR を読み取る",
  syncPasteCodePlaceholder: "または PC の予備コードを貼り付け...",
  syncCameraError:
    "カメラにアクセスできません。「画像から QR を読み取る」か予備コードを使用してください。",
  syncQrNotFound: "選択した画像から有効な RtMaga QR コードが見つかりませんでした。",
  syncConfirmTitle: "データ転送の確認",
  syncConfirmSendDesc: (peer, ip, count) =>
    `${count} 作品の閲覧履歴を "${peer}" (${ip}) へ送信しますか？`,
  syncConfirmReceiveDesc: (peer, ip, count) =>
    `"${peer}" (${ip}) から ${count} 作品の履歴を受信して統合しますか？より進んだ話が優先されます。`,
  syncApproveBtn: "確認して同期",
  syncDeclineBtn: "拒否",
  syncWaitingOtherConfirm: "相手端末の確認を待っています...",
  syncTransferring: "履歴を転送・統合しています...",
  syncCompletedTitle: "同期完了",
  syncReportSummary: (added, updated, skipped, kept) =>
    `新規 ${added} • 更新 ${updated} • 同一スキップ ${skipped} • ローカル優先 ${kept}`,
  syncResetBtn: "完了",
  syncSecurityNote:
    "ワンタイムコードは 5 分で失効 • 最大 5 回まで再試行可 • 同一 LAN 限定 • 同期データ専用（遠隔操作不可）。",
};

const ru: Dictionary = {
  brandSubtitle: "Лёгкая читалка манги",
  navHome: "Главная",
  navHistory: "История",
  navSettings: "Настройки",
  checkUpdates: "Проверить главы",
  checking: "Проверка...",
  checkingProgress: (d, t) => `Проверка (${d}/${t})`,
  checkUpdatesTooltip: "Проверить новые главы для манги из истории",
  lightMode: "Светлая тема",
  darkMode: "Тёмная тема",

  homeTitle: "Вставьте ссылку на мангу, чтобы начать читать",
  newChapters: "Новые главы",
  continueReading: "Продолжить чтение",
  viewAllHistory: "Вся история",
  noRecentTitle: "Недавней манги пока нет",
  noRecentDesc:
    "Вставьте ссылку выше, чтобы начать. Прогресс чтения будет сохраняться здесь автоматически.",

  linkPlaceholder: "Вставьте ссылку на мангу или главу",
  fetching: "Загрузка",
  open: "Открыть",
  openVerification: "Открыть окно проверки",
  waitingVerification: "Ожидание проверки...",
  toastOpeningVerify: "Открывается окно браузера для ручной проверки...",
  toastVerifyDone: "Проверка завершена. Повторная загрузка...",

  badgeNew: "Новое",
  chaptersBtn: "Главы",
  backHome: "Главная",
  refreshChapters: "Обновить главы",
  refreshing: "Обновление...",
  toastChaptersUpdated: "Список глав обновлён.",
  chapterCount: (n) => `Глав: ${n}`,
  continueChapter: (name) => `Продолжить · ${name}`,
  startReading: "Начать чтение",
  readFromFirst: "Читать с 1-й главы",
  chaptersHeading: "Главы",
  filterChapters: "Поиск главы...",
  newestFirst: "Сначала новые",
  oldestFirst: "Сначала старые",

  back: "Назад",
  backTooltip: "Назад (Esc)",
  readingFallback: "Чтение",
  prevChapterTooltip: "Предыдущая глава (←)",
  nextChapterTooltip: "Следующая глава (→)",
  quickSettingsTooltip: "Быстрые настройки",
  fullscreenTooltip: "Полный экран (F)",
  imageWidth: "Ширина страниц",
  fitWindow: "По ширине окна",
  autoAppendNext: "Автоподгрузка следующей главы",
  readerBg: "Фон читалки",
  bgDark: "Тёмный",
  bgLight: "Светлый",
  bgAppTheme: "Как в теме",
  loadingPages: "Загрузка страниц главы...",
  retry: "Повторить",
  loadingNextChapter: "Загрузка следующей главы...",
  nextChapter: "Следующая глава",
  reachedLatest: "Вы дочитали до последней главы",
  failedPage: (n) => `Не удалось загрузить страницу ${n}`,
  pageAlt: (n) => `Страница ${n}`,

  historyTitle: "История чтения",
  historySubtitle:
    "Новые главы автоматически проверяются для манги из этого списка. Удаление останавливает проверку.",
  clearAll: "Очистить всё",
  confirmClear: "Подтвердить",
  cancel: "Отмена",
  historyEmptyTitle: "История пуста",
  historyEmptyDesc: "Манга появится здесь сразу после открытия первой главы.",
  today: "Сегодня",
  yesterday: "Вчера",
  earlier: "Ранее",
  removeHistoryTooltip: "Удалить из истории",
  toastRemovedHistory: "Удалено из истории, проверка обновлений остановлена.",
  toastClearedHistory: "История чтения очищена.",

  settingsTitle: "Настройки",
  settingsSubtitle: "Все изменения сохраняются автоматически и применяются сразу.",
  appearanceSection: "Внешний вид",
  languageLabel: "Язык",
  languageDesc: "Выберите язык интерфейса, настроек и уведомлений.",
  colorThemeLabel: "Цветовая тема",
  colorThemeDesc: "Светлая, тёмная или по системным настройкам.",
  themeSystem: "Системная",
  themeLight: "Светлая",
  themeDark: "Тёмная",
  readerSection: "Читалка",
  widthModeLabel: "Режим ширины страниц",
  widthModeDesc: "Фиксированная ширина в пикселях, процентах или по окну.",
  widthPixels: "Пиксели",
  widthPercent: "Проценты",
  widthLabelPx: (px) => `Ширина (${px} px)`,
  widthLabelPct: (pct) => `Ширина (${pct}%)`,
  gapLabel: (px) => `Отступ между страницами (${px} px)`,
  gapDesc: "0 px — бесшовная вертикальная лента.",
  preloadLabel: (n) => `Предзагрузка (${n} стр.)`,
  preloadDesc: "Загружает следующие страницы заранее для плавной прокрутки.",
  autoAppendDesc: "Если выключено, в конце главы появится кнопка перехода.",
  updatesSection: "Обновления глав",
  checkNow: "Проверить сейчас",
  autoCheckLabel: "Автопроверка манги из истории",
  autoCheckDesc: "Выполняется в фоне через выбранный интервал.",
  checkIntervalLabel: "Интервал проверки",
  everyHours: (h) => `Каждые ${h} ч.`,
  notificationsLabel: "Уведомления ОС",
  notificationsDesc: "Показывать уведомление при выходе новых глав.",
  sourcesSection: "Источники",
  headlessLabel: "Фоновый браузер",
  headlessDesc: "Автоматически используется на Windows для сайтов с JS-рендерингом.",
  cacheSection: "Кэш изображений",
  cacheUsageLabel: "Занято на диске",
  cacheUsageDesc: (size, files) => `${size} (файлов: ${files})`,
  calculating: "Подсчёт...",
  clearCache: "Очистить кэш",
  clearing: "Очистка...",
  maxCacheLabel: (mb) => `Максимум кэша (${mb} МБ)`,
  toastCacheCleared: "Кэш изображений очищен.",
  toastCacheClearFailed: "Не удалось очистить кэш.",
  toastSettingsSaveFailed: "Не удалось сохранить настройки.",
  dismissNotification: "Закрыть уведомление",

  toastFoundOne: (title, latest) => `Новая глава: ${title} (${latest})`,
  toastFoundMany: (n) => `Новые главы у ${n} манг`,
  toastNoHistoryToCheck: "В истории пока нет манги для проверки.",
  toastNoNewChapters: (c) => `Проверено: ${c} — новых глав пока нет.`,
  toastCheckFailed: "Не удалось проверить обновления.",

  appUpdatesSection: "Обновления приложения",
  appVersionLabel: (v) => `Текущая версия: v${v}`,
  checkAppUpdatesLabel: "Проверять новые релизы при запуске",
  checkAppUpdatesDesc: "Автоматически уведомлять о выходе новой версии RtMaga.",
  checkReleaseNow: "Проверить релиз",
  releaseAvailableBanner: (v) => `Доступна новая версия RtMaga v${v}`,
  downloadUpdate: "Скачать",
  viewRelease: "Подробнее",
  dismissUpdate: "Скрыть",
  releaseUpToDate: (v) => `Установлена последняя версия (v${v}).`,
  releaseCheckFailed: "Не удалось проверить наличие новых релизов.",
  releaseOpenFailed: "Не удалось открыть ссылку на релиз.",
  updatePromptQuestion: "Скачать и установить это обновление автоматически прямо сейчас?",
  updateDataSafeNote:
    "История чтения, прогресс и настройки автоматически сохраняются и останутся нетронутыми.",
  updateDownloadAndInstall: "Скачать и установить",
  updateInstallNow: "Установить сейчас",
  updateLater: "Позже",
  updateStageDownloading: "Загрузка обновления...",
  updateStageVerifying: "Проверка целостности файла...",
  updateStageBackingUp: "Резервное копирование истории и настроек...",
  updateStageInstalling: "Запуск установщика...",
  updateStatusRestarting: "Установщик запущен. RtMaga закроется и перезапустится автоматически.",
  updateAndroidPermissionHint:
    "Разрешите RtMaga «Установку неизвестных приложений» в открывшихся настройках, затем вернитесь и нажмите «Установить сейчас» (файл уже загружен).",
  updateAndroidInstallerOpened:
    "Открыт системный установщик. Подтвердите установку — ваши данные сохранятся.",
  toastHistoryUpdateFailed: "Не удалось обновить историю чтения.",

  errInvalidUrlTitle: "Некорректная ссылка",
  errInvalidUrlDetail: "Вставьте корректную ссылку на мангу или главу (начинается с https://...).",
  errBlockedTitle: (host) => `${host} требует подтверждения`,
  errBlockedDetail:
    "На сайте включена защита от ботов. Пройдите проверку в открывшемся окне и повторите.",
  errNotRecognizedTitle: "Не удалось распознать мангу",
  errNotRecognizedDetail:
    "Попробуйте прямую ссылку на мангу/главу или включите фоновый браузер в настройках.",
  errNetworkTitle: "Ошибка подключения к сайту",
  errNotFoundTitle: "Не найдено",
  errAppTitle: "Ошибка приложения",
  errGenericTitle: "Произошла ошибка",

  readingModeLabel: "Режим чтения",
  readingModeDesc: "Переключение между вертикальной и горизонтальной прокруткой.",
  modeVertical: "Вертикальный",
  modeHorizontal: "Горизонтальный",
  verticalStyleLabel: "Вертикальная раскладка",
  verticalStyleDesc: "Бесшовная лента или группировка нескольких страниц.",
  verticalContinuous: "Непрерывная лента",
  verticalGrouped: "Группы страниц",
  horizontalDirLabel: "Направление чтения",
  dirLtr: "Слева направо (LTR)",
  dirRtl: "Справа налево (RTL)",
  pagesPerGroupLabel: "Страниц в группе",
  pagesPerGroupDesc: "Отображение 1, 2, 3 или произвольного числа страниц вместе (1-8).",
  pageGapLabel: (px) => `Отступ внутри группы (${px} px)`,
  groupGapLabel: (px) => `Отступ между группами (${px} px)`,
  snapModeLabel: "Привязка (Snap)",
  snapNone: "Свободно",
  snapPage: "Страница",
  snapGroup: "Группа",
  groupSnapLabel: "Привязка к группам",
  groupAlignLabel: "Выравнивание групп",
  alignCenter: "По центру",
  alignStart: "По верху / началу",
  centerGroupsLabel: "Центрировать группы",
  centerGroupsDesc: "Центрировать активную группу страниц по горизонтали.",
  dimUnfocusedLabel: "Затемнять неактивные страницы",
  dimUnfocusedDesc: "Мягко затемнять страницы вне текущей области чтения.",
  dimLevelLabel: (pct) => `Интенсивность затемнения (${pct}%)`,

  proxySection: "Встроенный прокси (обход геоблокировок)",
  proxySubtitle: "Только внутри RtMaga — не изменяет системный VPN и другие приложения.",
  proxyDirect: "Напрямую",
  proxyEnabled: "Прокси",
  proxyProtocolLabel: "Протокол",
  proxyHostLabel: "Хост прокси",
  proxyPortLabel: "Порт",
  proxyUsernameLabel: "Логин (необязательно)",
  proxyPasswordLabel: "Пароль (необязательно)",
  proxyFailClosedLabel: "Разрешить прямой переход при сбое прокси",
  proxyFailClosedDesc: "Выключено по умолчанию (рекомендуется), чтобы не раскрыть реальный IP.",
  proxyTestUrlLabel: "Тестовый URL",
  proxyTestBtn: "Проверить соединение",
  proxyTesting: "Проверка...",
  proxyTestOk: (ms, status) =>
    status ? `Подключено через прокси (${ms} мс, HTTP ${status})` : `Подключено (${ms} мс)`,
  proxyTestFailed: (msg) => `Ошибка прокси: ${msg}`,
  proxyDomainRulesTitle: "Правила по доменам",
  proxyDomainRulesDesc: "Индивидуальные правила маршрутизации для отдельных сайтов.",
  proxyDomainPlaceholder: "Домен (напр. example.com)",
  proxyRuleForceProxy: "Всегда прокси",
  proxyRuleForceDirect: "Всегда напрямую",
  proxyRuleCustomProxy: "Свой прокси",
  proxyAddRule: "Добавить",
  proxyRemoveRule: "Удалить",
  proxyNoRules: "Индивидуальных правил для доменов нет.",

  syncSection: "LAN-синхронизация (без сервера)",
  syncSubtitle: "Прямой обмен историей по локальной сети Wi-Fi/LAN. Без облака и аккаунтов.",
  syncDeviceLabel: (name) => `Это устройство: ${name}`,
  syncPcSendTab: "Отправить на телефон (QR)",
  syncPcReceiveTab: "Получить с телефона (Код)",
  syncPhoneReceiveTab: "Получить с ПК (QR)",
  syncPhoneSendTab: "Отправить на ПК (Код)",
  syncStartQrBtn: "Создать QR-код",
  syncStartPinBtn: "Создать 6-значный код",
  syncCancelBtn: "Завершить сессию",
  syncExpiresIn: (mmss) => `Истекает через ${mmss}`,
  syncWaitingPeer: "Ожидание другого устройства в той же сети LAN...",
  syncCopyCode: "Скопировать резервный код",
  syncCopied: "Скопировано!",
  syncFallbackAddr: (addr) => `Адрес LAN: ${addr}`,
  syncEnterPinLabel: "6-значный код сопряжения",
  syncEnterPinPlaceholder: "Введите 6 цифр",
  syncManualAddrToggle: "Не найдено автоматически? Ввести IP:Port вручную",
  syncManualAddrPlaceholder: "напр. 192.168.1.42:54321",
  syncConnectBtn: "Подключить",
  syncConnecting: "Подключение...",
  syncStartCamera: "Сканировать QR камерой",
  syncStopCamera: "Остановить камеру",
  syncScanImageBtn: "Сканировать QR с фото",
  syncPasteCodePlaceholder: "Или вставьте резервный код с ПК...",
  syncCameraError:
    "Нет доступа к камере. Выберите «Сканировать QR с фото» или вставьте резервный код.",
  syncQrNotFound: "На выбранном изображении не найден действительный QR-код RtMaga.",
  syncConfirmTitle: "Подтвердите передачу данных",
  syncConfirmSendDesc: (peer, ip, count) =>
    `Отправить ${count} записей истории чтения на «${peer}» (${ip})?`,
  syncConfirmReceiveDesc: (peer, ip, count) =>
    `Принять и объединить ${count} записей от «${peer}» (${ip})? Сохраняются более новые главы.`,
  syncApproveBtn: "Подтвердить и синхронизировать",
  syncDeclineBtn: "Отклонить",
  syncWaitingOtherConfirm: "Ожидание подтверждения на другом устройстве...",
  syncTransferring: "Передача и объединение истории чтения...",
  syncCompletedTitle: "Синхронизация завершена",
  syncReportSummary: (added, updated, skipped, kept) =>
    `Добавлено ${added} • Обновлено ${updated} • Пропущено ${skipped} • Локально новее ${kept}`,
  syncResetBtn: "Готово",
  syncSecurityNote:
    "Одноразовая сессия истекает через 5 мин • Макс. 5 попыток • Только одна сеть LAN • Только данные чтения.",
};

const ko: Dictionary = {
  brandSubtitle: "깔끔한 만화 리더",
  navHome: "홈",
  navHistory: "기록",
  navSettings: "설정",
  checkUpdates: "업데이트 확인",
  checking: "확인 중...",
  checkingProgress: (d, t) => `확인 중 (${d}/${t})`,
  checkUpdatesTooltip: "기록에 있는 만화의 새 챕터를 확인합니다",
  lightMode: "라이트 모드",
  darkMode: "다크 모드",

  homeTitle: "만화 링크를 붙여넣어 바로 읽기",
  newChapters: "새 챕터",
  continueReading: "이어서 읽기",
  viewAllHistory: "전체 기록 보기",
  noRecentTitle: "최근 읽은 만화가 없습니다",
  noRecentDesc: "위에 만화 링크를 붙여넣어 시작하세요. 읽기 진행 상황이 자동으로 저장됩니다.",

  linkPlaceholder: "만화 또는 챕터 URL 붙여넣기",
  fetching: "불러오는 중",
  open: "열기",
  openVerification: "인증 창 열기",
  waitingVerification: "인증 대기 중...",
  toastOpeningVerify: "수동 인증을 위해 브라우저 창을 여는 중...",
  toastVerifyDone: "인증 완료. 만화를 다시 불러오는 중...",

  badgeNew: "새 글",
  chaptersBtn: "목록",
  backHome: "홈",
  refreshChapters: "목록 새로고침",
  refreshing: "새로고침 중...",
  toastChaptersUpdated: "챕터 목록을 업데이트했습니다.",
  chapterCount: (n) => `총 ${n}화`,
  continueChapter: (name) => `이어서 읽기 · ${name}`,
  startReading: "처음부터 읽기",
  readFromFirst: "1화부터 읽기",
  chaptersHeading: "챕터 목록",
  filterChapters: "챕터 검색...",
  newestFirst: "최신순",
  oldestFirst: "오래된순",

  back: "뒤로",
  backTooltip: "뒤로 (Esc)",
  readingFallback: "읽는 중",
  prevChapterTooltip: "이전 챕터 (←)",
  nextChapterTooltip: "다음 챕터 (→)",
  quickSettingsTooltip: "빠른 리더 설정",
  fullscreenTooltip: "전체 화면 전환 (F)",
  imageWidth: "이미지 너비",
  fitWindow: "창에 맞추기",
  autoAppendNext: "다음 챕터 자동 이어붙이기",
  readerBg: "리더 배경",
  bgDark: "다크",
  bgLight: "라이트",
  bgAppTheme: "앱 테마",
  loadingPages: "페이지 불러오는 중...",
  retry: "다시 시도",
  loadingNextChapter: "다음 챕터 불러오는 중...",
  nextChapter: "다음 챕터",
  reachedLatest: "최신 챕터까지 모두 읽었습니다",
  failedPage: (n) => `${n}페이지 로드 실패`,
  pageAlt: (n) => `${n}페이지`,

  historyTitle: "읽기 기록",
  historySubtitle: "여기에 있는 만화는 자동으로 새 챕터를 확인합니다. 삭제하면 확인이 중지됩니다.",
  clearAll: "전체 삭제",
  confirmClear: "삭제 확인",
  cancel: "취소",
  historyEmptyTitle: "기록이 비어 있습니다",
  historyEmptyDesc: "첫 챕터를 열면 여기에 자동으로 표시됩니다.",
  today: "오늘",
  yesterday: "어제",
  earlier: "이전",
  removeHistoryTooltip: "기록에서 삭제",
  toastRemovedHistory: "기록에서 삭제하고 업데이트 확인을 중지했습니다.",
  toastClearedHistory: "모든 읽기 기록을 삭제했습니다.",

  settingsTitle: "설정",
  settingsSubtitle: "모든 변경 사항은 자동으로 저장되며 즉시 적용됩니다.",
  appearanceSection: "화면",
  languageLabel: "언어",
  languageDesc: "메뉴, 설정 및 알림에 표시될 언어를 선택합니다.",
  colorThemeLabel: "색상 테마",
  colorThemeDesc: "라이트, 다크 또는 시스템 설정을 따릅니다.",
  themeSystem: "시스템",
  themeLight: "라이트",
  themeDark: "다크",
  readerSection: "리더",
  widthModeLabel: "이미지 너비 모드",
  widthModeDesc: "고정 픽셀, 화면 비율 또는 창 맞춤을 선택합니다.",
  widthPixels: "픽셀",
  widthPercent: "퍼센트",
  widthLabelPx: (px) => `너비 (${px} px)`,
  widthLabelPct: (pct) => `너비 (${pct}%)`,
  gapLabel: (px) => `페이지 간격 (${px} px)`,
  gapDesc: "0 px로 설정하면 세로로 빈틈없이 이어집니다.",
  preloadLabel: (n) => `미리 불러오기 (${n}장)`,
  preloadDesc: "아래 페이지를 미리 로드하여 스크롤을 부드럽게 합니다.",
  autoAppendDesc: "끄면 챕터 끝에 '다음 챕터' 버튼이 표시됩니다.",
  updatesSection: "챕터 업데이트",
  checkNow: "지금 확인",
  autoCheckLabel: "기록의 만화 자동 확인",
  autoCheckDesc: "설정한 주기로 백그라운드에서 확인합니다.",
  checkIntervalLabel: "확인 주기",
  everyHours: (h) => `${h}시간마다`,
  notificationsLabel: "시스템 알림",
  notificationsDesc: "새 챕터가 발견되면 알림을 표시합니다.",
  sourcesSection: "소스",
  headlessLabel: "헤드리스 브라우저 보조",
  headlessDesc: "Windows에서 JS 렌더링이 필요한 사이트에 자동으로 사용됩니다.",
  cacheSection: "이미지 캐시",
  cacheUsageLabel: "현재 사용량",
  cacheUsageDesc: (size, files) => `${size} (캐시된 이미지 ${files}개)`,
  calculating: "계산 중...",
  clearCache: "캐시 비우기",
  clearing: "비우는 중...",
  maxCacheLabel: (mb) => `최대 캐시 용량 (${mb} MB)`,
  toastCacheCleared: "이미지 캐시를 비웠습니다.",
  toastCacheClearFailed: "캐시를 비우지 못했습니다.",
  toastSettingsSaveFailed: "설정을 저장하지 못했습니다.",
  dismissNotification: "알림 닫기",

  toastFoundOne: (title, latest) => `새 챕터: ${title} (${latest})`,
  toastFoundMany: (n) => `${n}개의 만화에 새 챕터가 있습니다`,
  toastNoHistoryToCheck: "확인할 기록이 아직 없습니다.",
  toastNoNewChapters: (c) => `${c}개 확인 완료 — 새 챕터가 없습니다.`,
  toastCheckFailed: "지금은 업데이트를 확인할 수 없습니다.",

  appUpdatesSection: "앱 업데이트",
  appVersionLabel: (v) => `현재 버전: v${v}`,
  checkAppUpdatesLabel: "시작 시 새 릴리스 자동 확인",
  checkAppUpdatesDesc: "새로운 RtMaga 버전이 출시되면 자동으로 알려줍니다.",
  checkReleaseNow: "릴리스 확인",
  releaseAvailableBanner: (v) => `RtMaga v${v} 업데이트가 있습니다`,
  downloadUpdate: "다운로드",
  viewRelease: "자세히 보기",
  dismissUpdate: "닫기",
  releaseUpToDate: (v) => `최신 버전(v${v})을 사용 중입니다.`,
  releaseCheckFailed: "지금은 새 릴리스를 확인할 수 없습니다.",
  releaseOpenFailed: "릴리스 링크를 열 수 없습니다.",
  updatePromptQuestion: "지금 RtMaga가 이 업데이트를 자동으로 다운로드하고 설치할까요?",
  updateDataSafeNote: "읽기 기록, 진행 상황 및 설정이 자동으로 백업되어 그대로 유지됩니다.",
  updateDownloadAndInstall: "다운로드 및 설치",
  updateInstallNow: "지금 설치",
  updateLater: "나중에",
  updateStageDownloading: "업데이트 다운로드 중...",
  updateStageVerifying: "파일 무결성 확인 중...",
  updateStageBackingUp: "기록 및 설정 백업 중...",
  updateStageInstalling: "설치 프로그램 실행 중...",
  updateStatusRestarting: "설치 프로그램이 시작되었습니다. 완료 후 RtMaga가 자동으로 다시 시작됩니다.",
  updateAndroidPermissionHint:
    "열린 설정 화면에서 RtMaga의 '알 수 없는 앱 설치'를 허용한 뒤 돌아와 '지금 설치'를 누르세요 (파일은 이미 다운로드됨).",
  updateAndroidInstallerOpened:
    "시스템 설치 화면이 열렸습니다. 설치를 확인하면 데이터가 유지된 상태로 업그레이드됩니다.",
  toastHistoryUpdateFailed: "읽기 기록을 업데이트하지 못했습니다.",

  errInvalidUrlTitle: "잘못된 URL",
  errInvalidUrlDetail: "https:// 로 시작하는 유효한 만화 또는 챕터 링크를 붙여넣어 주세요.",
  errBlockedTitle: (host) => `${host}에서 인증이 필요합니다`,
  errBlockedDetail:
    "봇 차단이 활성화되어 있습니다. 브라우저 창에서 인증을 완료한 뒤 다시 시도하세요.",
  errNotRecognizedTitle: "만화 콘텐츠를 인식하지 못했습니다",
  errNotRecognizedDetail:
    "작품 또는 챕터 링크를 직접 붙여넣거나 설정에서 헤드리스 브라우저를 켜 보세요.",
  errNetworkTitle: "사이트에 연결할 수 없습니다",
  errNotFoundTitle: "찾을 수 없음",
  errAppTitle: "애플리케이션 오류",
  errGenericTitle: "오류가 발생했습니다",

  readingModeLabel: "읽기 모드",
  readingModeDesc: "세로 스크롤과 가로 페이지/그룹 스크롤을 전환합니다.",
  modeVertical: "세로 스크롤",
  modeHorizontal: "가로 스크롤",
  verticalStyleLabel: "세로 레이아웃",
  verticalStyleDesc: "연속 스크롤 또는 여러 페이지를 묶어 보는 그룹 표시를 선택합니다.",
  verticalContinuous: "연속 스크롤",
  verticalGrouped: "페이지 그룹",
  horizontalDirLabel: "읽기 방향",
  dirLtr: "왼쪽에서 오른쪽 (LTR)",
  dirRtl: "오른쪽에서 왼쪽 (RTL)",
  pagesPerGroupLabel: "그룹당 페이지 수",
  pagesPerGroupDesc: "1, 2, 3 또는 사용자 지정 개수(1-8)의 페이지를 함께 표시합니다.",
  pageGapLabel: (px) => `그룹 내 간격 (${px} px)`,
  groupGapLabel: (px) => `그룹 간 간격 (${px} px)`,
  snapModeLabel: "스냅 동작",
  snapNone: "자유",
  snapPage: "페이지",
  snapGroup: "그룹",
  groupSnapLabel: "그룹 단위 스냅",
  groupAlignLabel: "그룹 정렬",
  alignCenter: "가운데",
  alignStart: "상단 / 시작",
  centerGroupsLabel: "뷰포트 가운데 정렬",
  centerGroupsDesc: "현재 페이지 그룹을 화면 가로 중앙에 배치합니다.",
  dimUnfocusedLabel: "비초점 페이지 어둡게",
  dimUnfocusedDesc: "현재 읽고 있는 영역 바깥의 페이지를 은은하게 어둡게 합니다.",
  dimLevelLabel: (pct) => `어두움 강도 (${pct}%)`,

  proxySection: "앱 내 프록시 (지역 차단 우회)",
  proxySubtitle: "RtMaga 내부 요청에만 적용되며 시스템 VPN이나 다른 앱에는 영향을 주지 않습니다.",
  proxyDirect: "직접 연결",
  proxyEnabled: "프록시",
  proxyProtocolLabel: "프로토콜",
  proxyHostLabel: "프록시 호스트",
  proxyPortLabel: "포트",
  proxyUsernameLabel: "사용자 이름 (선택)",
  proxyPasswordLabel: "비밀번호 (선택)",
  proxyFailClosedLabel: "프록시 오류 시 직접 연결로 폴백 허용",
  proxyFailClosedDesc: "실제 IP 노출을 방지하기 위해 기본값은 꺼짐(권장)입니다.",
  proxyTestUrlLabel: "테스트 URL",
  proxyTestBtn: "연결 테스트",
  proxyTesting: "테스트 중...",
  proxyTestOk: (ms, status) =>
    status ? `프록시 연결 성공 (${ms} ms, HTTP ${status})` : `프록시 연결 성공 (${ms} ms)`,
  proxyTestFailed: (msg) => `프록시 테스트 실패: ${msg}`,
  proxyDomainRulesTitle: "소스별 도메인 규칙",
  proxyDomainRulesDesc: "특정 만화 사이트 도메인별로 프록시 경로를 개별 지정합니다.",
  proxyDomainPlaceholder: "도메인 (예: example.com)",
  proxyRuleForceProxy: "항상 프록시",
  proxyRuleForceDirect: "항상 직접 연결",
  proxyRuleCustomProxy: "사용자 프록시",
  proxyAddRule: "규칙 추가",
  proxyRemoveRule: "삭제",
  proxyNoRules: "설정된 도메인 규칙이 없습니다.",

  syncSection: "LAN 동기화 (서버 없음)",
  syncSubtitle: "같은 Wi-Fi/LAN 내 기기끼리 직접 동기화합니다. 백엔드나 계정이 필요 없습니다.",
  syncDeviceLabel: (name) => `이 기기: ${name}`,
  syncPcSendTab: "휴대폰으로 보내기 (QR)",
  syncPcReceiveTab: "휴대폰에서 받기 (코드)",
  syncPhoneReceiveTab: "PC에서 받기 (QR 스캔)",
  syncPhoneSendTab: "PC로 보내기 (코드)",
  syncStartQrBtn: "QR 코드 생성",
  syncStartPinBtn: "6자리 페어링 코드 생성",
  syncCancelBtn: "세션 종료",
  syncExpiresIn: (mmss) => `${mmss} 후 만료`,
  syncWaitingPeer: "같은 LAN의 상대 기기를 기다리는 중...",
  syncCopyCode: "대체 코드 복사",
  syncCopied: "복사됨!",
  syncFallbackAddr: (addr) => `LAN 주소: ${addr}`,
  syncEnterPinLabel: "6자리 페어링 코드",
  syncEnterPinPlaceholder: "6자리 코드 입력",
  syncManualAddrToggle: "자동 검색이 안 되나요? IP:포트 직접 입력",
  syncManualAddrPlaceholder: "예: 192.168.1.42:54321",
  syncConnectBtn: "연결",
  syncConnecting: "연결 중...",
  syncStartCamera: "카메라로 QR 스캔",
  syncStopCamera: "카메라 중지",
  syncScanImageBtn: "이미지에서 QR 스캔",
  syncPasteCodePlaceholder: "또는 PC의 대체 코드를 붙여넣기...",
  syncCameraError:
    "카메라에 접근할 수 없습니다. '이미지에서 QR 스캔' 또는 대체 코드를 사용하세요.",
  syncQrNotFound: "선택한 이미지에서 유효한 RtMaga QR 코드를 찾을 수 없습니다.",
  syncConfirmTitle: "데이터 전송 확인",
  syncConfirmSendDesc: (peer, ip, count) =>
    `${count}개의 만화 기록을 "${peer}" (${ip})로 보낼까요?`,
  syncConfirmReceiveDesc: (peer, ip, count) =>
    `"${peer}" (${ip})에서 ${count}개의 만화 기록을 받아 병합할까요? 더 최신 챕터가 자동 유지됩니다.`,
  syncApproveBtn: "확인 및 동기화",
  syncDeclineBtn: "거절",
  syncWaitingOtherConfirm: "상대 기기의 확인을 기다리는 중...",
  syncTransferring: "읽기 기록을 전송하고 병합하는 중...",
  syncCompletedTitle: "동기화 완료",
  syncReportSummary: (added, updated, skipped, kept) =>
    `신규 ${added} • 최신화 ${updated} • 동일 건너뜀 ${skipped} • 로컬 유지 ${kept}`,
  syncResetBtn: "완료",
  syncSecurityNote:
    "일회용 세션 5분 만료 • 최대 5회 실패 제한 • 동일 LAN 전용 • 데이터 동기화 전용(원격 제어 불가).",
};

const th: Dictionary = {
  brandSubtitle: "ตัวอ่านมังงะเรียบง่าย",
  navHome: "หน้าแรก",
  navHistory: "ประวัติ",
  navSettings: "ตั้งค่า",
  checkUpdates: "เช็กตอนใหม่",
  checking: "กำลังเช็ก...",
  checkingProgress: (d, t) => `กำลังเช็ก (${d}/${t})`,
  checkUpdatesTooltip: "ตรวจสอบตอนใหม่ของมังงะในประวัติการอ่าน",
  lightMode: "โหมดสว่าง",
  darkMode: "โหมดมืด",

  homeTitle: "วางลิงก์มังงะเพื่อเริ่มอ่านทันที",
  newChapters: "มีตอนใหม่",
  continueReading: "อ่านต่อ",
  viewAllHistory: "ดูประวัติทั้งหมด",
  noRecentTitle: "ยังไม่มีประวัติการอ่าน",
  noRecentDesc: "วางลิงก์มังงะด้านบนเพื่อเริ่มอ่าน ความคืบหน้าจะถูกบันทึกไว้ที่นี่อัตโนมัติ",

  linkPlaceholder: "วางลิงก์มังงะหรือลิงก์ตอน",
  fetching: "กำลังดึงข้อมูล",
  open: "เปิดอ่าน",
  openVerification: "เปิดหน้าต่างยืนยันตัวตน",
  waitingVerification: "กำลังรอยืนยัน...",
  toastOpeningVerify: "กำลังเปิดหน้าต่างเบราว์เซอร์เพื่อให้คุณยืนยันตัวตน...",
  toastVerifyDone: "ยืนยันเสร็จแล้ว กำลังลองโหลดใหม่อีกครั้ง...",

  badgeNew: "ใหม่",
  chaptersBtn: "ตอน",
  backHome: "หน้าแรก",
  refreshChapters: "รีเฟรชรายการตอน",
  refreshing: "กำลังรีเฟรช...",
  toastChaptersUpdated: "อัปเดตรายการตอนล่าสุดแล้ว",
  chapterCount: (n) => `${n} ตอน`,
  continueChapter: (name) => `อ่านต่อ · ${name}`,
  startReading: "เริ่มอ่าน",
  readFromFirst: "อ่านตั้งแต่ตอนแรก",
  chaptersHeading: "รายการตอน",
  filterChapters: "ค้นหาเลขตอน...",
  newestFirst: "ใหม่สุดก่อน",
  oldestFirst: "เก่าสุดก่อน",

  back: "กลับ",
  backTooltip: "กลับ (Esc)",
  readingFallback: "กำลังอ่าน",
  prevChapterTooltip: "ตอนก่อนหน้า (←)",
  nextChapterTooltip: "ตอนถัดไป (→)",
  quickSettingsTooltip: "ตั้งค่าด่วน",
  fullscreenTooltip: "เต็มหน้าจอ (F)",
  imageWidth: "ความกว้างภาพ",
  fitWindow: "พอดีหน้าต่าง",
  autoAppendNext: "ต่อตอนถัดไปอัตโนมัติ",
  readerBg: "พื้นหลังตัวอ่าน",
  bgDark: "มืด",
  bgLight: "สว่าง",
  bgAppTheme: "ตามแอป",
  loadingPages: "กำลังโหลดภาพในตอน...",
  retry: "ลองใหม่",
  loadingNextChapter: "กำลังโหลดตอนถัดไป...",
  nextChapter: "ตอนถัดไป",
  reachedLatest: "คุณอ่านถึงตอนล่าสุดแล้ว",
  failedPage: (n) => `โหลดหน้าที่ ${n} ไม่สำเร็จ`,
  pageAlt: (n) => `หน้า ${n}`,

  historyTitle: "ประวัติการอ่าน",
  historySubtitle:
    "แอปจะเช็กตอนใหม่ให้อัตโนมัติสำหรับเรื่องในหน้านี้ การลบออกจะหยุดการเช็กตอนใหม่",
  clearAll: "ล้างทั้งหมด",
  confirmClear: "ยืนยันล้างทั้งหมด",
  cancel: "ยกเลิก",
  historyEmptyTitle: "ประวัติว่างเปล่า",
  historyEmptyDesc: "มังงะจะปรากฏที่นี่ทันทีเมื่อคุณเปิดอ่านตอนแรก",
  today: "วันนี้",
  yesterday: "เมื่อวาน",
  earlier: "ก่อนหน้านี้",
  removeHistoryTooltip: "ลบออกจากประวัติ",
  toastRemovedHistory: "ลบออกจากประวัติและหยุดเช็กตอนใหม่แล้ว",
  toastClearedHistory: "ล้างประวัติการอ่านทั้งหมดแล้ว",

  settingsTitle: "ตั้งค่า",
  settingsSubtitle: "การเปลี่ยนแปลงทั้งหมดจะถูกบันทึกและมีผลทันที",
  appearanceSection: "หน้าตาแอป",
  languageLabel: "ภาษา",
  languageDesc: "เลือกภาษาที่แสดงสำหรับเมนู การตั้งค่า และการแจ้งเตือน",
  colorThemeLabel: "ธีมสี",
  colorThemeDesc: "เลือกโหมดสว่าง โหมดมืด หรือตามระบบปฏิบัติการ",
  themeSystem: "ตามระบบ",
  themeLight: "สว่าง",
  themeDark: "มืด",
  readerSection: "ตัวอ่านมังงะ",
  widthModeLabel: "โหมดความกว้างภาพ",
  widthModeDesc: "กำหนดความกว้างเป็นพิกเซล เปอร์เซ็นต์ หรือพอดีหน้าต่าง",
  widthPixels: "พิกเซล",
  widthPercent: "เปอร์เซ็นต์",
  widthLabelPx: (px) => `ความกว้าง (${px} px)`,
  widthLabelPct: (pct) => `ความกว้าง (${pct}%)`,
  gapLabel: (px) => `ระยะห่างระหว่างภาพ (${px} px)`,
  gapDesc: "ตั้งเป็น 0 px เพื่อให้อ่านเลื่อนแนวตั้งแบบไร้รอยต่อ",
  preloadLabel: (n) => `โหลดภาพล่วงหน้า (${n} ภาพ)`,
  preloadDesc: "โหลดภาพด้านล่างไว้ล่วงหน้าเพื่อให้เลื่อนลื่นขึ้น",
  autoAppendDesc: "ปิดหากต้องการแสดงปุ่มกดไปตอนถัดไปแทน",
  updatesSection: "ตรวจหาตอนใหม่",
  checkNow: "เช็กตอนนี้",
  autoCheckLabel: "เช็กมังงะในประวัติอัตโนมัติ",
  autoCheckDesc: "ทำงานเบื้องหลังตามรอบเวลาที่กำหนด",
  checkIntervalLabel: "รอบเวลาการเช็ก",
  everyHours: (h) => `ทุก ${h} ชั่วโมง`,
  notificationsLabel: "การแจ้งเตือนระบบ",
  notificationsDesc: "แสดงการแจ้งเตือนเมื่อพบตอนใหม่",
  sourcesSection: "แหล่งข้อมูล",
  headlessLabel: "เบราว์เซอร์เบื้องหลัง",
  headlessDesc: "ใช้งานอัตโนมัติบน Windows เมื่อเว็บโหลดภาพด้วย JavaScript",
  cacheSection: "แคชรูปภาพ",
  cacheUsageLabel: "พื้นที่ใช้งานปัจจุบัน",
  cacheUsageDesc: (size, files) => `${size} (${files} ภาพที่แคชไว้)`,
  calculating: "กำลังคำนวณ...",
  clearCache: "ล้างแคช",
  clearing: "กำลังล้าง...",
  maxCacheLabel: (mb) => `ขนาดแคชสูงสุด (${mb} MB)`,
  toastCacheCleared: "ล้างแคชรูปภาพแล้ว",
  toastCacheClearFailed: "ไม่สามารถล้างแคชได้",
  toastSettingsSaveFailed: "บันทึกการตั้งค่าไม่สำเร็จ",
  dismissNotification: "ปิดการแจ้งเตือน",

  toastFoundOne: (title, latest) => `ตอนใหม่: ${title} (${latest})`,
  toastFoundMany: (n) => `มีตอนใหม่สำหรับมังงะ ${n} เรื่อง`,
  toastNoHistoryToCheck: "ยังไม่มีมังงะในประวัติสำหรับตรวจสอบ",
  toastNoNewChapters: (c) => `ตรวจสอบแล้ว ${c} เรื่อง — ยังไม่มีตอนใหม่`,
  toastCheckFailed: "ไม่สามารถเช็กตอนใหม่ได้ในขณะนี้",

  appUpdatesSection: "อัปเดตแอปพลิเคชัน",
  appVersionLabel: (v) => `เวอร์ชันปัจจุบัน: v${v}`,
  checkAppUpdatesLabel: "ตรวจสอบเวอร์ชันใหม่อัตโนมัติเมื่อเปิดแอป",
  checkAppUpdatesDesc: "แจ้งเตือนอัตโนมัติเมื่อมี RtMaga เวอร์ชันใหม่",
  checkReleaseNow: "ตรวจหาเวอร์ชันใหม่",
  releaseAvailableBanner: (v) => `มี RtMaga เวอร์ชันใหม่ v${v}`,
  downloadUpdate: "ดาวน์โหลด",
  viewRelease: "ดูรายละเอียด",
  dismissUpdate: "ปิด",
  releaseUpToDate: (v) => `คุณใช้เวอร์ชันล่าสุดอยู่แล้ว (v${v})`,
  releaseCheckFailed: "ไม่สามารถตรวจสอบเวอร์ชันใหม่ได้ในขณะนี้",
  releaseOpenFailed: "ไม่สามารถเปิดลิงก์ดาวน์โหลดได้",
  updatePromptQuestion: "คุณต้องการให้ RtMaga ดาวน์โหลดและติดตั้งอัปเดตนี้โดยอัตโนมัติเลยหรือไม่?",
  updateDataSafeNote:
    "ประวัติการอ่าน ความคืบหน้า รายการมังงะ และการตั้งค่าของคุณจะถูกสำรองและเก็บรักษาไว้ทั้งหมด",
  updateDownloadAndInstall: "ดาวน์โหลดและติดตั้ง",
  updateInstallNow: "ติดตั้งเลย",
  updateLater: "ภายหลัง",
  updateStageDownloading: "กำลังดาวน์โหลดอัปเดต...",
  updateStageVerifying: "กำลังตรวจสอบความถูกต้องของไฟล์...",
  updateStageBackingUp: "กำลังสำรองประวัติและการตั้งค่า...",
  updateStageInstalling: "กำลังเปิดโปรแกรมติดตั้ง...",
  updateStatusRestarting: "เริ่มการติดตั้งแล้ว แอปจะปิดและเปิดใหม่โดยอัตโนมัติหลังอัปเกรด",
  updateAndroidPermissionHint:
    "กรุณาเปิดสิทธิ์ 'ติดตั้งแอปที่ไม่รู้จัก' ให้ RtMaga ในหน้าตั้งค่าที่เปิดขึ้น จากนั้นกลับมาแตะ 'ติดตั้งเลย' (ดาวน์โหลดไฟล์ไว้แล้ว)",
  updateAndroidInstallerOpened:
    "เปิดหน้าติดตั้งของระบบแล้ว ยืนยันการติดตั้งเพื่ออัปเกรดโดยไม่สูญเสียข้อมูล",
  toastHistoryUpdateFailed: "ไม่สามารถอัปเดตประวัติการอ่านได้",

  errInvalidUrlTitle: "ลิงก์ไม่ถูกต้อง",
  errInvalidUrlDetail: "กรุณาวางลิงก์มังงะหรือตอนที่ถูกต้อง (ขึ้นต้นด้วย https://...)",
  errBlockedTitle: (host) => `${host} ต้องการการยืนยันตัวตน`,
  errBlockedDetail:
    "เว็บไซต์เปิดการป้องกันบอท กรุณาเปิดหน้าต่างเบราว์เซอร์เพื่อยืนยันแล้วลองใหม่",
  errNotRecognizedTitle: "ไม่พบข้อมูลมังงะจากลิงก์นี้",
  errNotRecognizedDetail:
    "ลองวางลิงก์หน้าเรื่องหรือตอนโดยตรง หรือเปิดโหมดเบราว์เซอร์เบื้องหลังในการตั้งค่า",
  errNetworkTitle: "ไม่สามารถเชื่อมต่อเว็บไซต์ได้",
  errNotFoundTitle: "ไม่พบข้อมูล",
  errAppTitle: "ข้อผิดพลาดของแอป",
  errGenericTitle: "เกิดข้อผิดพลาด",

  readingModeLabel: "โหมดการอ่าน",
  readingModeDesc: "สลับระหว่างการเลื่อนแนวตั้งและการเลื่อนแนวนอนแบบหน้าเดียวหรือกลุ่มหน้า",
  modeVertical: "แนวตั้ง",
  modeHorizontal: "แนวนอน",
  verticalStyleLabel: "รูปแบบแนวตั้ง",
  verticalStyleDesc: "เลือกเลื่อนต่อเนื่องแบบไร้รอยต่อหรือจัดกลุ่มหน้าต่อมุมมอง",
  verticalContinuous: "เลื่อนต่อเนื่อง",
  verticalGrouped: "จัดกลุ่มหน้า",
  horizontalDirLabel: "ทิศทางการอ่าน",
  dirLtr: "ซ้ายไปขวา (LTR)",
  dirRtl: "ขวาไปซ้าย (RTL)",
  pagesPerGroupLabel: "จำนวนหน้าต่อกลุ่ม",
  pagesPerGroupDesc: "แสดง 1, 2, 3 หรือกำหนดจำนวนหน้าเองพร้อมกัน (1-8)",
  pageGapLabel: (px) => `ระยะห่างภายในกลุ่ม (${px} px)`,
  groupGapLabel: (px) => `ระยะห่างระหว่างกลุ่ม (${px} px)`,
  snapModeLabel: "การล็อกตำแหน่ง (Snap)",
  snapNone: "อิสระ",
  snapPage: "ทีละหน้า",
  snapGroup: "ทีละกลุ่ม",
  groupSnapLabel: "ล็อกตำแหน่งตามกลุ่ม",
  groupAlignLabel: "การจัดวางกลุ่ม",
  alignCenter: "กึ่งกลาง",
  alignStart: "ด้านบน / จุดเริ่มต้น",
  centerGroupsLabel: "จัดกลุ่มให้อยู่กึ่งกลางจอ",
  centerGroupsDesc: "จัดกลุ่มหน้าที่กำลังอ่านให้อยู่กึ่งกลางแนวนอน",
  dimUnfocusedLabel: "ลดแสงหน้านอกโฟกัส",
  dimUnfocusedDesc: "ทำให้หน้าหรือกลุ่มที่อยู่นอกระยะโฟกัสมืดลงเล็กน้อย",
  dimLevelLabel: (pct) => `ระดับความมืด (${pct}%)`,

  proxySection: "พร็อกซีภายในแอป (ข้ามการบล็อก IP)",
  proxySubtitle: "ทำงานเฉพาะใน RtMaga เท่านั้น — ไม่เปลี่ยน VPN ระบบและไม่กระทบแอปอื่น",
  proxyDirect: "เชื่อมต่อตรง",
  proxyEnabled: "ใช้พร็อกซี",
  proxyProtocolLabel: "โปรโตคอล",
  proxyHostLabel: "ที่อยู่พร็อกซี",
  proxyPortLabel: "พอร์ต",
  proxyUsernameLabel: "ชื่อผู้ใช้ (ไม่บังคับ)",
  proxyPasswordLabel: "รหัสผ่าน (ไม่บังคับ)",
  proxyFailClosedLabel: "อนุญาตให้สลับไปเชื่อมต่อตรงเมื่อพร็อกซีล้มเหลว",
  proxyFailClosedDesc: "ปิดไว้เป็นค่าเริ่มต้น (แนะนำ) เพื่อป้องกัน IP จริงรั่วไหลโดยไม่ตั้งใจ",
  proxyTestUrlLabel: "URL ทดสอบ",
  proxyTestBtn: "ทดสอบการเชื่อมต่อ",
  proxyTesting: "กำลังทดสอบ...",
  proxyTestOk: (ms, status) =>
    status ? `เชื่อมต่อผ่านพร็อกซีสำเร็จ (${ms} ms, HTTP ${status})` : `เชื่อมต่อสำเร็จ (${ms} ms)`,
  proxyTestFailed: (msg) => `ทดสอบพร็อกซีไม่สำเร็จ: ${msg}`,
  proxyDomainRulesTitle: "กฎเส้นทางแยกตามโดเมน",
  proxyDomainRulesDesc: "กำหนดการใช้พร็อกซีเฉพาะโดเมนของเว็บมังงะแต่ละแห่ง",
  proxyDomainPlaceholder: "โดเมน (เช่น example.com)",
  proxyRuleForceProxy: "บังคับใช้พร็อกซี",
  proxyRuleForceDirect: "เชื่อมต่อตรงเสมอ",
  proxyRuleCustomProxy: "พร็อกซีเฉพาะ",
  proxyAddRule: "เพิ่มกฎ",
  proxyRemoveRule: "ลบ",
  proxyNoRules: "ยังไม่ได้ตั้งค่ากฎแยกตามโดเมน",

  syncSection: "ซิงค์ผ่าน LAN (ไม่ใช้เซิร์ฟเวอร์)",
  syncSubtitle: "ซิงค์ประวัติการอ่านโดยตรงระหว่างอุปกรณ์ใน Wi-Fi/LAN เดียวกัน ไม่มีแบ็กเอนด์หรือบัญชี",
  syncDeviceLabel: (name) => `อุปกรณ์นี้: ${name}`,
  syncPcSendTab: "ส่งไปยังมือถือ (QR)",
  syncPcReceiveTab: "รับจากมือถือ (รหัสจับคู่)",
  syncPhoneReceiveTab: "รับจากคอมพิวเตอร์ (สแกน QR)",
  syncPhoneSendTab: "ส่งไปยังคอมพิวเตอร์ (รหัสจับคู่)",
  syncStartQrBtn: "สร้างคิวอาร์โค้ด",
  syncStartPinBtn: "สร้างรหัสจับคู่ 6 หลัก",
  syncCancelBtn: "สิ้นสุดเซสชัน",
  syncExpiresIn: (mmss) => `หมดอายุใน ${mmss}`,
  syncWaitingPeer: "กำลังรออุปกรณ์อีกเครื่องในเครือข่าย LAN เดียวกัน...",
  syncCopyCode: "คัดลอกรหัสสำรอง",
  syncCopied: "คัดลอกแล้ว!",
  syncFallbackAddr: (addr) => `ที่อยู่ LAN: ${addr}`,
  syncEnterPinLabel: "รหัสจับคู่ 6 หลัก",
  syncEnterPinPlaceholder: "กรอกรหัส 6 หลัก",
  syncManualAddrToggle: "ค้นหาอัตโนมัติไม่พบ? กรอก IP:Port ด้วยตนเอง",
  syncManualAddrPlaceholder: "เช่น 192.168.1.42:54321",
  syncConnectBtn: "เชื่อมต่อ",
  syncConnecting: "กำลังเชื่อมต่อ...",
  syncStartCamera: "สแกน QR ด้วยกล้อง",
  syncStopCamera: "ปิดกล้อง",
  syncScanImageBtn: "สแกน QR จากรูปภาพ",
  syncPasteCodePlaceholder: "หรือวางรหัสสำรองจากคอมพิวเตอร์...",
  syncCameraError: "ไม่สามารถเข้าถึงกล้องได้ ใช้ 'สแกน QR จากรูปภาพ' หรือวางรหัสสำรองแทน",
  syncQrNotFound: "ไม่พบคิวอาร์โค้ด RtMaga ที่ถูกต้องในรูปภาพที่เลือก",
  syncConfirmTitle: "ยืนยันการโอนถ่ายข้อมูล",
  syncConfirmSendDesc: (peer, ip, count) =>
    `ส่งประวัติมังงะ ${count} เรื่องไปยัง "${peer}" (${ip}) หรือไม่?`,
  syncConfirmReceiveDesc: (peer, ip, count) =>
    `รับและรวมมังงะ ${count} เรื่องจาก "${peer}" (${ip}) หรือไม่? เรื่องที่ซ้ำจะเก็บตอนที่อ่านไกลกว่า`,
  syncApproveBtn: "ยืนยันและซิงค์",
  syncDeclineBtn: "ปฏิเสธ",
  syncWaitingOtherConfirm: "กำลังรอให้อีกอุปกรณ์กดยืนยัน...",
  syncTransferring: "กำลังโอนถ่ายและรวมประวัติการอ่าน...",
  syncCompletedTitle: "ซิงค์เสร็จสมบูรณ์",
  syncReportSummary: (added, updated, skipped, kept) =>
    `เพิ่มใหม่ ${added} • อัปเดตตอนใหม่ ${updated} • ข้ามตอนเดิม ${skipped} • เก็บของเครื่องนี้ ${kept}`,
  syncResetBtn: "เสร็จสิ้น",
  syncSecurityNote:
    "รหัสใช้ครั้งเดียวหมดอายุใน 5 นาที • ผิดได้สูงสุด 5 ครั้ง • เฉพาะ LAN เดียวกัน • ซิงค์ข้อมูลเท่านั้น ไม่ควบคุมอุปกรณ์",
};

interface V3Dictionary {
  navBookmarks: string;
  historyResumeHint: string;
  historyNoResumeTarget: string;
  pressBackAgainToExit: string;

  bookmarksTitle: string;
  bookmarksSubtitle: string;
  bookmarksSortLabel: string;
  bookmarksSortNewest: string;
  bookmarksSortHasNew: string;
  bookmarksSortRecent: string;
  bookmarksSortTitle: string;
  bookmarksEmptyTitle: string;
  bookmarksEmptyDesc: string;
  bookmarkAdd: string;
  bookmarkRemove: string;
  bookmarkedLabel: string;
  toastBookmarkAdded: string;
  toastBookmarkRemoved: string;

  notifCenterTitle: string;
  notifMarkAllRead: string;
  notifEmptyTitle: string;
  notifEmptyDesc: string;
  notifChaptersLabel: (range: string, count: number) => string;
  notifGrantPermissionBtn: string;

  scrollToTopLabel: string;
  orientationAuto: string;
  orientationPortrait: string;
  orientationLandscape: string;

  autoWebtoonLabel: string;
  autoWebtoonDesc: string;
  autoWebtoonBadge: string;
  toastWebtoonDetected: string;
  readerOverrideActive: string;
  readerResetToAuto: string;
  readerOverrideResetToast: string;
  tapZonesLabel: string;
  tapZonesDesc: string;

  deviceIdentitySection: string;
  deviceDetectedLabel: string;
  deviceCustomNameLabel: string;
  deviceCustomNameDesc: string;
  deviceResetName: string;

  floatingSection: string;
  floatingSubtitle: string;
  floatingScrollTopScreens: string;
  floatingScrollTopPos: string;
  floatingScreenManga: string;
  floatingScreenReader: string;
  floatingReaderSettingsLabel: string;
  floatingReaderSettingsDesc: string;
  floatingOrientationLockLabel: string;
  floatingOrientationLockDesc: string;
  floatingPositionLabel: string;
  floatingEdgeLeft: string;
  floatingEdgeRight: string;
  floatingAutoDimLabel: string;
  floatingAutoDimDesc: string;

  netProtectionSection: string;
  netProtectionSubtitle: string;
  netSystemDns: string;
  netEncryptedDnsOn: string;
  netHonestDisclaimer: string;
  netSelectProvider: string;
  netTypeEncryptedDns: string;
  netTypeDnsFiltering: string;
  netTypeLocalTunnel: string;
  netProfileLabel: string;
  netAccountIdLabel: string;
  netSystemFallbackLabel: string;
  netSystemFallbackDesc: string;
  netTestDnsBtn: string;
  netTestDnsOk: (ms: number, ips: number) => string;
  netTestDnsFailed: string;
  netLocalTunnelPresetBtn: string;
  netLocalTunnelPresetDesc: string;
  netLocalTunnelPresetApplied: string;
  netAdvancedProxyToggle: string;
}

const V3_EN: V3Dictionary = {
  navBookmarks: "Bookmarks",
  historyResumeHint: "Tap History again while on History to resume reading",
  historyNoResumeTarget: "No recent reading session to resume",
  pressBackAgainToExit: "Press Back again to exit",

  bookmarksTitle: "Bookmarks",
  bookmarksSubtitle: "Your saved manga — prioritized first when checking for new chapters and included in LAN Sync.",
  bookmarksSortLabel: "Sort:",
  bookmarksSortNewest: "Recently added",
  bookmarksSortHasNew: "New chapters first",
  bookmarksSortRecent: "Recently read",
  bookmarksSortTitle: "Title (A-Z)",
  bookmarksEmptyTitle: "No bookmarks yet",
  bookmarksEmptyDesc: "Tap the Bookmark button on any manga page or inside the reader to save it here.",
  bookmarkAdd: "Bookmark",
  bookmarkRemove: "Remove bookmark",
  bookmarkedLabel: "Bookmarked",
  toastBookmarkAdded: "Added to Bookmarks",
  toastBookmarkRemoved: "Removed from Bookmarks",

  notifCenterTitle: "Notifications",
  notifMarkAllRead: "Mark all read",
  notifEmptyTitle: "All caught up",
  notifEmptyDesc: "New chapter alerts for your bookmarks and reading history will appear here.",
  notifChaptersLabel: (range, count) =>
    count > 1 ? `New chapters ${range} (+${count})` : `New: ${range}`,
  notifGrantPermissionBtn: "Allow notifications",

  scrollToTopLabel: "Scroll to top",
  orientationAuto: "Orientation: Auto",
  orientationPortrait: "Locked: Portrait",
  orientationLandscape: "Locked: Landscape",

  autoWebtoonLabel: "Auto Webtoon / Long-Strip detection",
  autoWebtoonDesc: "Passively detects tall webtoon strips from loaded pages and switches to seamless vertical scrolling.",
  autoWebtoonBadge: "Auto Webtoon mode",
  toastWebtoonDetected: "Webtoon format detected — switched to seamless vertical strip",
  readerOverrideActive: "Custom mode for this manga",
  readerResetToAuto: "Reset to Auto",
  readerOverrideResetToast: "Reset to automatic reading mode",
  tapZonesLabel: "Horizontal edge tap zones",
  tapZonesDesc: "Tap left/right edges to turn pages and tap center to toggle controls in horizontal mode.",

  deviceIdentitySection: "Device Identity",
  deviceDetectedLabel: "Detected device",
  deviceCustomNameLabel: "Display name (LAN Sync)",
  deviceCustomNameDesc: "Leave empty to use your real device name automatically.",
  deviceResetName: "Reset to detected name",

  floatingSection: "Floating Controls",
  floatingSubtitle: "Customize floating button visibility and screen placement (auto-avoids overlaps and cutouts).",
  floatingScrollTopScreens: "Show Scroll-to-Top button on",
  floatingScrollTopPos: "Scroll-to-Top placement",
  floatingScreenManga: "Manga",
  floatingScreenReader: "Reader",
  floatingReaderSettingsLabel: "Floating Reader Settings button",
  floatingReaderSettingsDesc: "Quick-access button inside the reader to adjust layout and mode.",
  floatingOrientationLockLabel: "Mobile Orientation Lock button",
  floatingOrientationLockDesc: "Quickly lock Portrait, Landscape, or Auto rotation while reading on mobile.",
  floatingPositionLabel: "Edge & vertical height",
  floatingEdgeLeft: "Left",
  floatingEdgeRight: "Right",
  floatingAutoDimLabel: "Auto-dim floating controls in Reader",
  floatingAutoDimDesc: "Gently fades floating buttons when idle and restores brightness on touch.",

  netProtectionSection: "Network Protection (In-App DNS)",
  netProtectionSubtitle: "Zero-config Encrypted DNS (DNS-over-HTTPS) for manga requests inside RtMaga — no system VPN required.",
  netSystemDns: "System DNS",
  netEncryptedDnsOn: "Encrypted DNS",
  netHonestDisclaimer:
    "Encrypted DNS / DNS Filtering encrypts domain lookups inside RtMaga to bypass ISP DNS blocking or filter ad/malware domains. It is NOT a VPN and does not hide your IP address; for full IP tunneling, use a local SOCKS5/HTTP proxy or external VPN.",
  netSelectProvider: "Choose a provider",
  netTypeEncryptedDns: "Encrypted DNS",
  netTypeDnsFiltering: "DNS Filtering",
  netTypeLocalTunnel: "Local Tunnel",
  netProfileLabel: "Filtering profile",
  netAccountIdLabel: "Profile / Account ID",
  netSystemFallbackLabel: "Fall back to System DNS on failure",
  netSystemFallbackDesc: "Automatically use OS DNS if the selected provider cannot be reached.",
  netTestDnsBtn: "Test DNS provider",
  netTestDnsOk: (ms, ips) => `Connected · ${ms} ms (${ips} IPs)`,
  netTestDnsFailed: "Connection failed",
  netLocalTunnelPresetBtn: "Use Local SOCKS5 Tunnel (127.0.0.1:40000)",
  netLocalTunnelPresetDesc: "Route RtMaga traffic through a local SOCKS5 proxy tunnel (e.g. local WARP proxy).",
  netLocalTunnelPresetApplied: "Applied local SOCKS5 tunnel preset (127.0.0.1:40000)",
  netAdvancedProxyToggle: "Advanced Proxy & Per-Domain Rules",
};

const V3_VI: V3Dictionary = {
  navBookmarks: "Đánh dấu",
  historyResumeHint: "Nhấn Lịch sử lần nữa khi đang ở trang Lịch sử để đọc tiếp ngay",
  historyNoResumeTarget: "Chưa có truyện gần đây để đọc tiếp",
  pressBackAgainToExit: "Nhấn Quay lại lần nữa để thoát",

  bookmarksTitle: "Truyện đã đánh dấu",
  bookmarksSubtitle: "Danh sách yêu thích — được ưu tiên kiểm tra chương mới trước và tự động đồng bộ qua LAN.",
  bookmarksSortLabel: "Sắp xếp:",
  bookmarksSortNewest: "Mới thêm",
  bookmarksSortHasNew: "Có chương mới",
  bookmarksSortRecent: "Đọc gần đây",
  bookmarksSortTitle: "Tên truyện (A-Z)",
  bookmarksEmptyTitle: "Chưa có truyện nào được đánh dấu",
  bookmarksEmptyDesc: "Nhấn nút Đánh dấu trên trang chi tiết truyện hoặc trong trình đọc để lưu lại đây.",
  bookmarkAdd: "Đánh dấu",
  bookmarkRemove: "Bỏ đánh dấu",
  bookmarkedLabel: "Đã đánh dấu",
  toastBookmarkAdded: "Đã thêm vào Đánh dấu",
  toastBookmarkRemoved: "Đã bỏ khỏi Đánh dấu",

  notifCenterTitle: "Thông báo",
  notifMarkAllRead: "Đọc tất cả",
  notifEmptyTitle: "Chưa có thông báo mới",
  notifEmptyDesc: "Thông báo chương mới từ truyện đánh dấu và lịch sử đọc sẽ hiển thị tại đây.",
  notifChaptersLabel: (range, count) =>
    count > 1 ? `Chương mới ${range} (+${count})` : `Mới: ${range}`,
  notifGrantPermissionBtn: "Cấp quyền thông báo",

  scrollToTopLabel: "Lên đầu trang",
  orientationAuto: "Xoay màn hình: Tự động",
  orientationPortrait: "Khóa dọc",
  orientationLandscape: "Khóa ngang",

  autoWebtoonLabel: "Tự động nhận diện Webtoon / Dải dài",
  autoWebtoonDesc: "Phân tích tỉ lệ ảnh đã tải và tự chuyển sang cuộn dọc liền mạch (không khoảng trống).",
  autoWebtoonBadge: "Chế độ Webtoon tự động",
  toastWebtoonDetected: "Phát hiện truyện Webtoon — tự chuyển sang cuộn dọc liền mạch",
  readerOverrideActive: "Chế độ riêng cho truyện này",
  readerResetToAuto: "Về tự động",
  readerOverrideResetToast: "Đã đặt lại chế độ đọc tự động",
  tapZonesLabel: "Chạm cạnh trái/phải để chuyển trang",
  tapZonesDesc: "Trong chế độ đọc ngang: chạm cạnh trái/phải để sang trang, chạm giữa để ẩn/hiện thanh công cụ.",

  deviceIdentitySection: "Tên thiết bị",
  deviceDetectedLabel: "Thiết bị được nhận diện",
  deviceCustomNameLabel: "Tên hiển thị (LAN Sync)",
  deviceCustomNameDesc: "Để trống để tự dùng tên thật của thiết bị.",
  deviceResetName: "Khôi phục tên mặc định",

  floatingSection: "Nút nổi tùy chỉnh",
  floatingSubtitle: "Tùy chỉnh hiển thị và vị trí các nút nổi (tự động tránh đè nhau và tránh tai thỏ/cử chỉ).",
  floatingScrollTopScreens: "Hiện nút Lên đầu trang tại",
  floatingScrollTopPos: "Vị trí nút Lên đầu trang",
  floatingScreenManga: "Chi tiết truyện",
  floatingScreenReader: "Trình đọc",
  floatingReaderSettingsLabel: "Nút Cài đặt đọc nổi",
  floatingReaderSettingsDesc: "Nút mở nhanh cài đặt chế độ đọc ngay trong trình đọc.",
  floatingOrientationLockLabel: "Nút Khóa xoay màn hình (Di động)",
  floatingOrientationLockDesc: "Chuyển nhanh giữa Tự động, Khóa dọc và Khóa ngang khi đọc trên điện thoại.",
  floatingPositionLabel: "Cạnh & độ cao dọc",
  floatingEdgeLeft: "Trái",
  floatingEdgeRight: "Phải",
  floatingAutoDimLabel: "Tự làm mờ nút nổi khi đang đọc",
  floatingAutoDimDesc: "Tự giảm độ sáng nút nổi khi không thao tác và sáng lại khi chạm.",

  netProtectionSection: "Bảo vệ mạng (DNS nội bộ)",
  netProtectionSubtitle: "Chọn nhanh nhà cung cấp DNS mã hóa (DNS-over-HTTPS) trong ứng dụng — không cần cấu hình phức tạp hay VPN hệ thống.",
  netSystemDns: "DNS hệ thống",
  netEncryptedDnsOn: "Bật DNS mã hóa",
  netHonestDisclaimer:
    "DNS mã hóa / Lọc DNS chỉ mã hóa tra cứu tên miền bên trong RtMaga để vượt chặn DNS nhà mạng hoặc lọc quảng cáo/mã độc. Đây KHÔNG phải VPN và không đổi địa chỉ IP; nếu cần định tuyến toàn bộ lưu lượng, hãy dùng Local SOCKS5/HTTP Proxy hoặc VPN ngoài.",
  netSelectProvider: "Chọn nhà cung cấp",
  netTypeEncryptedDns: "DNS mã hóa",
  netTypeDnsFiltering: "Lọc DNS",
  netTypeLocalTunnel: "Đường hầm cục bộ",
  netProfileLabel: "Hồ sơ lọc",
  netAccountIdLabel: "Mã hồ sơ / Tài khoản",
  netSystemFallbackLabel: "Tự về DNS hệ thống khi lỗi",
  netSystemFallbackDesc: "Tự chuyển về DNS mặc định của máy nếu không kết nối được máy chủ DNS đã chọn.",
  netTestDnsBtn: "Kiểm tra DNS",
  netTestDnsOk: (ms, ips) => `Đã kết nối · ${ms} ms (${ips} IP)`,
  netTestDnsFailed: "Kết nối thất bại",
  netLocalTunnelPresetBtn: "Dùng Local SOCKS5 (127.0.0.1:40000)",
  netLocalTunnelPresetDesc: "Định tuyến qua cổng proxy SOCKS5 cục bộ (ví dụ WARP local proxy).",
  netLocalTunnelPresetApplied: "Đã áp dụng cấu hình Local SOCKS5 (127.0.0.1:40000)",
  netAdvancedProxyToggle: "Cấu hình Proxy thủ công & Quy tắc theo tên miền",
};

const V3_ZH: V3Dictionary = {
  ...V3_EN,
  navBookmarks: "收藏",
  historyResumeHint: "在历史页面再次点击“历史”可立即继续上次阅读",
  historyNoResumeTarget: "暂无可继续的阅读记录",
  pressBackAgainToExit: "再按一次返回退出",
  bookmarksTitle: "我的收藏",
  bookmarksSubtitle: "优先检查新章节并自动包含在局域网同步中。",
  bookmarksSortLabel: "排序：",
  bookmarksSortNewest: "最近添加",
  bookmarksSortHasNew: "有新章节优先",
  bookmarksSortRecent: "最近阅读",
  bookmarksSortTitle: "按标题 (A-Z)",
  bookmarksEmptyTitle: "暂无收藏",
  bookmarksEmptyDesc: "在漫画详情页或阅读器中点击收藏按钮即可添加到这里。",
  bookmarkAdd: "收藏",
  bookmarkRemove: "取消收藏",
  bookmarkedLabel: "已收藏",
  toastBookmarkAdded: "已加入收藏",
  toastBookmarkRemoved: "已取消收藏",
  notifCenterTitle: "通知中心",
  notifMarkAllRead: "全部已读",
  notifEmptyTitle: "暂无新通知",
  notifEmptyDesc: "收藏和阅读历史的新章节更新提醒将显示在这里。",
  notifChaptersLabel: (range, count) => (count > 1 ? `新章节 ${range} (+${count})` : `新：${range}`),
  notifGrantPermissionBtn: "允许通知",
  scrollToTopLabel: "回到顶部",
  orientationAuto: "方向：自动",
  orientationPortrait: "锁定竖屏",
  orientationLandscape: "锁定横屏",
  autoWebtoonLabel: "自动识别条漫 (Webtoon)",
  autoWebtoonDesc: "根据已加载图片的长宽比自动切换为无缝长图垂直滚动。",
  autoWebtoonBadge: "自动条漫模式",
  toastWebtoonDetected: "检测到条漫格式 — 已切换为无缝竖向模式",
  readerOverrideActive: "此漫画已手动指定模式",
  readerResetToAuto: "恢复自动",
  readerOverrideResetToast: "已恢复自动阅读模式",
  tapZonesLabel: "横向边缘点击翻页",
  tapZonesDesc: "在横向模式中点击左右边缘翻页，点击中间切换工具栏。",
  deviceIdentitySection: "设备名称",
  deviceDetectedLabel: "检测到的设备",
  deviceCustomNameLabel: "自定义名称 (局域网同步)",
  deviceCustomNameDesc: "留空则自动使用真实设备名称。",
  deviceResetName: "恢复默认名称",
  floatingSection: "悬浮按钮设置",
  floatingSubtitle: "自定义悬浮按钮显示页面与位置（自动避让重叠与刘海区域）。",
  floatingScrollTopScreens: "在以下页面显示回到顶部按钮",
  floatingScrollTopPos: "回到顶部按钮位置",
  floatingScreenManga: "漫画详情",
  floatingScreenReader: "阅读器",
  floatingReaderSettingsLabel: "阅读器悬浮设置按钮",
  floatingReaderSettingsDesc: "在阅读器中快速打开阅读设置。",
  floatingOrientationLockLabel: "移动端方向锁定按钮",
  floatingOrientationLockDesc: "快速切换自动/竖屏/横屏锁定。",
  floatingPositionLabel: "边缘与垂直位置",
  floatingEdgeLeft: "左侧",
  floatingEdgeRight: "右侧",
  floatingAutoDimLabel: "阅读时闲置自动淡化",
  floatingAutoDimDesc: "无操作时自动降低悬浮按钮透明度，触摸时恢复。",
  netProtectionSection: "网络保护（应用内加密 DNS）",
  netProtectionSubtitle: "内置免配置 DNS-over-HTTPS 服务商，无需系统 VPN。",
  netSystemDns: "系统 DNS",
  netEncryptedDnsOn: "启用加密 DNS",
  netSelectProvider: "选择服务商",
  netTypeEncryptedDns: "加密 DNS",
  netTypeDnsFiltering: "DNS 过滤",
  netTypeLocalTunnel: "本地隧道",
  netProfileLabel: "过滤配置",
  netTestDnsBtn: "测试 DNS 连接",
  netTestDnsOk: (ms, ips) => `已连接 · ${ms} ms (${ips} 个 IP)`,
  netTestDnsFailed: "连接失败",
  netAdvancedProxyToggle: "高级代理与按域名规则",
};

const V3_JA: V3Dictionary = {
  ...V3_EN,
  navBookmarks: "ブックマーク",
  historyResumeHint: "履歴画面でもう一度「履歴」を押すと前回の続きから開きます",
  historyNoResumeTarget: "再開できる閲覧履歴がありません",
  pressBackAgainToExit: "もう一度戻ると終了します",
  bookmarksTitle: "ブックマーク",
  bookmarksSubtitle: "新着チェックで優先され、LAN同期にも含まれます。",
  bookmarksSortLabel: "並び替え：",
  bookmarksSortNewest: "追加順",
  bookmarksSortHasNew: "新着優先",
  bookmarksSortRecent: "最近読んだ順",
  bookmarksSortTitle: "タイトル順",
  bookmarksEmptyTitle: "ブックマークはありません",
  bookmarksEmptyDesc: "作品ページやリーダーのブックマークボタンから追加できます。",
  bookmarkAdd: "ブックマーク",
  bookmarkRemove: "ブックマーク解除",
  bookmarkedLabel: "登録済み",
  toastBookmarkAdded: "ブックマークに追加しました",
  toastBookmarkRemoved: "ブックマークを解除しました",
  notifCenterTitle: "通知センター",
  notifMarkAllRead: "すべて既読",
  notifEmptyTitle: "新しい通知はありません",
  notifEmptyDesc: "新着チャプターの通知がここに表示されます。",
  notifChaptersLabel: (range, count) => (count > 1 ? `新着 ${range} (+${count})` : `新着: ${range}`),
  notifGrantPermissionBtn: "通知を許可",
  scrollToTopLabel: "トップへ戻る",
  orientationAuto: "回転: 自動",
  orientationPortrait: "縦向き固定",
  orientationLandscape: "横向き固定",
  autoWebtoonLabel: "Webtoon / 縦読み自動判定",
  autoWebtoonDesc: "縦長画像を自動検出し、シームレスな縦スクロールに切り替えます。",
  autoWebtoonBadge: "自動Webtoonモード",
  toastWebtoonDetected: "Webtoon形式を検出 — 縦読みモードに切替しました",
  readerOverrideActive: "この作品のカスタムモード",
  readerResetToAuto: "自動に戻す",
  readerOverrideResetToast: "自動モードにリセットしました",
  deviceIdentitySection: "デバイス名",
  deviceDetectedLabel: "検出されたデバイス",
  deviceCustomNameLabel: "表示名 (LAN同期)",
  deviceCustomNameDesc: "空欄の場合は実際のデバイス名を使用します。",
  floatingSection: "フローティングボタン",
  floatingSubtitle: "画面ごとの表示切替と配置位置を調整できます。",
  floatingScrollTopScreens: "トップへ戻るボタンの表示画面",
  floatingEdgeLeft: "左",
  floatingEdgeRight: "右",
  netProtectionSection: "ネットワーク保護 (アプリ内DNS)",
  netSystemDns: "システムDNS",
  netEncryptedDnsOn: "暗号化DNS",
  netTestDnsBtn: "DNS接続テスト",
  netAdvancedProxyToggle: "詳細プロキシ設定とドメインルール",
};

const V3_RU: V3Dictionary = {
  ...V3_EN,
  navBookmarks: "Закладки",
  historyResumeHint: "Нажмите «История» ещё раз, чтобы продолжить чтение",
  historyNoResumeTarget: "Нет недавней главы для продолжения",
  pressBackAgainToExit: "Нажмите «Назад» ещё раз для выхода",
  bookmarksTitle: "Закладки",
  bookmarksSubtitle: "Проверяются первыми на новые главы и синхронизируются по LAN.",
  bookmarksSortLabel: "Сортировка:",
  bookmarksSortNewest: "Недавние",
  bookmarksSortHasNew: "С новыми главами",
  bookmarksSortRecent: "Недавно прочитанные",
  bookmarksSortTitle: "По названию",
  bookmarksEmptyTitle: "Закладок пока нет",
  bookmarksEmptyDesc: "Нажмите кнопку закладки на странице манги или в читалке.",
  bookmarkAdd: "В закладки",
  bookmarkRemove: "Убрать закладку",
  bookmarkedLabel: "В закладках",
  toastBookmarkAdded: "Добавлено в закладки",
  toastBookmarkRemoved: "Удалено из закладок",
  notifCenterTitle: "Уведомления",
  notifMarkAllRead: "Прочитать все",
  notifEmptyTitle: "Нет новых уведомлений",
  notifEmptyDesc: "Здесь появятся уведомления о новых главах.",
  notifGrantPermissionBtn: "Разрешить уведомления",
  scrollToTopLabel: "Наверх",
  orientationAuto: "Ориентация: Авто",
  orientationPortrait: "Портретная",
  orientationLandscape: "Альбомная",
  deviceIdentitySection: "Имя устройства",
  floatingSection: "Плавающие кнопки",
  floatingEdgeLeft: "Слева",
  floatingEdgeRight: "Справа",
  netProtectionSection: "Защита сети (Encrypted DNS)",
  netSystemDns: "Системный DNS",
  netEncryptedDnsOn: "Шифрованный DNS",
  netTestDnsBtn: "Проверить DNS",
  netAdvancedProxyToggle: "Расширенный прокси и правила доменов",
};

const V3_KO: V3Dictionary = {
  ...V3_EN,
  navBookmarks: "북마크",
  historyResumeHint: "기록 화면에서 기록을 다시 누르면 마지막으로 보던 화를 이어서 봅니다",
  historyNoResumeTarget: "이어서 볼 최근 기록이 없습니다",
  pressBackAgainToExit: "한 번 더 뒤로 누르면 종료됩니다",
  bookmarksTitle: "북마크",
  bookmarksSubtitle: "새 챕터 확인 시 우선 체크되며 LAN 동기화에도 포함됩니다.",
  bookmarksSortLabel: "정렬:",
  bookmarksSortNewest: "최근 추가순",
  bookmarksSortHasNew: "새 챕터 우선",
  bookmarksSortRecent: "최근 읽은순",
  bookmarksSortTitle: "제목순",
  bookmarksEmptyTitle: "북마크가 없습니다",
  bookmarksEmptyDesc: "만화 정보 페이지나 뷰어에서 북마크 버튼을 눌러 저장하세요.",
  bookmarkAdd: "북마크",
  bookmarkRemove: "북마크 해제",
  bookmarkedLabel: "북마크됨",
  toastBookmarkAdded: "북마크에 추가됨",
  toastBookmarkRemoved: "북마크에서 제거됨",
  notifCenterTitle: "알림 센터",
  notifMarkAllRead: "모두 읽음",
  notifEmptyTitle: "새 알림이 없습니다",
  notifEmptyDesc: "새 챕터 알림이 여기에 표시됩니다.",
  notifGrantPermissionBtn: "알림 허용",
  scrollToTopLabel: "맨 위로",
  orientationAuto: "회전: 자동",
  orientationPortrait: "세로 고정",
  orientationLandscape: "가로 고정",
  deviceIdentitySection: "기기 이름",
  floatingSection: "플로팅 버튼",
  floatingEdgeLeft: "왼쪽",
  floatingEdgeRight: "오른쪽",
  netProtectionSection: "네트워크 보호 (앱 내 암호화 DNS)",
  netSystemDns: "시스템 DNS",
  netEncryptedDnsOn: "암호화 DNS",
  netTestDnsBtn: "DNS 연결 테스트",
  netAdvancedProxyToggle: "고급 프록시 및 도메인 규칙",
};

const V3_TH: V3Dictionary = {
  ...V3_EN,
  navBookmarks: "บุ๊กมาร์ก",
  historyResumeHint: "กดประวัติอีกครั้งขณะอยู่ในหน้าประวัติเพื่ออ่านต่อทันที",
  historyNoResumeTarget: "ยังไม่มีประวัติการอ่านล่าสุด",
  pressBackAgainToExit: "กดย้อนกลับอีกครั้งเพื่อออก",
  bookmarksTitle: "บุ๊กมาร์ก",
  bookmarksSubtitle: "ตรวจสอบตอนใหม่ก่อนเป็นอันดับแรกและซิงค์ผ่าน LAN โดยอัตโนมัติ",
  bookmarksSortLabel: "เรียงตาม:",
  bookmarksSortNewest: "เพิ่มล่าสุด",
  bookmarksSortHasNew: "มีตอนใหม่ก่อน",
  bookmarksSortRecent: "อ่านล่าสุด",
  bookmarksSortTitle: "ชื่อเรื่อง (A-Z)",
  bookmarksEmptyTitle: "ยังไม่มีบุ๊กมาร์ก",
  bookmarksEmptyDesc: "กดปุ่มบุ๊กมาร์กในหน้ามังงะหรือหน้าอ่านเพื่อบันทึกไว้ที่นี่",
  bookmarkAdd: "บุ๊กมาร์ก",
  bookmarkRemove: "ลบบุ๊กมาร์ก",
  bookmarkedLabel: "บุ๊กมาร์กแล้ว",
  toastBookmarkAdded: "เพิ่มในบุ๊กมาร์กแล้ว",
  toastBookmarkRemoved: "ลบออกจากบุ๊กมาร์กแล้ว",
  notifCenterTitle: "การแจ้งเตือน",
  notifMarkAllRead: "อ่านทั้งหมด",
  notifEmptyTitle: "ไม่มีการแจ้งเตือนใหม่",
  notifEmptyDesc: "การแจ้งเตือนตอนใหม่จะปรากฏที่นี่",
  notifGrantPermissionBtn: "อนุญาตการแจ้งเตือน",
  scrollToTopLabel: "เลื่อนขึ้นด้านบน",
  orientationAuto: "การหมุน: อัตโนมัติ",
  orientationPortrait: "ล็อกแนวตั้ง",
  orientationLandscape: "ล็อกแนวนอน",
  deviceIdentitySection: "ชื่ออุปกรณ์",
  floatingSection: "ปุ่มลอยแบบกำหนดเอง",
  floatingEdgeLeft: "ซ้าย",
  floatingEdgeRight: "ขวา",
  netProtectionSection: "การป้องกันเครือข่าย (DNS ภายในแอป)",
  netSystemDns: "DNS ของระบบ",
  netEncryptedDnsOn: "DNS เข้ารหัส",
  netTestDnsBtn: "ทดสอบ DNS",
  netAdvancedProxyToggle: "พร็อกซีขั้นสูงและกฎตามโดเมน",
};

const V3_DICTIONARIES: Record<Language, V3Dictionary> = {
  en: V3_EN,
  vi: V3_VI,
  zh: V3_ZH,
  ja: V3_JA,
  ru: V3_RU,
  ko: V3_KO,
  th: V3_TH,
};

export interface V4Dictionary {
  // Granular errors
  errUnsupportedSourceTitle: (host: string) => string;
  errTimeoutTitle: string;
  errHttpStatusTitle: (status: number) => string;
  errMissingImagesTitle: string;
  errInvalidImageTitle: string;
  errCorruptedDataTitle: string;

  // Settings reset toasts
  toastSectionReset: string;
  toastAllSettingsReset: string;

  // PageImage & Reader v2.2
  failedPageSkipped: (n: number) => string;
  retryingPage: (n: number, attempt: number) => string;
  readerResetZoom: string;
  quickChapterJump: string;
  noGapModeLabel: string;
  noGapModeDesc: string;
  keepScreenOnLabel: string;
  keepScreenOnDesc: string;
  skipBrokenImagesLabel: string;
  skipBrokenImagesDesc: string;
  chapterDividerFinished: (name: string) => string;
  readerProgressSeek: string;

  // Library, cards & Completed section
  badgeCompleted: string;
  unreadBadge: (n: number) => string;
  coverTapResumeHint: string;
  toastMangaCompletedNoResume: string;
  crashRecoveryTitle: string;
  crashRecoveryDesc: (title: string, chapter: string) => string;
  crashRecoveryResume: string;
  completedSection: string;
  completedSectionDesc: string;

  // Manga detail (Quick jump, Hide, Permanent Delete)
  quickJumpPlaceholder: string;
  quickJumpGo: string;
  quickJumpNotFound: (query: string) => string;
  hideMangaBtn: string;
  hideMangaDesc: string;
  toastMangaHidden: string;
  deleteMangaBtn: string;
  deleteMangaConfirmTitle: string;
  deleteMangaConfirmScope: (title: string) => string;
  deleteMangaConfirmBtn: string;
  toastMangaDeleted: string;

  // Visual Preview
  visualPreviewTitle: string;
  previewGapChip: string;
  previewMarginChip: string;

  // Diagnostics Panel
  diagnosticsSection: string;
  diagnosticsDesc: string;
  diagnosticsCount: (n: number) => string;
  diagnosticsRefreshBtn: string;
  diagnosticsCopyBtn: string;
  diagnosticsExportBtn: string;
  diagnosticsClearBtn: string;
  diagnosticsEmpty: string;
  diagnosticsCopied: string;
  diagnosticsExported: string;
  diagnosticsCleared: string;
  diagnosticsExportFailed: string;

  // Searchable Settings, Behavior, Speed Limit, Scoped Cache, Hidden Manga, Reset All
  settingsSearchPlaceholder: string;
  settingsSearchClear: string;
  settingsSearchEmpty: (query: string) => string;
  resetSectionBtn: string;
  verticalMarginLabel: (px: number) => string;
  verticalMarginDesc: string;
  dimLevelDesc: string;
  horizontalDirDesc: string;
  pageGapDesc: string;
  groupGapDesc: string;
  snapModeDesc: string;
  doubleTapActionLabel: string;
  doubleTapActionDesc: string;
  doubleTapZoom: string;
  doubleTapToggleToolbar: string;
  doubleTapNextPage: string;
  doubleTapDisabled: string;
  toolbarAutoHideLabel: string;
  toolbarAutoHideDesc: string;
  toolbarAutoHideOff: string;
  secondsOption: (sec: number) => string;
  autoMarkReadLabel: string;
  autoMarkReadDesc: string;
  readerBgDesc: string;
  checkIntervalDesc: string;
  speedLimitLabel: string;
  speedLimitDesc: string;
  speedLimitUnlimited: string;
  cachePreservesUserDataNote: string;
  maxCacheDesc: string;
  cacheBySourceTitle: string;
  cacheBySourceDesc: string;
  cacheByMangaTitle: string;
  cacheByMangaDesc: string;
  toastScopedCacheCleared: (target: string) => string;
  hiddenMangaSection: string;
  hiddenMangaDesc: string;
  hiddenMangaEmpty: string;
  unhideMangaBtn: string;
  toastMangaUnhidden: (title: string) => string;
  resetAllSection: string;
  resetAllDesc: string;
  resetAllBtn: string;
  resetAllConfirmTitle: string;
  resetAllConfirmDesc: string;

  // Second-pass polish: DNS disabled banner, collapsible reader categories, floating long-press
  netSystemDnsActiveNote: string;
  netMissingAccountIdWarn: string;
  readerCatLayout: string;
  readerCatDisplay: string;
  readerCatControls: string;
  readerCatBehavior: string;
  openFullSettingsBtn: string;
  longPressConfigHint: string;
  scrollTopQuickConfigTitle: string;
  orientationQuickConfigTitle: string;
  orientationCurrentMode: string;
  buttonVisibilityLabel: string;
  noGapWebtoonHint: string;
}

const V4_EN: V4Dictionary = {
  errUnsupportedSourceTitle: (host) => `Unsupported source (${host})`,
  errTimeoutTitle: "Request timed out",
  errHttpStatusTitle: (status) => `Website returned HTTP ${status}`,
  errMissingImagesTitle: "No readable images found",
  errInvalidImageTitle: "Invalid or unsupported image format",
  errCorruptedDataTitle: "Corrupted or unreadable data",

  toastSectionReset: "Section restored to default settings.",
  toastAllSettingsReset: "All settings restored to defaults. Your library and history were preserved.",

  failedPageSkipped: (n) => `Page ${n} failed (skipped)`,
  retryingPage: (n, attempt) => `Retrying page ${n} (attempt ${attempt})...`,
  readerResetZoom: "Reset zoom",
  quickChapterJump: "Quick chapter jump",
  noGapModeLabel: "No-Gap mode",
  noGapModeDesc: "Remove all vertical spacing between pages for seamless long-strip reading.",
  keepScreenOnLabel: "Keep screen on while reading",
  keepScreenOnDesc: "Prevent the display from sleeping while the reader is active.",
  skipBrokenImagesLabel: "Skip broken images",
  skipBrokenImagesDesc: "Collapse permanently failed pages into a compact inline retry pill.",
  chapterDividerFinished: (name) => `End of ${name}`,
  readerProgressSeek: "Seek reading position",

  badgeCompleted: "Completed",
  unreadBadge: (n) => `${n} unread`,
  coverTapResumeHint: "Tap cover to resume reading",
  toastMangaCompletedNoResume: "You have finished all available chapters. Tap the title to browse chapters.",
  crashRecoveryTitle: "Previous reading session ended unexpectedly",
  crashRecoveryDesc: (title, chapter) => `Resume ${title} — ${chapter}?`,
  crashRecoveryResume: "Resume session",
  completedSection: "Completed",
  completedSectionDesc: "Manga you have caught up on. They automatically return above when a new chapter is detected.",

  quickJumpPlaceholder: "Jump ch #...",
  quickJumpGo: "Go",
  quickJumpNotFound: (q) => `No chapter matching "${q}".`,
  hideMangaBtn: "Hide",
  hideMangaDesc: "Hide this manga from Home, History, and Bookmarks without deleting its data.",
  toastMangaHidden: "Manga hidden. You can restore it anytime in Settings > Hidden Manga.",
  deleteMangaBtn: "Delete",
  deleteMangaConfirmTitle: "Delete manga permanently?",
  deleteMangaConfirmScope: (title) =>
    `This will permanently delete "${title}" along with its chapters, reading history, bookmarks, notifications, and cached page images. This cannot be undone.`,
  deleteMangaConfirmBtn: "Delete permanently",
  toastMangaDeleted: "Manga and all associated local data were permanently deleted.",

  visualPreviewTitle: "Live Layout Preview",
  previewGapChip: "Gap",
  previewMarginChip: "Margin",

  diagnosticsSection: "Crash & Error Diagnostics",
  diagnosticsDesc: "Recent local application errors and crash reports (tokens and credentials are automatically redacted).",
  diagnosticsCount: (n) => (n === 1 ? "1 log entry" : `${n} log entries`),
  diagnosticsRefreshBtn: "Refresh",
  diagnosticsCopyBtn: "Copy JSON",
  diagnosticsExportBtn: "Export Logs",
  diagnosticsClearBtn: "Clear",
  diagnosticsEmpty: "No error or crash logs recorded.",
  diagnosticsCopied: "Sanitized diagnostic logs copied to clipboard.",
  diagnosticsExported: "Sanitized diagnostic logs exported.",
  diagnosticsCleared: "Diagnostic logs cleared.",
  diagnosticsExportFailed: "Could not export diagnostic logs.",

  settingsSearchPlaceholder: "Search settings (e.g. speed, cache, gap, double tap, hidden, logs)...",
  settingsSearchClear: "Clear search",
  settingsSearchEmpty: (q) => `No settings match "${q}".`,
  resetSectionBtn: "Reset",
  verticalMarginLabel: (px) => `Vertical side margin (${px} px)`,
  verticalMarginDesc: "Add breathing room on the left and right edges in vertical reading mode.",
  dimLevelDesc: "Adjust how strongly non-focused pages are dimmed.",
  horizontalDirDesc: "Choose Western (LTR) or Japanese manga (RTL) page progression.",
  pageGapDesc: "Spacing between individual pages inside a spread group.",
  groupGapDesc: "Spacing separating consecutive page groups in horizontal mode.",
  snapModeDesc: "Automatically snap scrolling to individual pages, groups, or scroll freely.",
  doubleTapActionLabel: "Double-tap action",
  doubleTapActionDesc: "Choose what happens when you double-tap inside the reader.",
  doubleTapZoom: "Zoom (1x ↔ 1.8x)",
  doubleTapToggleToolbar: "Toggle toolbar",
  doubleTapNextPage: "Next page / scroll down",
  doubleTapDisabled: "Disabled",
  toolbarAutoHideLabel: "Toolbar auto-hide delay",
  toolbarAutoHideDesc: "Automatically hide reader controls after a period of inactivity.",
  toolbarAutoHideOff: "Off (manual only)",
  secondsOption: (sec) => `${sec} seconds`,
  autoMarkReadLabel: "Auto-mark chapter read on open",
  autoMarkReadDesc: "Immediately mark a chapter as read as soon as you open it in the reader.",
  readerBgDesc: "Choose a dark, light, or app-matched backdrop behind manga pages.",
  checkIntervalDesc: "How frequently background update checks run while the app is open.",
  speedLimitLabel: "Global network speed limit",
  speedLimitDesc: "Throttle total download bandwidth across chapters and images (0 = Unlimited).",
  speedLimitUnlimited: "Unlimited",
  cachePreservesUserDataNote: "Clearing cache only removes temporary images — your library, history, bookmarks, and settings stay safe.",
  maxCacheDesc: "Oldest cached images are automatically evicted when disk usage exceeds this limit.",
  cacheBySourceTitle: "Clear cache by source",
  cacheBySourceDesc: "Remove cached images belonging to a single website.",
  cacheByMangaTitle: "Clear cache by manga",
  cacheByMangaDesc: "Remove cached images for a specific manga title.",
  toastScopedCacheCleared: (target) => `Cleared cached images for ${target}.`,
  hiddenMangaSection: "Hidden Manga",
  hiddenMangaDesc: "Manage manga hidden from Home, History, and Bookmarks. Unhide to restore or delete permanently.",
  hiddenMangaEmpty: "No hidden manga.",
  unhideMangaBtn: "Unhide",
  toastMangaUnhidden: (title) => `Restored "${title}" to your library.`,
  resetAllSection: "Reset All Settings",
  resetAllDesc: "Restore every preference back to factory defaults without deleting your manga, history, bookmarks, or cache.",
  resetAllBtn: "Reset all settings",
  resetAllConfirmTitle: "Reset all settings to defaults?",
  resetAllConfirmDesc:
    "All appearance, reader, floating controls, network, and source preferences will be reset to defaults. Your manga library, chapters, reading history, bookmarks, and image cache will NOT be deleted.",

  netSystemDnsActiveNote:
    "System DNS is active — custom encrypted DNS is disabled. Switch to Encrypted DNS to choose a provider.",
  netMissingAccountIdWarn:
    "Enter your Profile ID above before enabling this provider, or allow system DNS fallback.",
  readerCatLayout: "Layout & Spacing",
  readerCatDisplay: "Display & Visuals",
  readerCatControls: "Controls & Gestures",
  readerCatBehavior: "Reading & Performance",
  openFullSettingsBtn: "Open full settings",
  longPressConfigHint: "Hold to configure",
  scrollTopQuickConfigTitle: "Scroll-to-Top Button",
  orientationQuickConfigTitle: "Screen Rotation",
  orientationCurrentMode: "Screen orientation",
  buttonVisibilityLabel: "Show floating button",
  noGapWebtoonHint: "Auto Webtoon",
};

const V4_VI: V4Dictionary = {
  errUnsupportedSourceTitle: (host) => `Nguồn không được hỗ trợ (${host})`,
  errTimeoutTitle: "Hết thời gian chờ kết nối",
  errHttpStatusTitle: (status) => `Trang web trả về mã lỗi HTTP ${status}`,
  errMissingImagesTitle: "Không tìm thấy ảnh hợp lệ trong chương",
  errInvalidImageTitle: "Định dạng ảnh không hợp lệ hoặc bị hỏng",
  errCorruptedDataTitle: "Dữ liệu bị hỏng hoặc không thể đọc",

  toastSectionReset: "Đã khôi phục mục này về cài đặt mặc định.",
  toastAllSettingsReset: "Đã khôi phục toàn bộ cài đặt mặc định. Truyện và lịch sử của bạn vẫn nguyên vẹn.",

  failedPageSkipped: (n) => `Trang ${n} lỗi (đã bỏ qua)`,
  retryingPage: (n, attempt) => `Đang thử tải lại trang ${n} (lần ${attempt})...`,
  readerResetZoom: "Đặt lại thu phóng",
  quickChapterJump: "Nhảy nhanh tới chương",
  noGapModeLabel: "Chế độ không khoảng cách (No-Gap)",
  noGapModeDesc: "Loại bỏ hoàn toàn khoảng cách dọc giữa các trang để đọc dải dài liền mạch.",
  keepScreenOnLabel: "Giữ màn hình luôn sáng khi đọc",
  keepScreenOnDesc: "Ngăn màn hình tự tắt trong lúc bạn đang mở trình đọc truyện.",
  skipBrokenImagesLabel: "Tự bỏ qua ảnh hỏng",
  skipBrokenImagesDesc: "Thu gọn trang ảnh lỗi hoàn toàn thành thẻ nhỏ để không làm gián đoạn việc đọc.",
  chapterDividerFinished: (name) => `Hết ${name}`,
  readerProgressSeek: "Kéo thanh tiến độ để nhảy trang",

  badgeCompleted: "Đã đọc xong",
  unreadBadge: (n) => `Còn ${n} chưa đọc`,
  coverTapResumeHint: "Chạm ảnh bìa để đọc tiếp",
  toastMangaCompletedNoResume: "Bạn đã đọc hết các chương hiện có. Hãy bấm vào tên truyện để xem danh sách chương.",
  crashRecoveryTitle: "Phiên đọc trước đã bị đóng đột ngột",
  crashRecoveryDesc: (title, chapter) => `Khôi phục đọc tiếp ${title} — ${chapter}?`,
  crashRecoveryResume: "Đọc tiếp ngay",
  completedSection: "Đã đọc xong",
  completedSectionDesc: "Các truyện đã đọc đến chương mới nhất. Khi có chương mới, truyện sẽ tự động quay lại danh sách phía trên.",

  quickJumpPlaceholder: "Nhảy tới chap...",
  quickJumpGo: "Đi",
  quickJumpNotFound: (q) => `Không tìm thấy chương khớp với "${q}".`,
  hideMangaBtn: "Ẩn truyện",
  hideMangaDesc: "Ẩn truyện khỏi Trang chủ, Lịch sử và Đánh dấu mà không xóa dữ liệu.",
  toastMangaHidden: "Đã ẩn truyện. Bạn có thể hiện lại bất cứ lúc nào trong Cài đặt > Truyện đã ẩn.",
  deleteMangaBtn: "Xóa vĩnh viễn",
  deleteMangaConfirmTitle: "Xóa vĩnh viễn truyện này?",
  deleteMangaConfirmScope: (title) =>
    `Hành động này sẽ xóa vĩnh viễn "${title}" cùng toàn bộ danh sách chương, lịch sử đọc, đánh dấu, thông báo và ảnh cache trên máy. Không thể hoàn tác.`,
  deleteMangaConfirmBtn: "Xóa vĩnh viễn",
  toastMangaDeleted: "Đã xóa vĩnh viễn truyện và toàn bộ dữ liệu liên quan.",

  visualPreviewTitle: "Xem trước bố cục trực tiếp",
  previewGapChip: "Khoảng cách",
  previewMarginChip: "Lề ngang",

  diagnosticsSection: "Nhật ký lỗi & Sự cố",
  diagnosticsDesc: "Ghi nhận lỗi gần đây và báo cáo sự cố cục bộ (mật khẩu và token được tự động ẩn).",
  diagnosticsCount: (n) => `${n} bản ghi nhật ký`,
  diagnosticsRefreshBtn: "Làm mới",
  diagnosticsCopyBtn: "Sao chép JSON",
  diagnosticsExportBtn: "Xuất nhật ký",
  diagnosticsClearBtn: "Xóa hết",
  diagnosticsEmpty: "Chưa ghi nhận lỗi hay sự cố nào.",
  diagnosticsCopied: "Đã sao chép nhật ký lỗi (đã ẩn thông tin nhạy cảm).",
  diagnosticsExported: "Đã xuất tệp nhật ký lỗi.",
  diagnosticsCleared: "Đã xóa toàn bộ nhật ký lỗi.",
  diagnosticsExportFailed: "Không thể xuất nhật ký lỗi.",

  settingsSearchPlaceholder: "Tìm kiếm cài đặt (vd: tốc độ, cache, lề, chạm đúp, truyện ẩn, lỗi)...",
  settingsSearchClear: "Xóa tìm kiếm",
  settingsSearchEmpty: (q) => `Không có cài đặt nào khớp với "${q}".`,
  resetSectionBtn: "Mặc định",
  verticalMarginLabel: (px) => `Lề trái/phải khi cuộn dọc (${px} px)`,
  verticalMarginDesc: "Thêm khoảng trống hai bên lề màn hình ở chế độ đọc dọc.",
  dimLevelDesc: "Điều chỉnh mức độ làm tối các trang nằm ngoài vùng đang tập trung.",
  horizontalDirDesc: "Chọn chiều đọc từ trái sang phải (LTR) hoặc kiểu manga Nhật (RTL).",
  pageGapDesc: "Khoảng cách giữa các trang bên trong cùng một nhóm trang ngang.",
  groupGapDesc: "Khoảng cách ngăn cách giữa các nhóm trang liên tiếp khi cuộn ngang.",
  snapModeDesc: "Tự căn dừng theo từng trang, từng nhóm trang hoặc cuộn tự do.",
  doubleTapActionLabel: "Thao tác khi chạm đúp (Double-Tap)",
  doubleTapActionDesc: "Chọn hành động khi bạn chạm nhanh 2 lần trong trình đọc.",
  doubleTapZoom: "Thu phóng (1x ↔ 1.8x)",
  doubleTapToggleToolbar: "Ẩn/hiện thanh công cụ",
  doubleTapNextPage: "Trang kế tiếp / cuộn xuống",
  doubleTapDisabled: "Tắt",
  toolbarAutoHideLabel: "Tự ẩn thanh công cụ sau",
  toolbarAutoHideDesc: "Tự động ẩn thanh điều khiển trình đọc khi không có thao tác.",
  toolbarAutoHideOff: "Tắt (chỉ ẩn thủ công)",
  secondsOption: (sec) => `${sec} giây`,
  autoMarkReadLabel: "Tự đánh dấu đã đọc khi mở chương",
  autoMarkReadDesc: "Đánh dấu chương là đã đọc ngay khi bạn mở chương đó trong trình đọc.",
  readerBgDesc: "Chọn nền tối, nền sáng hoặc tự đồng bộ theo chủ đề ứng dụng.",
  checkIntervalDesc: "Tần suất chạy kiểm tra chương mới ở chế độ nền.",
  speedLimitLabel: "Giới hạn tốc độ mạng toàn cục",
  speedLimitDesc: "Giới hạn tổng băng thông tải ảnh và dữ liệu chương (0 = Không giới hạn).",
  speedLimitUnlimited: "Không giới hạn",
  cachePreservesUserDataNote: "Xóa cache chỉ xóa ảnh tải tạm — truyện, lịch sử đọc, đánh dấu và cài đặt vẫn được giữ nguyên.",
  maxCacheDesc: "Ảnh cũ nhất sẽ tự động được dọn dẹp khi bộ nhớ đệm vượt quá giới hạn này.",
  cacheBySourceTitle: "Xóa cache theo nguồn web",
  cacheBySourceDesc: "Chỉ xóa ảnh tạm thuộc về một trang web cụ thể.",
  cacheByMangaTitle: "Xóa cache theo từng truyện",
  cacheByMangaDesc: "Chỉ xóa ảnh tạm của một bộ truyện cụ thể.",
  toastScopedCacheCleared: (target) => `Đã xóa ảnh cache của ${target}.`,
  hiddenMangaSection: "Quản lý truyện đã ẩn",
  hiddenMangaDesc: "Khôi phục truyện đã ẩn về Trang chủ hoặc xóa vĩnh viễn khỏi thiết bị.",
  hiddenMangaEmpty: "Chưa có truyện nào bị ẩn.",
  unhideMangaBtn: "Hiện lại",
  toastMangaUnhidden: (title) => `Đã khôi phục "${title}" về thư viện.`,
  resetAllSection: "Đặt lại toàn bộ cài đặt",
  resetAllDesc: "Khôi phục tất cả tùy chỉnh về mặc định ban đầu mà không xóa truyện, lịch sử, đánh dấu hay cache.",
  resetAllBtn: "Đặt lại tất cả",
  resetAllConfirmTitle: "Khôi phục toàn bộ cài đặt về mặc định?",
  resetAllConfirmDesc:
    "Mọi tùy chọn giao diện, trình đọc, nút nổi, mạng và nguồn sẽ trở về mặc định. Thư viện truyện, danh sách chương, lịch sử đọc, đánh dấu và cache ảnh sẽ KHÔNG bị xóa.",

  netSystemDnsActiveNote:
    "Đang dùng DNS hệ thống — DNS mã hóa tùy chỉnh đang tắt. Chuyển sang DNS mã hóa để chọn nhà cung cấp.",
  netMissingAccountIdWarn:
    "Hãy nhập Profile ID ở trên trước khi bật nhà cung cấp này, hoặc cho phép dự phòng DNS hệ thống.",
  readerCatLayout: "Bố cục & Khoảng cách",
  readerCatDisplay: "Hiển thị & Giao diện",
  readerCatControls: "Điều khiển & Cử chỉ",
  readerCatBehavior: "Đọc truyện & Hiệu năng",
  openFullSettingsBtn: "Mở cài đặt đầy đủ",
  longPressConfigHint: "Nhấn giữ để cấu hình",
  scrollTopQuickConfigTitle: "Nút Cuộn lên đầu trang",
  orientationQuickConfigTitle: "Xoay màn hình",
  orientationCurrentMode: "Hướng màn hình",
  buttonVisibilityLabel: "Hiển thị nút nổi",
  noGapWebtoonHint: "Tự động Webtoon",
};

const V4_ZH: V4Dictionary = {
  ...V4_EN,
  errUnsupportedSourceTitle: (host) => `不支持的来源 (${host})`,
  errTimeoutTitle: "请求超时",
  errHttpStatusTitle: (status) => `网站返回 HTTP ${status}`,
  errMissingImagesTitle: "未找到可读取的图片",
  errInvalidImageTitle: "无效或损坏的图片格式",
  errCorruptedDataTitle: "数据损坏或无法读取",
  toastSectionReset: "已恢复此分区的默认设置。",
  toastAllSettingsReset: "已恢复所有默认设置，书架与阅读历史已完整保留。",
  failedPageSkipped: (n) => `第 ${n} 页加载失败（已跳过）`,
  retryingPage: (n, attempt) => `正在重试第 ${n} 页（第 ${attempt} 次）...`,
  readerResetZoom: "重置缩放",
  quickChapterJump: "快速跳转章节",
  noGapModeLabel: "无间隙模式",
  noGapModeDesc: "移除所有纵向页间空隙，提供无缝条漫阅读体验。",
  keepScreenOnLabel: "阅读时保持屏幕常亮",
  keepScreenOnDesc: "阅读期间防止设备自动息屏。",
  skipBrokenImagesLabel: "自动跳过损坏图片",
  skipBrokenImagesDesc: "将加载失败的图片折叠为紧凑的重试提示条。",
  chapterDividerFinished: (name) => `${name} 已读完`,
  badgeCompleted: "已追完",
  unreadBadge: (n) => `${n} 话未读`,
  toastMangaCompletedNoResume: "已读完所有最新章节，请点击标题查看章节列表。",
  crashRecoveryTitle: "上次阅读会话意外中断",
  crashRecoveryDesc: (title, chapter) => `是否恢复阅读 ${title} — ${chapter}？`,
  crashRecoveryResume: "恢复阅读",
  completedSection: "已追完",
  completedSectionDesc: "已读至最新话的漫画；检测到新章节时会自动回到上方活跃列表。",
  quickJumpPlaceholder: "跳转话数...",
  quickJumpGo: "跳转",
  hideMangaBtn: "隐藏",
  deleteMangaBtn: "永久删除",
  deleteMangaConfirmTitle: "永久删除此漫画？",
  deleteMangaConfirmBtn: "永久删除",
  visualPreviewTitle: "实时布局预览",
  diagnosticsSection: "崩溃与错误日志",
  settingsSearchPlaceholder: "搜索设置（如：限速、缓存、无间隙、双击、隐藏、日志）...",
  resetSectionBtn: "重置",
  doubleTapActionLabel: "双击操作",
  doubleTapZoom: "缩放 (1x ↔ 1.8x)",
  doubleTapToggleToolbar: "切换工具栏",
  doubleTapNextPage: "下一页 / 向下滚动",
  doubleTapDisabled: "禁用",
  toolbarAutoHideLabel: "工具栏自动隐藏延迟",
  toolbarAutoHideOff: "关闭",
  secondsOption: (sec) => `${sec} 秒`,
  autoMarkReadLabel: "打开章节时自动标为已读",
  speedLimitLabel: "全局网络限速",
  speedLimitUnlimited: "不限速",
  cacheBySourceTitle: "按来源清除缓存",
  cacheByMangaTitle: "按漫画清除缓存",
  hiddenMangaSection: "已隐藏的漫画",
  unhideMangaBtn: "取消隐藏",
  resetAllSection: "重置全部设置",
  resetAllBtn: "重置全部",
};

const V4_JA: V4Dictionary = {
  ...V4_EN,
  errUnsupportedSourceTitle: (host) => `非対応のソース (${host})`,
  errTimeoutTitle: "リクエストがタイムアウトしました",
  errHttpStatusTitle: (status) => `HTTP ${status} エラーが返されました`,
  errMissingImagesTitle: "読み込める画像が見つかりませんでした",
  errInvalidImageTitle: "無効または破損した画像形式です",
  errCorruptedDataTitle: "データが破損しています",
  toastSectionReset: "このセクションをデフォルトに戻しました。",
  toastAllSettingsReset: "すべての設定をデフォルトに戻しました。ライブラリと履歴は保持されます。",
  readerResetZoom: "ズームをリセット",
  quickChapterJump: "チャプターへジャンプ",
  noGapModeLabel: "隙間なしモード",
  keepScreenOnLabel: "閲覧中は画面をオンに保つ",
  skipBrokenImagesLabel: "壊れた画像をスキップ",
  badgeCompleted: "読了",
  unreadBadge: (n) => `未読 ${n}`,
  toastMangaCompletedNoResume: "最新話まで読み終えています。タイトルをタップして一覧を開けます。",
  crashRecoveryTitle: "前回の閲覧セッションが異常終了しました",
  crashRecoveryResume: "再開する",
  completedSection: "読了済み",
  quickJumpPlaceholder: "話数へ移動...",
  quickJumpGo: "移動",
  hideMangaBtn: "非表示",
  deleteMangaBtn: "完全に削除",
  deleteMangaConfirmBtn: "完全に削除",
  visualPreviewTitle: "リアルタイムレイアウトプレビュー",
  diagnosticsSection: "クラッシュ・エラーログ",
  settingsSearchPlaceholder: "設定を検索...",
  resetSectionBtn: "リセット",
  doubleTapActionLabel: "ダブルタップ動作",
  toolbarAutoHideLabel: "ツールバー自動非表示",
  autoMarkReadLabel: "開いたチャプターを自動で既読にする",
  speedLimitLabel: "グローバル通信速度制限",
  speedLimitUnlimited: "無制限",
  hiddenMangaSection: "非表示のマンガ",
  unhideMangaBtn: "表示に戻す",
  resetAllSection: "すべての設定をリセット",
  resetAllBtn: "すべてリセット",
};

const V4_RU: V4Dictionary = {
  ...V4_EN,
  errUnsupportedSourceTitle: (host) => `Неподдерживаемый источник (${host})`,
  errTimeoutTitle: "Время ожидания истекло",
  errHttpStatusTitle: (status) => `Сайт вернул HTTP ${status}`,
  badgeCompleted: "Прочитано",
  unreadBadge: (n) => `Новых: ${n}`,
  completedSection: "Прочитано",
  hideMangaBtn: "Скрыть",
  deleteMangaBtn: "Удалить навсегда",
  visualPreviewTitle: "Предпросмотр макета",
  diagnosticsSection: "Журнал ошибок и сбоев",
  settingsSearchPlaceholder: "Поиск настроек...",
  resetSectionBtn: "Сброс",
  noGapModeLabel: "Без отступов",
  keepScreenOnLabel: "Не выключать экран",
  speedLimitLabel: "Ограничение скорости сети",
  speedLimitUnlimited: "Без лимита",
  hiddenMangaSection: "Скрытая манга",
  unhideMangaBtn: "Показать",
  resetAllSection: "Сбросить все настройки",
  resetAllBtn: "Сбросить всё",
};

const V4_KO: V4Dictionary = {
  ...V4_EN,
  errUnsupportedSourceTitle: (host) => `지원하지 않는 소스 (${host})`,
  errTimeoutTitle: "요청 시간 초과",
  errHttpStatusTitle: (status) => `웹사이트 HTTP ${status} 오류`,
  badgeCompleted: "완독",
  unreadBadge: (n) => `${n}화 안 읽음`,
  completedSection: "완독",
  hideMangaBtn: "숨기기",
  deleteMangaBtn: "영구 삭제",
  visualPreviewTitle: "실시간 레이아웃 미리보기",
  diagnosticsSection: "충돌 및 오류 로그",
  settingsSearchPlaceholder: "설정 검색...",
  resetSectionBtn: "초기화",
  noGapModeLabel: "여백 없음 모드",
  keepScreenOnLabel: "읽는 동안 화면 켜짐 유지",
  speedLimitLabel: "전역 네트워크 속도 제한",
  speedLimitUnlimited: "무제한",
  hiddenMangaSection: "숨긴 만화",
  unhideMangaBtn: "숨기기 해제",
  resetAllSection: "모든 설정 초기화",
  resetAllBtn: "전체 초기화",
};

const V4_TH: V4Dictionary = {
  ...V4_EN,
  errUnsupportedSourceTitle: (host) => `ไม่รองรับแหล่งที่มา (${host})`,
  errTimeoutTitle: "หมดเวลาเชื่อมต่อ",
  errHttpStatusTitle: (status) => `เว็บไซต์ตอบกลับ HTTP ${status}`,
  badgeCompleted: "อ่านจบแล้ว",
  unreadBadge: (n) => `ยังไม่อ่าน ${n}`,
  completedSection: "อ่านจบแล้ว",
  hideMangaBtn: "ซ่อน",
  deleteMangaBtn: "ลบถาวร",
  visualPreviewTitle: "ตัวอย่างการจัดวางแบบเรียลไทม์",
  diagnosticsSection: "บันทึกข้อผิดพลาดและปัญหา",
  settingsSearchPlaceholder: "ค้นหาการตั้งค่า...",
  resetSectionBtn: "รีเซ็ต",
  noGapModeLabel: "โหมดไร้ช่องว่าง",
  keepScreenOnLabel: "เปิดหน้าจอค้างไว้ขณะอ่าน",
  speedLimitLabel: "จำกัดความเร็วเครือข่ายทั้งหมด",
  speedLimitUnlimited: "ไม่จำกัด",
  hiddenMangaSection: "มังงะที่ซ่อนไว้",
  unhideMangaBtn: "เลิกซ่อน",
  resetAllSection: "รีเซ็ตการตั้งค่าทั้งหมด",
  resetAllBtn: "รีเซ็ตทั้งหมด",
};

const V4_DICTIONARIES: Record<Language, V4Dictionary> = {
  en: V4_EN,
  vi: V4_VI,
  zh: V4_ZH,
  ja: V4_JA,
  ru: V4_RU,
  ko: V4_KO,
  th: V4_TH,
};

export interface V5Dictionary {
  // URL input controls (Part 5)
  clearUrlBtn: string;
  pasteUrlBtn: string;
  clipboardEmptyError: string;
  clipboardPermissionError: string;
  clipboardUnsupportedError: string;

  // Granular recognition & network error titles (Part 1)
  errDnsFailureTitle: (host: string) => string;
  errTlsFailureTitle: (host: string) => string;
  errRateLimitedTitle: string;
  errAccessRestrictedTitle: (host: string, status: number) => string;
  errApiFailureTitle: (service: string) => string;
  errMissingMetadataTitle: string;

  // Browse / Catalog page (Part 1.4 & 1.5)
  browseTitle: string;
  browseSearchPlaceholder: string;
  browseSearchBtn: string;
  browseCategoriesLabel: string;
  browseResultsCount: (n: number) => string;
  browseEmptyTitle: string;
  browseEmptyDesc: string;
  browsePrevPage: string;
  browseNextPage: string;
  browsePageIndicator: (cur: number, total: number | null) => string;
  browseOpeningManga: string;
  floatingScreenBrowse: string;

  // Enriched Manga Detail metadata & chapter language filter
  mangaAltTitlesLabel: string;
  mangaAuthorsLabel: string;
  mangaArtistsLabel: string;
  mangaStatusLabel: string;
  mangaGenresLabel: string;
  mangaSynopsisLabel: string;
  mangaLangFilterLabel: string;
  mangaLangAll: string;

  // Reader navigation speed (Part 2.2)
  navigationSpeedLabel: (speed: number) => string;
  navigationSpeedDesc: string;

  // Diagnostics file export path display (Part 6)
  diagnosticsSavedTitle: string;
  diagnosticsCopyPathBtn: string;
  diagnosticsPathCopied: string;
  diagnosticsSaveFailed: (reason: string) => string;

  // Update checking explicit status (Part 7)
  releaseStatusUpToDate: (version: string) => string;
  releaseStatusChecking: string;
  releaseStatusServerUnavailable: string;
  releaseStatusInvalidMetadata: string;
  releaseStatusRateLimited: string;
  releaseStatusNetworkError: string;
  appFullscreenEnter: string;
  appFullscreenExit: string;
}

const V5_EN: V5Dictionary = {
  clearUrlBtn: "Clear URL",
  pasteUrlBtn: "Paste",
  clipboardEmptyError: "Clipboard is empty.",
  clipboardPermissionError: "Clipboard access was denied. Paste directly into the input box.",
  clipboardUnsupportedError: "Clipboard paste is not supported in this environment.",

  errDnsFailureTitle: (host) => `DNS lookup failed (${host})`,
  errTlsFailureTitle: (host) => `TLS/SSL connection failed (${host})`,
  errRateLimitedTitle: "Rate limited by website (HTTP 429)",
  errAccessRestrictedTitle: (host, status) => `Access restricted on ${host} (HTTP ${status})`,
  errApiFailureTitle: (service) => `${service} API error`,
  errMissingMetadataTitle: "Missing required manga metadata",

  browseTitle: "Catalog Browser",
  browseSearchPlaceholder: "Search manga on this website...",
  browseSearchBtn: "Search",
  browseCategoriesLabel: "Categories",
  browseResultsCount: (n) => `${n} ${n === 1 ? "title" : "titles"}`,
  browseEmptyTitle: "No manga entries found",
  browseEmptyDesc: "Try another search query, category, or page.",
  browsePrevPage: "Previous",
  browseNextPage: "Next",
  browsePageIndicator: (cur, total) => (total ? `Page ${cur} / ${total}` : `Page ${cur}`),
  browseOpeningManga: "Opening manga...",
  floatingScreenBrowse: "Catalog",

  mangaAltTitlesLabel: "Alternative titles",
  mangaAuthorsLabel: "Author",
  mangaArtistsLabel: "Artist",
  mangaStatusLabel: "Status",
  mangaGenresLabel: "Genres",
  mangaSynopsisLabel: "Synopsis",
  mangaLangFilterLabel: "Language",
  mangaLangAll: "All",

  navigationSpeedLabel: (speed) => `Scroll & Navigation Speed (${speed}/10)`,
  navigationSpeedDesc:
    "Controls smooth vertical scrolling speed and horizontal mouse-wheel/keyboard stepping responsiveness (without adding horizontal slide animations).",

  diagnosticsSavedTitle: "Diagnostic log exported to:",
  diagnosticsCopyPathBtn: "Copy Path",
  diagnosticsPathCopied: "Log file path copied to clipboard.",
  diagnosticsSaveFailed: (reason) => `Failed to export diagnostic log file: ${reason}`,

  releaseStatusUpToDate: (version) => `Up to date — you are running the latest version (v${version}).`,
  releaseStatusChecking: "Checking for application updates...",
  releaseStatusServerUnavailable: "Update server is currently unavailable. Please try again later.",
  releaseStatusInvalidMetadata: "Update server returned invalid release metadata.",
  releaseStatusRateLimited: "Update check rate-limited by GitHub. Please try again later.",
  releaseStatusNetworkError: "Network failure while checking for updates.",
  appFullscreenEnter: "Enter Fullscreen (F11)",
  appFullscreenExit: "Windowed Mode (F11)",
};

const V5_VI: V5Dictionary = {
  clearUrlBtn: "Xóa URL",
  pasteUrlBtn: "Dán",
  clipboardEmptyError: "Bộ nhớ tạm đang trống.",
  clipboardPermissionError: "Không có quyền đọc bộ nhớ tạm. Hãy dán trực tiếp vào ô nhập.",
  clipboardUnsupportedError: "Môi trường này không hỗ trợ nút dán tự động.",

  errDnsFailureTitle: (host) => `Lỗi phân giải DNS (${host})`,
  errTlsFailureTitle: (host) => `Lỗi kết nối TLS/SSL (${host})`,
  errRateLimitedTitle: "Trang web giới hạn tần suất (HTTP 429)",
  errAccessRestrictedTitle: (host, status) => `Trang ${host} từ chối truy cập (HTTP ${status})`,
  errApiFailureTitle: (service) => `Lỗi API ${service}`,
  errMissingMetadataTitle: "Thiếu thông tin truyện bắt buộc",

  browseTitle: "Duyệt danh mục",
  browseSearchPlaceholder: "Tìm truyện trên trang này...",
  browseSearchBtn: "Tìm",
  browseCategoriesLabel: "Danh mục",
  browseResultsCount: (n) => `${n} truyện`,
  browseEmptyTitle: "Không tìm thấy truyện nào",
  browseEmptyDesc: "Hãy thử từ khóa, thể loại hoặc trang khác.",
  browsePrevPage: "Trang trước",
  browseNextPage: "Trang sau",
  browsePageIndicator: (cur, total) => (total ? `Trang ${cur} / ${total}` : `Trang ${cur}`),
  browseOpeningManga: "Đang mở truyện...",
  floatingScreenBrowse: "Danh mục",

  mangaAltTitlesLabel: "Tên khác",
  mangaAuthorsLabel: "Tác giả",
  mangaArtistsLabel: "Họa sĩ",
  mangaStatusLabel: "Trạng thái",
  mangaGenresLabel: "Thể loại",
  mangaSynopsisLabel: "Giới thiệu",
  mangaLangFilterLabel: "Ngôn ngữ",
  mangaLangAll: "Tất cả",

  navigationSpeedLabel: (speed) => `Tốc độ cuộn & điều hướng (${speed}/10)`,
  navigationSpeedDesc:
    "Điều chỉnh tốc độ cuộn mượt ở chế độ dọc và độ nhạy con lăn/phím ở chế độ ngang (không tạo hiệu ứng trượt trang ngang).",

  diagnosticsSavedTitle: "Đã lưu tệp nhật ký lỗi tại:",
  diagnosticsCopyPathBtn: "Sao chép đường dẫn",
  diagnosticsPathCopied: "Đã sao chép đường dẫn tệp nhật ký.",
  diagnosticsSaveFailed: (reason) => `Xuất tệp nhật ký lỗi thất bại: ${reason}`,

  releaseStatusUpToDate: (version) => `Đã là bản mới nhất — bạn đang dùng RtMaga v${version}.`,
  releaseStatusChecking: "Đang kiểm tra bản cập nhật ứng dụng...",
  releaseStatusServerUnavailable: "Máy chủ cập nhật hiện không khả dụng. Hãy thử lại sau.",
  releaseStatusInvalidMetadata: "Dữ liệu phát hành từ máy chủ không hợp lệ.",
  releaseStatusRateLimited: "Đã chạm giới hạn kiểm tra của GitHub. Hãy thử lại sau.",
  releaseStatusNetworkError: "Lỗi mạng khi kiểm tra bản cập nhật.",
  appFullscreenEnter: "Toàn màn hình (F11)",
  appFullscreenExit: "Chế độ cửa sổ (F11)",
};

const V5_DICTIONARIES: Record<Language, V5Dictionary> = {
  en: V5_EN,
  vi: V5_VI,
  zh: V5_EN,
  ja: V5_EN,
  ru: V5_EN,
  ko: V5_EN,
  th: V5_EN,
};

export type FullDictionary = Dictionary & V3Dictionary & V4Dictionary & V5Dictionary;

const DICTIONARIES: Record<Language, Dictionary> = { en, vi, zh, ja, ru, ko, th };

export function getDict(lang: Language): FullDictionary {
  return {
    ...(DICTIONARIES[lang] ?? en),
    ...(V3_DICTIONARIES[lang] ?? V3_EN),
    ...(V4_DICTIONARIES[lang] ?? V4_EN),
    ...(V5_DICTIONARIES[lang] ?? V5_EN),
  };
}

export function getLocale(lang: Language): string {
  return LANGUAGE_OPTIONS.find((o) => o.code === lang)?.locale ?? "en-US";
}
