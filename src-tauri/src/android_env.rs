//! Cross-platform device environment, real device identity, mobile orientation lock,
//! immersive system bars, and Android notification channel/permission bridge.

#[cfg(target_os = "windows")]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[cfg(not(target_os = "android"))]
use crate::error::AppError;

/// Execution mode for Tier 1 on-device OCR/bubble detection.
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tier1RunMode {
    /// Detect speech-bubble bounding boxes only (Bubble Auto-Zoom without translation).
    DetectOnly,
    /// Run full OCR + translation pipeline.
    OcrAndTranslate,
}

/// Hardware and native AI runtime capabilities reported by the platform bridge.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors Android AiRuntimePlugin capability flags"
)]
pub(crate) struct AiRuntimeCapabilities {
    pub onnx_ready: bool,
    pub ml_kit_ready: bool,
    pub llama_jni_ready: bool,
    pub vulkan_supported: bool,
}

/// Single speech-bubble / translated region returned by Android `AiRuntimePlugin`.
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct AndroidAiRegion {
    pub text: String,
    pub source_text: Option<String>,
    pub bbox: [u16; 4],
}

/// Response returned by Android `AiRuntimePlugin.runTier1OcrAndTranslate`.
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct AndroidTier1Reply {
    pub regions: Vec<AndroidAiRegion>,
}

/// Response returned by Android `AiRuntimePlugin.runTier2Vlm`.
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct AndroidTier2Reply {
    pub raw_json: String,
}

/// Display safe-area and gesture inset measurements in logical CSS pixels (`dp`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct DeviceInsets {
    pub cutout_top: u16,
    pub cutout_bottom: u16,
    pub cutout_left: u16,
    pub cutout_right: u16,
    pub gesture_left: u16,
    pub gesture_right: u16,
    pub ime_bottom: u16,
}

/// Adaptive runtime device environment reported to the Svelte frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct DeviceEnvInfo {
    pub platform: String,
    pub sdk_int: u32,
    pub manufacturer: String,
    pub model: String,
    pub skin: String,
    pub real_device_name: String,
    pub orientation: String,
    pub orientation_lock: String,
    pub viewport_width_dp: u32,
    pub viewport_height_dp: u32,
    pub immersive: bool,
    pub insets: DeviceInsets,
}

impl Default for DeviceEnvInfo {
    fn default() -> Self {
        Self {
            platform: if cfg!(target_os = "android") {
                "android".to_owned()
            } else if cfg!(target_os = "windows") {
                "windows".to_owned()
            } else {
                "other".to_owned()
            },
            sdk_int: 0,
            manufacturer: String::new(),
            model: String::new(),
            skin: "desktop".to_owned(),
            real_device_name: detect_os_device_name(),
            orientation: "landscape".to_owned(),
            orientation_lock: "auto".to_owned(),
            viewport_width_dp: 0,
            viewport_height_dp: 0,
            immersive: false,
            insets: DeviceInsets::default(),
        }
    }
}

/// Notification permission check/request result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct NotificationPermissionInfo {
    pub granted: bool,
    pub can_request: bool,
}

impl Default for NotificationPermissionInfo {
    fn default() -> Self {
        Self {
            granted: true,
            can_request: false,
        }
    }
}

/// Result of attempting to apply an orientation lock on mobile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OrientationLockReply {
    pub ok: bool,
    pub mode: String,
}

/// Result returned by Android SAF backup export (`ACTION_CREATE_DOCUMENT`).
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SafExportReply {
    pub cancelled: bool,
    pub uri: Option<String>,
    pub bytes: u64,
}

/// Result returned by Android SAF backup picker (`ACTION_OPEN_DOCUMENT`).
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct SafPickReply {
    pub cancelled: bool,
    pub path: Option<String>,
    pub display_name: Option<String>,
}

/// Result returned by Android MediaStore Gallery image save.
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct GallerySaveReply {
    pub saved_path: String,
    pub uri: Option<String>,
}

/// Returns the best-effort real OS/hardware device name (without custom user override).
pub(crate) fn detect_os_device_name() -> String {
    let from_env = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    from_env.unwrap_or_else(|| {
        if cfg!(target_os = "android") {
            "Android Device".to_owned()
        } else if cfg!(target_os = "windows") {
            "Windows PC".to_owned()
        } else {
            "RtMaga Device".to_owned()
        }
    })
}

