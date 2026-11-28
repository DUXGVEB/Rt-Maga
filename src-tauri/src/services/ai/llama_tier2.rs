//! Tier 2 optional `Qwen3-VL-2B-Instruct-IQ4_XS` multimodal VLM backend.
//!
//! Enforces strict memory-bounded runtime flags on every invocation:
//! - `n_ctx = 2048` (`-c 2048`)
//! - `flash_attn = true` (`-fa on`)
//! - KV-cache quantization `q8_0` (`-ctk q8_0 -ctv q8_0`)
//! - `use_mmap = true` (default memory-mapped weights; never passes `--no-mmap`)
//! - Stateless single-turn inference per page with Qwen3 thinking disabled
//! - Vulkan offload (`-ngl 99`) with automatic CPU fallback (`-ngl 0`).
//!
//! On Windows/desktop, keeps a local loopback `llama-server.exe` (`127.0.0.1:<ephemeral_port>`)
//! warm across pages while reading (avoiding reloading 1.45 GB per page), and kills it
//! immediately on toggle-off, tier switch, or reader exit.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_os = "android"))]
use std::time::{Duration, Instant};

#[cfg(not(target_os = "android"))]
use base64::Engine as _;
use tauri::AppHandle;
use tokio::sync::{Mutex, watch};

use crate::error::AppError;
use crate::services::ai::TranslatedRegionView;
use crate::services::ai::downloader::{asset_destination, is_tier2_installed};
#[cfg(not(target_os = "android"))]
use crate::services::ai::downloader::{win_llama_mtmd_cli_path, win_llama_server_path};
use crate::services::ai::engine::InferenceRequest;
use crate::services::ai::manifest::{TIER2_VLM_GGUF, TIER2_VLM_MMPROJ};
use crate::services::ai::preprocess::{
    PreprocessedPage, encode_jpeg_for_inference, parse_vlm_compact_json,
};

/// Enforced Tier 2 context window size (strictly capped for `< 1.8 GB` peak RAM).
#[cfg(any(not(target_os = "android"), test))]
pub(crate) const TIER2_N_CTX: u32 = 2048;

/// Enforced Tier 2 KV-cache quantization format (`q8_0`).
#[cfg(any(not(target_os = "android"), test))]
pub(crate) const TIER2_KV_CACHE_TYPE: &str = "q8_0";

/// Maximum generated tokens per page for compact JSON speech-bubble translations.
#[cfg(any(not(target_os = "android"), test))]
const TIER2_MAX_TOKENS: u32 = 768;

/// Maximum startup time waiting for local `llama-server.exe` `/health` endpoint.
#[cfg(not(target_os = "android"))]
const SERVER_HEALTH_TIMEOUT: Duration = Duration::from_secs(60);

/// GPU offload policy for Tier 2 `llama.cpp` sidecar execution.
#[cfg(any(not(target_os = "android"), test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuOffloadMode {
    /// Offload all layers to Vulkan GPU (`-ngl 99`).
    VulkanFull,
    /// CPU-only fallback (`-ngl 0`).
    CpuFallback,
}

#[cfg(not(target_os = "android"))]
struct RunningLlamaServer {
    child: tokio::process::Child,
    pid: Option<u32>,
    port: u16,
    model_path: PathBuf,
    mmproj_path: PathBuf,
}

#[cfg(not(target_os = "android"))]
impl std::fmt::Debug for RunningLlamaServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunningLlamaServer")
            .field("pid", &self.pid)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

/// Manages Tier 2 `Qwen3-VL-2B` execution and immediate process/JNI teardown.
#[derive(Debug)]
pub(crate) struct Tier2LlamaBackend {
    loaded: AtomicBool,
    #[cfg(not(target_os = "android"))]
    server: Mutex<Option<RunningLlamaServer>>,
    #[cfg(not(target_os = "android"))]
    active_cli_pid: Mutex<Option<u32>>,
    #[cfg(target_os = "android")]
    _android_marker: Mutex<()>,
}

