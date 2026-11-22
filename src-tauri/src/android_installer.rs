//! Native Android APK installer bridge backed by `ApkInstallerPlugin.kt`.

#[cfg(target_os = "android")]
mod imp {
    use std::path::Path;

    use serde::Deserialize;
    use tauri::plugin::{Builder as PluginBuilder, PluginHandle, TauriPlugin};
    use tauri::{AppHandle, Manager, Runtime};

    use crate::error::AppError;

    struct ApkInstallerState<R: Runtime>(PluginHandle<R>);

    #[derive(Debug, Deserialize)]
    struct InstallApkReply {
        status: String,
    }

    /// Registers the native `ApkInstallerPlugin` Kotlin class on Android.
    pub(crate) fn init_plugin<R: Runtime>() -> TauriPlugin<R> {
        PluginBuilder::new("apk_installer")
            .setup(|app, api| {
                let handle =
                    api.register_android_plugin("com.rtmaga.reader", "ApkInstallerPlugin")?;
                app.manage(ApkInstallerState(handle));
                Ok(())
            })
            .build()
    }

    /// Launches Android's system package installer for a verified APK inside the app cache dir.
    ///
    /// Returns `"launched"` when the installer opened or `"permission_required"` when Android's
    /// *Install unknown apps* settings screen was opened instead.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Internal`] if the native bridge call fails.
    pub(crate) fn install_apk(app: &AppHandle, path: &Path) -> Result<String, AppError> {
        let Some(path_str) = path.to_str() else {
            return Err(AppError::Internal {
                message: "invalid non-UTF8 APK path".to_owned(),
            });
        };
        let state = app.state::<ApkInstallerState<tauri::Wry>>();
        let reply: InstallApkReply = state
            .0
            .run_mobile_plugin("installApk", serde_json::json!({ "path": path_str }))
            .map_err(|e| AppError::Internal {
                message: format!("failed to launch Android APK installer: {e}"),
            })?;
        Ok(reply.status)
    }
}

#[cfg(target_os = "android")]
pub(crate) use imp::{init_plugin, install_apk};