/// Resolves the effective display name (custom user override if non-empty, otherwise real device name).
pub(crate) fn resolve_display_device_name(
    app: Option<&AppHandle>,
    custom_override: Option<&str>,
) -> String {
    if let Some(custom) = custom_override.map(str::trim).filter(|s| !s.is_empty()) {
        return custom.chars().take(48).collect();
    }
    if let Some(handle) = app
        && let Ok(env) = get_device_env(handle)
        && !env.real_device_name.trim().is_empty()
    {
        return env.real_device_name;
    }
    detect_os_device_name()
}

#[cfg(target_os = "android")]
mod imp {
    use std::path::{Path, PathBuf};

    use serde::Deserialize;
    use tauri::plugin::{Builder as PluginBuilder, PluginHandle, TauriPlugin};
    use tauri::{AppHandle, Manager, Runtime};

    use super::{
        AiRuntimeCapabilities, AndroidTier1Reply, AndroidTier2Reply, DeviceEnvInfo,
        GallerySaveReply, NotificationPermissionInfo, OrientationLockReply, SafExportReply,
        SafPickReply, Tier1RunMode,
    };
    use crate::error::AppError;

    struct DeviceEnvState<R: Runtime>(PluginHandle<R>);
    struct AiRuntimeState<R: Runtime>(PluginHandle<R>);

    #[derive(Debug, Deserialize)]
    struct SendNotifReply {
        sent: bool,
    }

    #[derive(Debug, Deserialize)]
    struct ClipboardTextReply {
        text: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct AiStorageDirReply {
        path: String,
    }

    #[derive(Debug, Deserialize)]
    struct HardUnloadReply {
        unloaded: bool,
    }

    pub(crate) fn init_plugin<R: Runtime>() -> TauriPlugin<R> {
        PluginBuilder::new("device_env")
            .setup(|app, api| {
                let handle = api.register_android_plugin("com.rtmaga.reader", "DeviceEnvPlugin")?;
                app.manage(DeviceEnvState(handle));
                Ok(())
            })
            .build()
    }

    pub(crate) fn init_ai_plugin<R: Runtime>() -> TauriPlugin<R> {
        PluginBuilder::new("ai-runtime")
            .setup(|app, api| {
                let handle = api.register_android_plugin("com.rtmaga.reader", "AiRuntimePlugin")?;
                app.manage(AiRuntimeState(handle));
                Ok(())
            })
            .build()
    }

    pub(crate) fn get_ai_storage_dir(app: &AppHandle) -> Option<PathBuf> {
        let state = app.try_state::<AiRuntimeState<tauri::Wry>>()?;
        let reply: AiStorageDirReply = state
            .0
            .run_mobile_plugin("getAiStorageDir", serde_json::json!({}))
            .ok()?;
        let trimmed = reply.path.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(PathBuf::from(trimmed))
        }
    }

    pub(crate) fn get_runtime_capabilities(app: &AppHandle) -> AiRuntimeCapabilities {
        let Some(state) = app.try_state::<AiRuntimeState<tauri::Wry>>() else {
            return AiRuntimeCapabilities::default();
        };
        state
            .0
            .run_mobile_plugin("getRuntimeCapabilities", serde_json::json!({}))
            .unwrap_or_default()
    }

    pub(crate) fn run_tier1_ocr_and_translate(
        app: &AppHandle,
        image_path: &Path,
        target_lang: &str,
        mode: Tier1RunMode,
    ) -> Result<AndroidTier1Reply, AppError> {
        let state = app.state::<AiRuntimeState<tauri::Wry>>();
        let detect_only = matches!(mode, Tier1RunMode::DetectOnly);
        state
            .0
            .run_mobile_plugin(
                "runTier1OcrAndTranslate",
                serde_json::json!({
                    "imagePath": image_path.to_string_lossy(),
                    "targetLang": target_lang,
                    "detectOnly": detect_only,
                }),
            )
            .map_err(|e| AppError::Internal {
                message: format!("Android Tier 1 AI inference failed: {e}"),
            })
    }