impl Tier2LlamaBackend {
    /// Creates a new idle Tier 2 VLM backend (loads nothing until first inference).
    pub(crate) fn new() -> Self {
        Self {
            loaded: AtomicBool::new(false),
            #[cfg(not(target_os = "android"))]
            server: Mutex::new(None),
            #[cfg(not(target_os = "android"))]
            active_cli_pid: Mutex::new(None),
            #[cfg(target_os = "android")]
            _android_marker: Mutex::new(()),
        }
    }

    /// Returns `true` if a Tier 2 worker or native context is active in memory.
    pub(crate) fn is_loaded(&self) -> bool {
        self.loaded.load(Ordering::Relaxed)
    }

    /// Returns `true` if Tier 2 GGUF + mmproj (and Windows sidecar on Windows) exist on disk.
    pub(crate) fn is_installed(storage_root: &Path) -> bool {
        is_tier2_installed(storage_root)
    }

    /// Builds the strict compact JSON manga translation prompt for `target_lang`.
    pub(crate) fn build_page_prompt(target_lang: &str) -> String {
        let lang_name = language_display_name(target_lang);
        format!(
            "Read all manga speech bubbles and sound effects in this image. Translate every text region into natural {lang_name}. Output ONLY a valid compact JSON array with no extra text: [{{\"t\":\"translated text\",\"b\":[x1,y1,x2,y2]}}], where [x1,y1,x2,y2] are integer coordinates in 0..1000 relative to image width and height."
        )
    }

    /// Constructs exact `llama-server` CLI arguments enforcing memory, KV-cache, and loopback bounds.
    #[cfg(any(not(target_os = "android"), test))]
    pub(crate) fn build_server_args(
        model_path: &Path,
        mmproj_path: &Path,
        port: u16,
        offload: GpuOffloadMode,
    ) -> Vec<String> {
        let ngl = match offload {
            GpuOffloadMode::VulkanFull => "99",
            GpuOffloadMode::CpuFallback => "0",
        };
        vec![
            "-m".to_owned(),
            model_path.to_string_lossy().into_owned(),
            "--mmproj".to_owned(),
            mmproj_path.to_string_lossy().into_owned(),
            "-c".to_owned(),
            TIER2_N_CTX.to_string(),
            "-fa".to_owned(),
            "on".to_owned(),
            "-ctk".to_owned(),
            TIER2_KV_CACHE_TYPE.to_owned(),
            "-ctv".to_owned(),
            TIER2_KV_CACHE_TYPE.to_owned(),
            "-ngl".to_owned(),
            ngl.to_owned(),
            "--host".to_owned(),
            "127.0.0.1".to_owned(),
            "--port".to_owned(),
            port.to_string(),
            "-np".to_owned(),
            "1".to_owned(),
            "--no-webui".to_owned(),
            "--jinja".to_owned(),
        ]
    }

    /// Constructs exact `llama-mtmd-cli` CLI arguments (legacy fallback when `llama-server.exe` is absent).
    #[cfg(any(not(target_os = "android"), test))]
    pub(crate) fn build_cli_args(
        model_path: &Path,
        mmproj_path: &Path,
        image_path: &Path,
        prompt: &str,
        offload: GpuOffloadMode,
    ) -> Vec<String> {
        let ngl = match offload {
            GpuOffloadMode::VulkanFull => "99",
            GpuOffloadMode::CpuFallback => "0",
        };
        vec![
            "-m".to_owned(),
            model_path.to_string_lossy().into_owned(),
            "--mmproj".to_owned(),
            mmproj_path.to_string_lossy().into_owned(),
            "--image".to_owned(),
            image_path.to_string_lossy().into_owned(),
            "-c".to_owned(),
            TIER2_N_CTX.to_string(),
            "-fa".to_owned(),
            "on".to_owned(),
            "-ctk".to_owned(),
            TIER2_KV_CACHE_TYPE.to_owned(),
            "-ctv".to_owned(),
            TIER2_KV_CACHE_TYPE.to_owned(),
            "-ngl".to_owned(),
            ngl.to_owned(),
            "--no-warmup".to_owned(),
            "--temp".to_owned(),
            "0.1".to_owned(),
            "-n".to_owned(),
            TIER2_MAX_TOKENS.to_string(),
            "-p".to_owned(),
            prompt.to_owned(),
        ]
    }

