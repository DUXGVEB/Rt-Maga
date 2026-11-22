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
  homeSubtitle: string;
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
  homeSubtitle:
    "Supports manga overview links and direct chapter URLs from TruyenQQ and most manga websites.",
  newChapters: "New Chapters",
  continueReading: "Continue Reading",
  viewAllHistory: "View all history",
  noRecentTitle: "No recent manga yet",
  noRecentDesc:
    "Paste any manga link above to start reading. Your reading progress will appear here automatically.",

  linkPlaceholder: "Paste any manga or chapter URL (e.g. https://truyenqqko.com/truyen-tranh/...)",
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
  readerSection: "Vertical Reader",
  widthModeLabel: "Image width mode",
  widthModeDesc: "Set a fixed pixel width, viewport percentage, or fit to window.",
  widthPixels: "Pixels",
  widthPercent: "Percent",
  widthLabelPx: (px) => `Width (${px} px)`,
  widthLabelPct: (pct) => `Width (${pct}%)`,
  gapLabel: (px) => `Gap between pages (${px} px)`,
  gapDesc: "Set to 0 px for seamless Webtoon-style strips.",
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
  headlessLabel: "Headless browser fallback (Edge/Chrome)",
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

  errInvalidUrlTitle: "Invalid URL",
  errInvalidUrlDetail: "Please paste a valid manga or chapter link (starting with https://...).",
  errBlockedTitle: (host) => `${host} requires verification`,
  errBlockedDetail:
    "This site enabled anti-bot protection (Cloudflare). Open a browser window to complete verification, then try again.",
  errNotRecognizedTitle: "Could not recognize manga content",
  errNotRecognizedDetail:
    "Try pasting a direct manga overview or chapter link, or enable Headless Browser fallback in Settings.",
  errNetworkTitle: "Could not connect to the website",
  errNotFoundTitle: "Not found",
  errAppTitle: "Application error",
  errGenericTitle: "Something went wrong",
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
  homeSubtitle:
    "Hỗ trợ link trang truyện hoặc link chương từ TruyenQQ và hầu hết các web truyện tranh khác.",
  newChapters: "Có chap mới",
  continueReading: "Đọc tiếp",
  viewAllHistory: "Xem tất cả lịch sử",
  noRecentTitle: "Chưa có truyện nào gần đây",
  noRecentDesc:
    "Dán một link truyện vào ô phía trên để bắt đầu. Tiến độ đọc sẽ tự động lưu tại đây.",

  linkPlaceholder: "Dán link truyện hoặc link chương bất kỳ (vd: https://truyenqqko.com/truyen-tranh/...)",
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
  readerSection: "Trình đọc cuộn dọc",
  widthModeLabel: "Chế độ độ rộng ảnh",
  widthModeDesc: "Giữ cố định theo pixel, theo phần trăm hoặc vừa cửa sổ.",
  widthPixels: "Pixel",
  widthPercent: "Phần trăm",
  widthLabelPx: (px) => `Độ rộng (${px} px)`,
  widthLabelPct: (pct) => `Độ rộng (${pct}%)`,
  gapLabel: (px) => `Khoảng cách giữa các ảnh (${px} px)`,
  gapDesc: "Để 0 px để ảnh nối liền mạch kiểu webtoon.",
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
  headlessLabel: "Trình duyệt chạy ngầm (Edge/Chrome)",
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

  errInvalidUrlTitle: "Đường dẫn không hợp lệ",
  errInvalidUrlDetail: "Hãy dán link truyện hoặc link chương hợp lệ (bắt đầu bằng https://...).",
  errBlockedTitle: (host) => `Trang ${host} yêu cầu xác minh`,
  errBlockedDetail:
    "Trang web đang bật kiểm tra chống bot (Cloudflare). Hãy mở cửa sổ để tự xác minh rồi thử lại.",
  errNotRecognizedTitle: "Chưa nhận diện được truyện từ link này",
  errNotRecognizedDetail:
    "Hãy thử dán trực tiếp link trang truyện hoặc trang chương, hoặc bật Headless trong Cài đặt.",
  errNetworkTitle: "Không kết nối được tới trang truyện",
  errNotFoundTitle: "Không tìm thấy dữ liệu",
  errAppTitle: "Lỗi ứng dụng",
  errGenericTitle: "Đã xảy ra lỗi",
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
  homeSubtitle: "支持粘贴漫画详情页或章节直链，适配 TruyenQQ 及大多数漫画网站。",
  newChapters: "有新章节",
  continueReading: "继续阅读",
  viewAllHistory: "查看全部历史",
  noRecentTitle: "暂无阅读记录",
  noRecentDesc: "在上方粘贴漫画链接开始阅读，您的进度会自动保存在这里。",

  linkPlaceholder: "粘贴任意漫画或章节链接 (例如 https://truyenqqko.com/truyen-tranh/...)",
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
  readerSection: "条漫阅读器",
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
  headlessLabel: "无头浏览器兜底 (Edge/Chrome)",
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

  errInvalidUrlTitle: "链接无效",
  errInvalidUrlDetail: "请粘贴有效的漫画或章节链接（以 https:// 开头）。",
  errBlockedTitle: (host) => `${host} 需要人机验证`,
  errBlockedDetail: "该网站启用了防爬虫验证（Cloudflare），请打开浏览器窗口完成验证后重试。",
  errNotRecognizedTitle: "无法识别漫画内容",
  errNotRecognizedDetail: "请尝试粘贴漫画详情页或章节直链，或在设置中开启无头浏览器。",
  errNetworkTitle: "无法连接到网站",
  errNotFoundTitle: "未找到数据",
  errAppTitle: "应用错误",
  errGenericTitle: "发生错误",
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
  homeSubtitle: "TruyenQQ や多くのマンガサイトの作品ページ・話ページ URL に対応しています。",
  newChapters: "新着チャプター",
  continueReading: "続きを読む",
  viewAllHistory: "履歴をすべて見る",
  noRecentTitle: "最近読んだ作品はありません",
  noRecentDesc: "上の入力欄にリンクを貼ると読み始められ、進捗がここに保存されます。",

  linkPlaceholder: "作品またはチャプターの URL を貼り付け (例: https://truyenqqko.com/truyen-tranh/...)",
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
  readerSection: "縦スクロールリーダー",
  widthModeLabel: "画像幅モード",
  widthModeDesc: "固定ピクセル、画面比率、またはウィンドウ幅に合わせます。",
  widthPixels: "ピクセル",
  widthPercent: "パーセント",
  widthLabelPx: (px) => `幅 (${px} px)`,
  widthLabelPct: (pct) => `幅 (${pct}%)`,
  gapLabel: (px) => `画像の間隔 (${px} px)`,
  gapDesc: "0 px にすると Webtoon のように隙間なく表示されます。",
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
  headlessLabel: "ヘッドレスブラウザ補助 (Edge/Chrome)",
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

  errInvalidUrlTitle: "無効な URL",
  errInvalidUrlDetail: "https:// から始まる有効な作品・話リンクを貼り付けてください。",
  errBlockedTitle: (host) => `${host} で認証が必要です`,
  errBlockedDetail:
    "ボット対策 (Cloudflare) が有効です。ブラウザウィンドウで認証後に再試行してください。",
  errNotRecognizedTitle: "マンガを認識できませんでした",
  errNotRecognizedDetail:
    "作品ページまたは話ページを直接貼るか、設定でヘッドレスブラウザを有効にしてください。",
  errNetworkTitle: "サイトに接続できませんでした",
  errNotFoundTitle: "データが見つかりません",
  errAppTitle: "アプリエラー",
  errGenericTitle: "エラーが発生しました",
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
  homeSubtitle:
    "Поддерживаются ссылки на страницу манги или главы с TruyenQQ и большинства сайтов.",
  newChapters: "Новые главы",
  continueReading: "Продолжить чтение",
  viewAllHistory: "Вся история",
  noRecentTitle: "Недавней манги пока нет",
  noRecentDesc:
    "Вставьте ссылку выше, чтобы начать. Прогресс чтения будет сохраняться здесь автоматически.",

  linkPlaceholder: "Вставьте ссылку на мангу или главу (напр. https://truyenqqko.com/truyen-tranh/...)",
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
  readerSection: "Вертикальная читалка",
  widthModeLabel: "Режим ширины страниц",
  widthModeDesc: "Фиксированная ширина в пикселях, процентах или по окну.",
  widthPixels: "Пиксели",
  widthPercent: "Проценты",
  widthLabelPx: (px) => `Ширина (${px} px)`,
  widthLabelPct: (pct) => `Ширина (${pct}%)`,
  gapLabel: (px) => `Отступ между страницами (${px} px)`,
  gapDesc: "0 px — бесшовная лента в стиле Webtoon.",
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
  headlessLabel: "Фоновый браузер (Edge/Chrome)",
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

  errInvalidUrlTitle: "Некорректная ссылка",
  errInvalidUrlDetail: "Вставьте корректную ссылку на мангу или главу (начинается с https://...).",
  errBlockedTitle: (host) => `${host} требует подтверждения`,
  errBlockedDetail:
    "На сайте включена защита от ботов (Cloudflare). Пройдите проверку в открывшемся окне и повторите.",
  errNotRecognizedTitle: "Не удалось распознать мангу",
  errNotRecognizedDetail:
    "Попробуйте прямую ссылку на мангу/главу или включите фоновый браузер в настройках.",
  errNetworkTitle: "Ошибка подключения к сайту",
  errNotFoundTitle: "Не найдено",
  errAppTitle: "Ошибка приложения",
  errGenericTitle: "Произошла ошибка",
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
  homeSubtitle: "TruyenQQ 및 대부분의 만화 사이트의 작품 페이지와 챕터 링크를 지원합니다.",
  newChapters: "새 챕터",
  continueReading: "이어서 읽기",
  viewAllHistory: "전체 기록 보기",
  noRecentTitle: "최근 읽은 만화가 없습니다",
  noRecentDesc: "위에 만화 링크를 붙여넣어 시작하세요. 읽기 진행 상황이 자동으로 저장됩니다.",

  linkPlaceholder: "만화 또는 챕터 URL 붙여넣기 (예: https://truyenqqko.com/truyen-tranh/...)",
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
  readerSection: "세로 스크롤 리더",
  widthModeLabel: "이미지 너비 모드",
  widthModeDesc: "고정 픽셀, 화면 비율 또는 창 맞춤을 선택합니다.",
  widthPixels: "픽셀",
  widthPercent: "퍼센트",
  widthLabelPx: (px) => `너비 (${px} px)`,
  widthLabelPct: (pct) => `너비 (${pct}%)`,
  gapLabel: (px) => `페이지 간격 (${px} px)`,
  gapDesc: "0 px로 설정하면 웹툰처럼 빈틈없이 이어집니다.",
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
  headlessLabel: "헤드리스 브라우저 보조 (Edge/Chrome)",
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

  errInvalidUrlTitle: "잘못된 URL",
  errInvalidUrlDetail: "https:// 로 시작하는 유효한 만화 또는 챕터 링크를 붙여넣어 주세요.",
  errBlockedTitle: (host) => `${host}에서 인증이 필요합니다`,
  errBlockedDetail:
    "봇 차단(Cloudflare)이 활성화되어 있습니다. 브라우저 창에서 인증을 완료한 뒤 다시 시도하세요.",
  errNotRecognizedTitle: "만화 콘텐츠를 인식하지 못했습니다",
  errNotRecognizedDetail:
    "작품 또는 챕터 링크를 직접 붙여넣거나 설정에서 헤드리스 브라우저를 켜 보세요.",
  errNetworkTitle: "사이트에 연결할 수 없습니다",
  errNotFoundTitle: "찾을 수 없음",
  errAppTitle: "애플리케이션 오류",
  errGenericTitle: "오류가 발생했습니다",
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
  homeSubtitle: "รองรับทั้งลิงก์หน้าเรื่องและลิงก์ตอนจาก TruyenQQ และเว็บมังงะส่วนใหญ่",
  newChapters: "มีตอนใหม่",
  continueReading: "อ่านต่อ",
  viewAllHistory: "ดูประวัติทั้งหมด",
  noRecentTitle: "ยังไม่มีประวัติการอ่าน",
  noRecentDesc: "วางลิงก์มังงะด้านบนเพื่อเริ่มอ่าน ความคืบหน้าจะถูกบันทึกไว้ที่นี่อัตโนมัติ",

  linkPlaceholder: "วางลิงก์มังงะหรือลิงก์ตอนใดก็ได้ (เช่น https://truyenqqko.com/truyen-tranh/...)",
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
  readerSection: "ตัวอ่านเลื่อนแนวตั้ง",
  widthModeLabel: "โหมดความกว้างภาพ",
  widthModeDesc: "กำหนดความกว้างเป็นพิกเซล เปอร์เซ็นต์ หรือพอดีหน้าต่าง",
  widthPixels: "พิกเซล",
  widthPercent: "เปอร์เซ็นต์",
  widthLabelPx: (px) => `ความกว้าง (${px} px)`,
  widthLabelPct: (pct) => `ความกว้าง (${pct}%)`,
  gapLabel: (px) => `ระยะห่างระหว่างภาพ (${px} px)`,
  gapDesc: "ตั้งเป็น 0 px เพื่อให้อ่านต่อเนื่องแบบเว็บตูน",
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
  headlessLabel: "เบราว์เซอร์เบื้องหลัง (Edge/Chrome)",
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

  errInvalidUrlTitle: "ลิงก์ไม่ถูกต้อง",
  errInvalidUrlDetail: "กรุณาวางลิงก์มังงะหรือตอนที่ถูกต้อง (ขึ้นต้นด้วย https://...)",
  errBlockedTitle: (host) => `${host} ต้องการการยืนยันตัวตน`,
  errBlockedDetail:
    "เว็บไซต์เปิดการป้องกันบอท (Cloudflare) กรุณาเปิดหน้าต่างเบราว์เซอร์เพื่อยืนยันแล้วลองใหม่",
  errNotRecognizedTitle: "ไม่พบข้อมูลมังงะจากลิงก์นี้",
  errNotRecognizedDetail:
    "ลองวางลิงก์หน้าเรื่องหรือตอนโดยตรง หรือเปิดโหมดเบราว์เซอร์เบื้องหลังในการตั้งค่า",
  errNetworkTitle: "ไม่สามารถเชื่อมต่อเว็บไซต์ได้",
  errNotFoundTitle: "ไม่พบข้อมูล",
  errAppTitle: "ข้อผิดพลาดของแอป",
  errGenericTitle: "เกิดข้อผิดพลาด",
};

const DICTIONARIES: Record<Language, Dictionary> = { en, vi, zh, ja, ru, ko, th };

export function getDict(lang: Language): Dictionary {
  return DICTIONARIES[lang] ?? en;
}

export function getLocale(lang: Language): string {
  return LANGUAGE_OPTIONS.find((o) => o.code === lang)?.locale ?? "en-US";
}