    pub(crate) fn run_tier2_vlm(
        app: &AppHandle,
        image_path: &Path,
        model_path: &Path,
        mmproj_path: &Path,
        target_lang: &str,
        prompt: &str,
    ) -> Result<AndroidTier2Reply, AppError> {
        let state = app.state::<AiRuntimeState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin(
                "runTier2Vlm",
                serde_json::json!({
                    "imagePath": image_path.to_string_lossy(),
                    "modelPath": model_path.to_string_lossy(),
                    "mmprojPath": mmproj_path.to_string_lossy(),
                    "targetLang": target_lang,
                    "prompt": prompt,
                }),
            )
            .map_err(|e| AppError::Internal {
                message: format!("Android Tier 2 VLM inference failed: {e}"),
            })
    }

    pub(crate) fn cancel_active_ai_jobs(app: &AppHandle) {
        if let Some(state) = app.try_state::<AiRuntimeState<tauri::Wry>>() {
            let _: Result<serde_json::Value, _> = state
                .0
                .run_mobile_plugin("cancelActiveJobs", serde_json::json!({}));
        }
    }

    pub(crate) fn hard_unload_all_ai(app: &AppHandle) -> bool {
        let Some(state) = app.try_state::<AiRuntimeState<tauri::Wry>>() else {
            return false;
        };
        state
            .0
            .run_mobile_plugin::<HardUnloadReply>("hardUnloadAll", serde_json::json!({}))
            .is_ok_and(|r| r.unloaded)
    }

    pub(crate) fn get_device_env(app: &AppHandle) -> Result<DeviceEnvInfo, AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin("getDeviceEnv", serde_json::json!({}))
            .map_err(|e| AppError::Internal {
                message: format!("failed to query Android device environment: {e}"),
            })
    }

    pub(crate) fn set_orientation_lock(
        app: &AppHandle,
        mode: &str,
    ) -> Result<OrientationLockReply, AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin("setOrientationLock", serde_json::json!({ "mode": mode }))
            .map_err(|e| AppError::Internal {
                message: format!("failed to set Android orientation lock: {e}"),
            })
    }

    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "mirrors Android immersive toggle IPC argument"
    )]
    pub(crate) fn set_immersive(app: &AppHandle, enabled: bool) -> Result<(), AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        let _: serde_json::Value = state
            .0
            .run_mobile_plugin("setImmersive", serde_json::json!({ "enabled": enabled }))
            .map_err(|e| AppError::Internal {
                message: format!("failed to update Android immersive mode: {e}"),
            })?;
        Ok(())
    }

    #[expect(
        clippy::fn_params_excessive_bools,
        reason = "mirrors Android keep-screen-on toggle IPC argument"
    )]
    pub(crate) fn set_keep_screen_on(app: &AppHandle, enabled: bool) -> Result<(), AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        let _: serde_json::Value = state
            .0
            .run_mobile_plugin("setKeepScreenOn", serde_json::json!({ "enabled": enabled }))
            .map_err(|e| AppError::Internal {
                message: format!("failed to update Android keep-screen-on flag: {e}"),
            })?;
        Ok(())
    }

    pub(crate) fn check_notification_permission(
        app: &AppHandle,
    ) -> Result<NotificationPermissionInfo, AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin("checkNotificationPermission", serde_json::json!({}))
            .map_err(|e| AppError::Internal {
                message: format!("failed to check Android notification permission: {e}"),
            })
    }

    pub(crate) fn request_notification_permission(
        app: &AppHandle,
    ) -> Result<NotificationPermissionInfo, AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin("requestNotificationPermission", serde_json::json!({}))
            .map_err(|e| AppError::Internal {
                message: format!("failed to request Android notification permission: {e}"),
            })
    }

    pub(crate) fn send_android_notification(
        app: &AppHandle,
        title: &str,
        body: &str,
        notification_id: i32,
    ) -> bool {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin::<SendNotifReply>(
                "sendLocalNotification",
                serde_json::json!({
                    "title": title,
                    "body": body,
                    "notificationId": notification_id
                }),
            )
            .is_ok_and(|r| r.sent)
    }

    pub(crate) fn read_clipboard_text(app: &AppHandle) -> Result<Option<String>, AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        let reply: ClipboardTextReply = state
            .0
            .run_mobile_plugin("readClipboardText", serde_json::json!({}))
            .map_err(|e| AppError::Internal {
                message: format!("failed to read Android clipboard: {e}"),
            })?;
        Ok(reply.text.and_then(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            }
        }))
    }

    pub(crate) fn export_backup_saf(
        app: &AppHandle,
        temp_path: &Path,
        file_name: &str,
        mime_type: &str,
    ) -> Result<SafExportReply, AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin(
                "exportBackupSaf",
                serde_json::json!({
                    "tempPath": temp_path.to_string_lossy(),
                    "fileName": file_name,
                    "mimeType": mime_type,
                }),
            )
            .map_err(|e| AppError::Internal {
                message: format!("failed to export backup via Android SAF: {e}"),
            })
    }

    pub(crate) fn pick_backup_saf(app: &AppHandle) -> Result<SafPickReply, AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin("pickBackupSaf", serde_json::json!({}))
            .map_err(|e| AppError::Internal {
                message: format!("failed to pick backup via Android SAF: {e}"),
            })
    }

    pub(crate) fn save_image_to_gallery(
        app: &AppHandle,
        temp_path: &Path,
        file_name: &str,
        mime_type: &str,
    ) -> Result<GallerySaveReply, AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        state
            .0
            .run_mobile_plugin(
                "saveImageToGallery",
                serde_json::json!({
                    "tempPath": temp_path.to_string_lossy(),
                    "fileName": file_name,
                    "mimeType": mime_type,
                }),
            )
            .map_err(|e| AppError::Internal {
                message: format!("failed to save image to Android Gallery: {e}"),
            })
    }

    pub(crate) fn open_external_url_android(app: &AppHandle, url: &str) -> Result<(), AppError> {
        let state = app.state::<DeviceEnvState<tauri::Wry>>();
        let _: serde_json::Value = state
            .0
            .run_mobile_plugin("openExternalUrl", serde_json::json!({ "url": url }))
            .map_err(|e| AppError::Internal {
                message: format!("failed to open external browser intent: {e}"),
            })?;
        Ok(())
    }
}