    /// Executes stateless single-page multimodal translation on `req.page`.
    ///
    /// # Errors
    ///
    /// Returns [`AppError`] if Tier 2 weights/sidecar are missing or inference fails.
    pub(crate) async fn infer_page(
        &self,
        req: &InferenceRequest<'_>,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Result<Option<Vec<TranslatedRegionView>>, AppError> {
        let model_path = asset_destination(req.storage_root, &TIER2_VLM_GGUF);
        let mmproj_path = asset_destination(req.storage_root, &TIER2_VLM_MMPROJ);
        if !model_path.is_file() || !mmproj_path.is_file() {
            return Err(AppError::NotFound {
                message: "Qwen3-VL-2B model weights are not downloaded".to_owned(),
            });
        }

        if *cancel_rx.borrow() != expected_gen {
            return Ok(None);
        }

        let prompt = Self::build_page_prompt(req.target_lang);

        #[cfg(target_os = "android")]
        let raw_output = {
            let tmp_image = write_temp_inference_jpeg(req.temp_dir, req.page).await?;
            let handle = req.app.ok_or_else(|| AppError::Internal {
                message: "Android AppHandle unavailable for Tier 2 VLM".to_owned(),
            })?;
            self.loaded.store(true, Ordering::Relaxed);
            let h = handle.clone();
            let img_p = tmp_image.path().to_path_buf();
            let m_p = model_path.clone();
            let mm_p = mmproj_path.clone();
            let lang = req.target_lang.to_owned();
            let pr = prompt.clone();
            let reply = tokio::task::spawn_blocking(move || {
                crate::android_env::run_tier2_vlm(&h, &img_p, &m_p, &mm_p, &lang, &pr)
            })
            .await
            .map_err(|e| AppError::Internal {
                message: format!("Android Tier 2 VLM worker failed: {e}"),
            })??;
            drop(tmp_image);
            if *cancel_rx.borrow() != expected_gen {
                return Ok(None);
            }
            Some(reply.raw_json)
        };

        #[cfg(not(target_os = "android"))]
        let raw_output = {
            let server_path = win_llama_server_path(req.storage_root);
            if server_path.is_file() {
                self.infer_via_local_server(
                    &server_path,
                    &model_path,
                    &mmproj_path,
                    req.page,
                    &prompt,
                    req.offload,
                    cancel_rx,
                    expected_gen,
                )
                .await?
            } else {
                self.infer_via_cli_fallback(
                    req,
                    &model_path,
                    &mmproj_path,
                    &prompt,
                    cancel_rx,
                    expected_gen,
                )
                .await?
            }
        };

        Ok(raw_output.map(|raw| parse_vlm_compact_json(&raw)))
    }

    #[cfg(not(target_os = "android"))]
    async fn infer_via_cli_fallback(
        &self,
        req: &InferenceRequest<'_>,
        model_path: &Path,
        mmproj_path: &Path,
        prompt: &str,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Result<Option<String>, AppError> {
        let cli_path = win_llama_mtmd_cli_path(req.storage_root);
        if !cli_path.is_file() {
            return Err(AppError::NotFound {
                message: "Windows llama.cpp sidecar (llama-server.exe) is not provisioned"
                    .to_owned(),
            });
        }
        let tmp_image = write_temp_inference_jpeg(req.temp_dir, req.page).await?;
        let primary_mode = req.offload;
        let out = match self
            .run_cli_once(
                &cli_path,
                model_path,
                mmproj_path,
                tmp_image.path(),
                prompt,
                primary_mode,
                cancel_rx.clone(),
                expected_gen,
            )
            .await
        {
            Ok(out) => out,
            Err(_vk_err) if primary_mode == GpuOffloadMode::VulkanFull => {
                if *cancel_rx.borrow() != expected_gen {
                    return Ok(None);
                }
                self.run_cli_once(
                    &cli_path,
                    model_path,
                    mmproj_path,
                    tmp_image.path(),
                    prompt,
                    GpuOffloadMode::CpuFallback,
                    cancel_rx,
                    expected_gen,
                )
                .await?
            }
            Err(err) => return Err(err),
        };
        drop(tmp_image);
        Ok(out)
    }

    /// Aborts any running Tier 2 inference request immediately (keeps warm desktop server alive
    /// across page swipes; `hard_unload` kills the server process completely).
    pub(crate) async fn cancel_active(&self, app: Option<&AppHandle>) {
        #[cfg(not(target_os = "android"))]
        {
            let _ = app;
            let pid = self.active_cli_pid.lock().await.take();
            if let Some(pid) = pid {
                terminate_pid_best_effort(pid);
            }
        }
        #[cfg(target_os = "android")]
        if let Some(handle) = app {
            crate::android_env::cancel_active_ai_jobs(handle);
        }
    }

    /// Hard-unloads all Tier 2 native state and kills the persistent Windows `llama-server.exe`.
    pub(crate) async fn hard_unload(&self, app: Option<&AppHandle>) {
        self.cancel_active(app).await;
        #[cfg(not(target_os = "android"))]
        {
            let running = self.server.lock().await.take();
            if let Some(mut srv) = running {
                let _ = srv.child.kill().await;
                let _ = srv.child.wait().await;
                if let Some(pid) = srv.pid {
                    terminate_pid_best_effort(pid);
                }
            }
        }
        self.loaded.store(false, Ordering::Relaxed);
        #[cfg(target_os = "android")]
        if let Some(handle) = app {
            let _ = crate::android_env::hard_unload_all_ai(handle);
        }
    }

    #[cfg(not(target_os = "android"))]
    #[expect(
        clippy::too_many_arguments,
        reason = "orchestrates persistent loopback server readiness and multimodal chat completion"
    )]
    async fn infer_via_local_server(
        &self,
        server_path: &Path,
        model_path: &Path,
        mmproj_path: &Path,
        page: &PreprocessedPage,
        prompt: &str,
        primary_offload: GpuOffloadMode,
        mut cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Result<Option<String>, AppError> {
        let Some(port) = self
            .ensure_server_ready(
                server_path,
                model_path,
                mmproj_path,
                primary_offload,
                cancel_rx.clone(),
                expected_gen,
            )
            .await?
        else {
            return Ok(None);
        };

        let rgb_clone = page.rgb.clone();
        let jpeg_bytes = tokio::task::spawn_blocking(move || encode_jpeg_for_inference(&rgb_clone))
            .await
            .map_err(|e| AppError::Internal {
                message: format!("JPEG encode worker failed: {e}"),
            })??;
        if *cancel_rx.borrow() != expected_gen {
            return Ok(None);
        }

        let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
        let data_url = format!("data:image/jpeg;base64,{b64}");

        let body = serde_json::json!({
            "model": "qwen3-vl-2b",
            "temperature": 0.1,
            "max_tokens": TIER2_MAX_TOKENS,
            "chat_template_kwargs": { "enable_thinking": false },
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {
                            "type": "image_url",
                            "image_url": { "url": data_url }
                        },
                        {
                            "type": "text",
                            "text": prompt
                        }
                    ]
                }
            ]
        });

        let client = build_loopback_client()?;
        let endpoint = format!("http://127.0.0.1:{port}/v1/chat/completions");

        let resp_res = tokio::select! {
            res = client.post(&endpoint).json(&body).send() => Some(res),
            () = wait_for_cancel(&mut cancel_rx, expected_gen) => None,
        };
        let Some(resp_res) = resp_res else {
            return Ok(None);
        };
        let resp = resp_res.map_err(|e| AppError::Internal {
            message: format!("failed contacting local llama-server: {e}"),
        })?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let err_body = resp.text().await.unwrap_or_default();
            let snippet: String = err_body.trim().chars().take(240).collect();
            return Err(AppError::Internal {
                message: format!("local llama-server returned HTTP {status}: {snippet}"),
            });
        }

        let json_res = tokio::select! {
            res = resp.json::<serde_json::Value>() => Some(res),
            () = wait_for_cancel(&mut cancel_rx, expected_gen) => None,
        };
        let Some(json_res) = json_res else {
            return Ok(None);
        };
        let payload = json_res.map_err(|e| AppError::Internal {
            message: format!("failed parsing llama-server JSON response: {e}"),
        })?;

        let content = payload
            .pointer("/choices/0/message/content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        Ok(Some(content))
    }

    #[cfg(not(target_os = "android"))]
    async fn ensure_server_ready(
        &self,
        server_path: &Path,
        model_path: &Path,
        mmproj_path: &Path,
        primary_offload: GpuOffloadMode,
        cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Result<Option<u16>, AppError> {
        let mut guard = self.server.lock().await;
        if let Some(ref mut srv) = *guard {
            let alive = srv.child.try_wait().ok().flatten().is_none();
            if alive && srv.model_path == model_path && srv.mmproj_path == mmproj_path {
                return Ok(Some(srv.port));
            }
            let _ = srv.child.kill().await;
            let _ = srv.child.wait().await;
            if let Some(pid) = srv.pid {
                terminate_pid_best_effort(pid);
            }
            *guard = None;
            self.loaded.store(false, Ordering::Relaxed);
        }

        let modes: &[GpuOffloadMode] = if primary_offload == GpuOffloadMode::VulkanFull {
            &[GpuOffloadMode::VulkanFull, GpuOffloadMode::CpuFallback]
        } else {
            &[GpuOffloadMode::CpuFallback]
        };

        let mut last_err: Option<AppError> = None;
        for &mode in modes {
            if *cancel_rx.borrow() != expected_gen {
                return Ok(None);
            }
            match spawn_and_wait_server(
                server_path,
                model_path,
                mmproj_path,
                mode,
                cancel_rx.clone(),
                expected_gen,
            )
            .await
            {
                Ok(Some(srv)) => {
                    let port = srv.port;
                    *guard = Some(srv);
                    self.loaded.store(true, Ordering::Relaxed);
                    return Ok(Some(port));
                }
                Ok(None) => return Ok(None),
                Err(err) => {
                    last_err = Some(err);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| AppError::Internal {
            message: "failed to start local llama-server".to_owned(),
        }))
    }

    #[cfg(not(target_os = "android"))]
    #[expect(
        clippy::too_many_arguments,
        reason = "legacy CLI fallback runner takes model paths, prompt, offload mode, and cancellation token"
    )]
    async fn run_cli_once(
        &self,
        cli_path: &Path,
        model_path: &Path,
        mmproj_path: &Path,
        image_path: &Path,
        prompt: &str,
        offload: GpuOffloadMode,
        mut cancel_rx: watch::Receiver<u64>,
        expected_gen: u64,
    ) -> Result<Option<String>, AppError> {
        use std::process::Stdio;
        use tokio::io::AsyncReadExt;

        let args = Self::build_cli_args(model_path, mmproj_path, image_path, prompt, offload);
        let mut cmd = tokio::process::Command::new(cli_path);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(bin_dir) = cli_path.parent() {
            cmd.current_dir(bin_dir);
        }

        #[cfg(target_os = "windows")]
        {
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = cmd.spawn().map_err(|e| AppError::Internal {
            message: format!("failed to launch llama-mtmd-cli sidecar: {e}"),
        })?;
        self.loaded.store(true, Ordering::Relaxed);
        let child_pid = child.id();
        *self.active_cli_pid.lock().await = child_pid;

        let mut stdout_handle = child.stdout.take();
        let mut stderr_handle = child.stderr.take();

        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(ref mut out) = stdout_handle {
                let _ = out.read_to_end(&mut buf).await;
            }
            buf
        });
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(ref mut err) = stderr_handle {
                let _ = err.read_to_end(&mut buf).await;
            }
            buf
        });

        let status_res = tokio::select! {
            res = child.wait() => Some(res),
            () = wait_for_cancel(&mut cancel_rx, expected_gen) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                None
            }
        };

        *self.active_cli_pid.lock().await = None;
        self.loaded.store(false, Ordering::Relaxed);

        let Some(wait_res) = status_res else {
            stdout_task.abort();
            stderr_task.abort();
            return Ok(None);
        };

        let status = wait_res.map_err(|e| AppError::Internal {
            message: format!("llama-mtmd-cli process wait failed: {e}"),
        })?;
        let stdout_bytes = stdout_task.await.unwrap_or_default();
        let stderr_bytes = stderr_task.await.unwrap_or_default();

        if !status.success() {
            let err_tail = String::from_utf8_lossy(&stderr_bytes);
            let snippet: String = err_tail.trim().chars().take(240).collect();
            return Err(AppError::Internal {
                message: format!(
                    "llama-mtmd-cli exited with status {:?}: {snippet}",
                    status.code()
                ),
            });
        }

        Ok(Some(String::from_utf8_lossy(&stdout_bytes).into_owned()))
    }
}