#[cfg(target_os = "android")]
pub(crate) use imp::{
    cancel_active_ai_jobs, check_notification_permission, export_backup_saf, get_ai_storage_dir,
    get_device_env, get_runtime_capabilities, hard_unload_all_ai, init_ai_plugin, init_plugin,
    open_external_url_android, pick_backup_saf, read_clipboard_text,
    request_notification_permission, run_tier1_ocr_and_translate, run_tier2_vlm,
    save_image_to_gallery, send_android_notification, set_immersive, set_keep_screen_on,
    set_orientation_lock,
};

#[cfg(not(target_os = "android"))]
pub(crate) fn get_runtime_capabilities(_app: &AppHandle) -> AiRuntimeCapabilities {
    AiRuntimeCapabilities {
        onnx_ready: true,
        ml_kit_ready: false,
        llama_jni_ready: false,
        vulkan_supported: detect_windows_vulkan_loader(),
    }
}

#[cfg(not(target_os = "android"))]
pub(crate) fn cancel_active_ai_jobs(_app: &AppHandle) {}

#[cfg(not(target_os = "android"))]
pub(crate) fn hard_unload_all_ai(_app: &AppHandle) -> bool {
    true
}

#[cfg(target_os = "windows")]
fn detect_windows_vulkan_loader() -> bool {
    let sys32 = std::env::var_os("SystemRoot")
        .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from)
        .join("System32")
        .join("vulkan-1.dll");
    sys32.is_file()
}

#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
fn detect_windows_vulkan_loader() -> bool {
    false
}

#[cfg(not(target_os = "android"))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "matches fallible Android plugin signature"
)]
pub(crate) fn get_device_env(_app: &AppHandle) -> Result<DeviceEnvInfo, AppError> {
    Ok(DeviceEnvInfo::default())
}

#[cfg(not(target_os = "android"))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "matches fallible Android plugin signature"
)]
pub(crate) fn set_orientation_lock(
    _app: &AppHandle,
    mode: &str,
) -> Result<OrientationLockReply, AppError> {
    Ok(OrientationLockReply {
        ok: false,
        mode: mode.to_owned(),
    })
}