#[cfg(not(target_os = "android"))]
async fn spawn_and_wait_server(
    server_path: &Path,
    model_path: &Path,
    mmproj_path: &Path,
    offload: GpuOffloadMode,
    mut cancel_rx: watch::Receiver<u64>,
    expected_gen: u64,
) -> Result<Option<RunningLlamaServer>, AppError> {
    use std::process::Stdio;

    let port = pick_ephemeral_loopback_port()?;
    let args = Tier2LlamaBackend::build_server_args(model_path, mmproj_path, port, offload);

    let mut cmd = tokio::process::Command::new(server_path);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(bin_dir) = server_path.parent() {
        cmd.current_dir(bin_dir);
    }
    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| AppError::Internal {
        message: format!("failed to launch llama-server sidecar: {e}"),
    })?;
    let pid = child.id();

    let client = build_loopback_client()?;
    let health_url = format!("http://127.0.0.1:{port}/health");
    let deadline = Instant::now() + SERVER_HEALTH_TIMEOUT;

    while Instant::now() < deadline {
        if *cancel_rx.borrow() != expected_gen {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Ok(None);
        }
        if let Ok(Some(exit_status)) = child.try_wait() {
            return Err(AppError::Internal {
                message: format!(
                    "llama-server exited prematurely during startup with status {:?}",
                    exit_status.code()
                ),
            });
        }
        if let Ok(resp) = client.get(&health_url).send().await
            && resp.status().is_success()
        {
            return Ok(Some(RunningLlamaServer {
                child,
                pid,
                port,
                model_path: model_path.to_path_buf(),
                mmproj_path: mmproj_path.to_path_buf(),
            }));
        }

        tokio::select! {
            () = tokio::time::sleep(Duration::from_millis(200)) => {}
            () = wait_for_cancel(&mut cancel_rx, expected_gen) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Ok(None);
            }
        }
    }

    let _ = child.kill().await;
    let _ = child.wait().await;
    Err(AppError::Internal {
        message: "timed out waiting for local llama-server to load Qwen3-VL-2B".to_owned(),
    })
}

#[cfg(not(target_os = "android"))]
fn pick_ephemeral_loopback_port() -> Result<u16, AppError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| AppError::Internal {
        message: format!("failed to bind ephemeral loopback port for llama-server: {e}"),
    })?;
    let port = listener
        .local_addr()
        .map_err(|e| AppError::Internal {
            message: format!("failed to query ephemeral loopback port: {e}"),
        })?
        .port();
    Ok(port)
}

#[cfg(not(target_os = "android"))]
fn build_loopback_client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| AppError::Internal {
            message: format!("failed to build loopback HTTP client: {e}"),
        })
}

#[cfg(not(target_os = "android"))]
async fn wait_for_cancel(cancel_rx: &mut watch::Receiver<u64>, expected_gen: u64) {
    loop {
        if *cancel_rx.borrow() != expected_gen {
            return;
        }
        if cancel_rx.changed().await.is_err() {
            return;
        }
    }
}

/// RAII temporary JPEG file guard that always unlinks on drop.
pub(crate) struct TempInferenceFile {
    path: PathBuf,
}

impl TempInferenceFile {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempInferenceFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Encodes `page.rgb` as JPEG and writes it into `<temp_dir>/page-<rand>.jpg` guarded by RAII drop.
///
/// # Errors
///
/// Returns [`AppError::Internal`] if creating the temp directory or writing the image fails.
pub(crate) async fn write_temp_inference_jpeg(
    temp_dir: &Path,
    page: &PreprocessedPage,
) -> Result<TempInferenceFile, AppError> {
    tokio::fs::create_dir_all(temp_dir)
        .await
        .map_err(|e| AppError::Internal {
            message: format!(
                "failed to prepare AI temp dir '{}': {e}",
                temp_dir.display()
            ),
        })?;
    let rgb_clone = page.rgb.clone();
    let jpeg_bytes = tokio::task::spawn_blocking(move || encode_jpeg_for_inference(&rgb_clone))
        .await
        .map_err(|e| AppError::Internal {
            message: format!("JPEG encode worker failed: {e}"),
        })??;

    let nonce: u64 = rand::random();
    let path = temp_dir.join(format!("page-{nonce:016x}.jpg"));
    tokio::fs::write(&path, &jpeg_bytes)
        .await
        .map_err(|e| AppError::Internal {
            message: format!("failed to write AI temp page image: {e}"),
        })?;
    Ok(TempInferenceFile { path })
}

#[cfg(target_os = "windows")]
fn terminate_pid_best_effort(pid: u32) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let _ = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F", "/T"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(all(not(target_os = "android"), not(target_os = "windows")))]
fn terminate_pid_best_effort(_pid: u32) {}

fn language_display_name(code: &str) -> &'static str {
    match code.trim().to_ascii_lowercase().as_str() {
        "vi" => "Vietnamese",
        "zh" => "Simplified Chinese",
        "ja" => "Japanese",
        "ru" => "Russian",
        "ko" => "Korean",
        "th" => "Thai",
        _ => "English",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_and_cli_args_enforce_exact_memory_and_kv_quantization_flags() {
        let model = Path::new("/models/Qwen3-VL-2B-Instruct-IQ4_XS.gguf");
        let mmproj = Path::new("/models/mmproj-Qwen3VL-2B-Instruct-Q8_0.gguf");
        let img = Path::new("/tmp/page.jpg");
        let prompt = Tier2LlamaBackend::build_page_prompt("vi");

        let srv_args =
            Tier2LlamaBackend::build_server_args(model, mmproj, 18080, GpuOffloadMode::VulkanFull);
        assert!(
            srv_args.windows(2).any(|w| w == ["-c", "2048"]),
            "server must enforce n_ctx=2048"
        );
        assert!(
            srv_args.windows(2).any(|w| w == ["-fa", "on"]),
            "server must enforce flash_attn=on"
        );
        assert!(
            srv_args.windows(2).any(|w| w == ["-ctk", "q8_0"]),
            "server must enforce KV-cache K=q8_0"
        );
        assert!(
            srv_args.windows(2).any(|w| w == ["-ctv", "q8_0"]),
            "server must enforce KV-cache V=q8_0"
        );
        assert!(
            srv_args.windows(2).any(|w| w == ["--host", "127.0.0.1"]),
            "server must bind loopback only"
        );
        assert!(
            srv_args.iter().any(|a| a == "--no-webui"),
            "server must disable web UI"
        );

        let vk_args = Tier2LlamaBackend::build_cli_args(
            model,
            mmproj,
            img,
            &prompt,
            GpuOffloadMode::VulkanFull,
        );
        assert!(
            vk_args.windows(2).any(|w| w == ["-ngl", "99"]),
            "Vulkan mode must pass -ngl 99"
        );
        assert!(
            !vk_args.iter().any(|a| a == "--no-mmap"),
            "use_mmap=true must stay enabled (never --no-mmap)"
        );

        let cpu_args = Tier2LlamaBackend::build_cli_args(
            model,
            mmproj,
            img,
            &prompt,
            GpuOffloadMode::CpuFallback,
        );
        assert!(
            cpu_args.windows(2).any(|w| w == ["-ngl", "0"]),
            "CPU fallback must pass -ngl 0"
        );
    }
}