#[cfg(not(target_os = "android"))]
#[expect(
    clippy::unnecessary_wraps,
    clippy::fn_params_excessive_bools,
    reason = "matches fallible Android plugin signature and immersive toggle"
)]
pub(crate) fn set_immersive(_app: &AppHandle, _enabled: bool) -> Result<(), AppError> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[expect(
    clippy::unnecessary_wraps,
    clippy::fn_params_excessive_bools,
    reason = "matches fallible Android plugin signature and keep-screen-on toggle"
)]
pub(crate) fn set_keep_screen_on(_app: &AppHandle, _enabled: bool) -> Result<(), AppError> {
    Ok(())
}

#[cfg(not(target_os = "android"))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "matches fallible Android plugin signature"
)]
pub(crate) fn check_notification_permission(
    _app: &AppHandle,
) -> Result<NotificationPermissionInfo, AppError> {
    Ok(NotificationPermissionInfo::default())
}

#[cfg(not(target_os = "android"))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "matches fallible Android plugin signature"
)]
pub(crate) fn request_notification_permission(
    _app: &AppHandle,
) -> Result<NotificationPermissionInfo, AppError> {
    Ok(NotificationPermissionInfo::default())
}

#[cfg(target_os = "windows")]
#[expect(
    clippy::unnecessary_wraps,
    reason = "matches fallible Android clipboard bridge signature"
)]
pub(crate) fn read_clipboard_text(_app: &AppHandle) -> Result<Option<String>, AppError> {
    Ok(read_windows_clipboard_text())
}

#[cfg(target_os = "windows")]
fn read_windows_clipboard_text() -> Option<String> {
    use std::ffi::c_void;

    const CF_UNICODETEXT: u32 = 13;
    const MAX_CLIPBOARD_UTF16_UNITS: usize = 16_384;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn OpenClipboard(hwnd_new_owner: *mut c_void) -> i32;
        fn CloseClipboard() -> i32;
        fn GetClipboardData(format: u32) -> *mut c_void;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalLock(hmem: *mut c_void) -> *const u16;
        fn GlobalUnlock(hmem: *mut c_void) -> i32;
    }

    struct ClipboardGuard;
    impl Drop for ClipboardGuard {
        fn drop(&mut self) {
            // SAFETY: ClipboardGuard is only created after OpenClipboard succeeds on this thread.
            unsafe {
                let _ = CloseClipboard();
            }
        }
    }

    struct GlobalLockGuard(*mut c_void);
    impl Drop for GlobalLockGuard {
        fn drop(&mut self) {
            // SAFETY: GlobalLockGuard wraps a non-null handle locked by GlobalLock on the open clipboard.
            unsafe {
                let _ = GlobalUnlock(self.0);
            }
        }
    }

    let mut opened = false;
    for _ in 0..5 {
        // SAFETY: Passing a null HWND associates the opened clipboard with the current task.
        let ok = unsafe { OpenClipboard(std::ptr::null_mut()) };
        if ok != 0 {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(12));
    }
    if !opened {
        return None;
    }
    let _clip_guard = ClipboardGuard;

    // SAFETY: Clipboard is open on the current thread; CF_UNICODETEXT yields a UTF-16 handle or null.
    let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
    if handle.is_null() {
        return None;
    }

    // SAFETY: `handle` is a valid non-null HGLOBAL returned by GetClipboardData while clipboard is open.
    let ptr = unsafe { GlobalLock(handle) };
    if ptr.is_null() {
        return None;
    }
    let _lock_guard = GlobalLockGuard(handle);

    let mut units = Vec::new();
    for offset in 0..MAX_CLIPBOARD_UTF16_UNITS {
        // SAFETY: `offset` walks sequentially within the NUL-terminated UTF-16 clipboard buffer.
        let elem_ptr = unsafe { ptr.add(offset) };
        // SAFETY: `elem_ptr` is valid and aligned while `_lock_guard` holds the HGLOBAL lock.
        let ch = unsafe { elem_ptr.read() };
        if ch == 0 {
            break;
        }
        units.push(ch);
    }

    let text = String::from_utf16_lossy(&units);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
#[expect(
    clippy::unnecessary_wraps,
    reason = "matches fallible Android clipboard bridge signature"
)]
pub(crate) fn read_clipboard_text(_app: &AppHandle) -> Result<Option<String>, AppError> {
    Ok(None)
}
