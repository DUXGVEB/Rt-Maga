//! Serverless peer-to-peer LAN synchronization engine for `RtMaga`.
//!
//! Supports two platform-tailored flows over local Wi-Fi/LAN with zero backend:
//! - **PC → Phone:** Ephemeral LAN HTTP endpoint + SVG QR code (with compact code fallback).
//! - **Phone → PC:** Ephemeral LAN HTTP endpoint + 6-digit Pairing Code + UDP LAN discovery
//!   (with manual `IP:Port` fallback for routers with AP isolation).
//!
//! Security guarantees:
//! - 5-minute strict session TTL with automatic shutdown & memory wipe.
//! - Random single-use 6-digit PIN + 256-bit session token (never reused).
//! - Max 5 failed pairing attempts (immediate termination) + 3-second cooldown per failure.
//! - LAN-only peer IP verification (public Internet IPs rejected at TCP accept).
//! - Mutual confirmation on BOTH devices showing peer Device Name, LAN IP, and manga count.
//! - Sync-only data scope (manga metadata, chapters, reading history; zero remote control).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use qrcode::QrCode;
use qrcode::render::svg;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

use crate::error::AppError;
use crate::repo::now_unix_secs;
use crate::repo::sync::{SyncBundle, SyncMergeReport};
use crate::state::AppState;

/// Strict session expiration duration (5 minutes).
const SESSION_TTL: Duration = Duration::from_secs(300);

/// Maximum failed pairing attempts before terminating the session immediately.
const MAX_FAILED_ATTEMPTS: u8 = 5;

/// Cooldown enforced after every failed pairing attempt.
const FAILED_ATTEMPT_COOLDOWN: Duration = Duration::from_secs(3);

/// UDP LAN discovery port used when PC searches for a Phone's 6-digit pairing code.
const LAN_DISCOVERY_PORT: u16 = 47_621;

/// Maximum incoming HTTP request payload accepted by the ephemeral sync endpoint (16 MiB).
const MAX_HTTP_REQ_BYTES: usize = 16 * 1024 * 1024;

/// Maximum incoming sync bundle size accepted by the receiver (32 MiB).
const MAX_SYNC_BUNDLE_BYTES: usize = 32 * 1024 * 1024;

/// Prefix for compact base64url QR/pairing codes.
const QR_CODE_PREFIX: &str = "rtmaga-sync:";

/// Mode selected by the Sender when opening a 5-minute LAN sync session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum SyncSenderMode {
    /// PC hosts an ephemeral LAN server and renders a QR code for the phone to scan.
    PcQr,
    /// Phone hosts an ephemeral LAN server + UDP LAN beacon and displays a 6-digit PIN.
    PhonePin,
}

/// Explicit user approval or rejection during mutual LAN sync confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum SyncDecision {
    Approve,
    Reject,
}

/// High-level phase reported to the Settings Sync UI via `sync://state-changed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub(crate) enum SyncPhase {
    #[default]
    Idle,
    HostingWaiting,
    HostConfirmPeer,
    HostWaitingPull,
    ReceiverConfirmPeer,
    ReceiverWaitingHost,
    Transferring,
    Completed,
    Expired,
    LockedOut,
    Cancelled,
    Error,
}

/// Snapshot of the current LAN sync session rendered by the Svelte Settings UI.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SyncStatusView {
    pub phase: SyncPhase,
    pub mode: Option<SyncSenderMode>,
    pub local_device_name: String,
    pub local_ip: Option<String>,
    pub local_port: Option<u16>,
    pub pairing_code: Option<String>,
    pub qr_svg: Option<String>,
    pub compact_code: Option<String>,
    pub expires_at_unix: Option<i64>,
    pub manga_count: usize,
    pub peer_name: Option<String>,
    pub peer_ip: Option<String>,
    pub failed_attempts: u8,
    pub max_attempts: u8,
    pub report: Option<SyncMergeReport>,
    pub message: Option<String>,
}

/// Compact QR payload encoded into `rtmaga-sync:<base64url(json)>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QrPayload {
    v: u8,
    ip: String,
    p: u16,
    c: String,
    t: String,
    d: String,
}

#[derive(Debug, Deserialize)]
struct PairRequestBody {
    pin: String,
    #[serde(default)]
    token: Option<String>,
    device_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairResponseBody {
    peer_name: String,
    peer_ip: String,
    manga_count: usize,
    transfer_ticket: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusResponseBody {
    decision: String, // "pending" | "approved" | "rejected"
}

#[derive(Debug, Deserialize)]
struct TicketBody {
    ticket: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct UdpBeaconReply {
    ip: String,
    port: u16,
    token: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HostDecision {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug)]
struct ActiveHostSession {
    mode: SyncSenderMode,
    pin: String,
    token: String,
    local_ip: String,
    local_port: u16,
    local_name: String,
    qr_svg: Option<String>,
    compact_code: Option<String>,
    expires_at: Instant,
    expires_at_unix: i64,
    bundle: Option<SyncBundle>,
    manga_count: usize,
    failed_attempts: u8,
    cooldown_until: Option<Instant>,
    peer_name: Option<String>,
    peer_ip: Option<String>,
    transfer_ticket: Option<String>,
    decision: HostDecision,
    tasks: Vec<JoinHandle<()>>,
}

#[derive(Debug, Clone)]
struct ActiveReceiverSession {
    base_url: String,
    peer_name: String,
    peer_ip: String,
    manga_count: usize,
    transfer_ticket: String,
    expires_at: Instant,
    expires_at_unix: i64,
    cancelled: Arc<AtomicBool>,
}

/// Thread-safe manager held inside [`AppState`] coordinating at most one active sync session.
#[derive(Debug, Default)]
pub(crate) struct SyncManager {
    inner: Mutex<SyncManagerInner>,
}

#[derive(Debug, Default)]
struct SyncManagerInner {
    host: Option<ActiveHostSession>,
    receiver: Option<ActiveReceiverSession>,
    last_terminal: Option<SyncStatusView>,
}

impl SyncManager {
    /// Creates an idle [`SyncManager`].
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns the current UI snapshot of the sync session.
    pub(crate) async fn status(&self) -> SyncStatusView {
        let mut guard = self.inner.lock().await;
        if guard
            .host
            .as_ref()
            .is_some_and(|h| Instant::now() >= h.expires_at)
            || guard
                .receiver
                .as_ref()
                .is_some_and(|r| Instant::now() >= r.expires_at)
        {
            wipe_sessions(&mut guard);
            let expired = SyncStatusView {
                phase: SyncPhase::Expired,
                local_device_name: local_device_name(),
                max_attempts: MAX_FAILED_ATTEMPTS,
                message: Some("Sync session expired after 5 minutes.".to_owned()),
                ..SyncStatusView::default()
            };
            guard.last_terminal = Some(expired.clone());
            return expired;
        }
        snapshot_from_inner(&guard)
    }

    /// Aborts any active host/receiver session and scrubs tokens & payloads from RAM.
    pub(crate) async fn cancel(&self, app: &AppHandle) -> SyncStatusView {
        let (view, pending_reject) = {
            let mut guard = self.inner.lock().await;
            let reject_target = guard
                .receiver
                .as_ref()
                .map(|r| (r.base_url.clone(), r.transfer_ticket.clone()));
            wipe_sessions(&mut guard);
            let idle = SyncStatusView {
                phase: SyncPhase::Idle,
                local_device_name: local_device_name(),
                max_attempts: MAX_FAILED_ATTEMPTS,
                ..SyncStatusView::default()
            };
            guard.last_terminal = None;
            (idle, reject_target)
        };
        if let Some((base_url, ticket)) = pending_reject
            && !ticket.is_empty()
            && let Ok(client) = build_lan_http_client(Duration::from_secs(3))
        {
            tokio::spawn(async move {
                let _ = client
                    .post(format!("{base_url}/sync/reject"))
                    .json(&serde_json::json!({ "ticket": ticket }))
                    .send()
                    .await;
            });
        }
        let _ = app.emit("sync://state-changed", &view);
        view
    }
}

/// Starts a 5-minute ephemeral LAN sender session (`PcQr` on Windows or `PhonePin` on Android).
///
/// # Errors
///
/// Returns [`AppError`] if the database cannot export history, no LAN interface is available,
/// or binding the ephemeral TCP socket fails.
pub(crate) async fn start_sender(
    app: &AppHandle,
    state: &Arc<AppState>,
    mode: SyncSenderMode,
) -> Result<SyncStatusView, AppError> {
    let device_name = resolve_local_device_name(app, state).await;
    let bundle = state.db.export_sync_bundle(&device_name).await?;
    let manga_count = bundle.manga.len();

    let local_ip = detect_lan_ipv4().ok_or_else(|| AppError::Network {
        message: "no active local LAN/Wi-Fi network interface detected".to_owned(),
    })?;

    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .map_err(|e| AppError::Network {
            message: format!("failed to open LAN sync listener: {e}"),
        })?;
    let local_port = listener
        .local_addr()
        .map_err(|e| AppError::Network {
            message: format!("failed to inspect sync listener port: {e}"),
        })?
        .port();

    let pin = generate_six_digit_pin();
    let token = generate_hex_token();
    let expires_at = Instant::now() + SESSION_TTL;
    let expires_at_unix = now_unix_secs() + i64::try_from(SESSION_TTL.as_secs()).unwrap_or(300);

    let (qr_svg, compact_code) = if mode == SyncSenderMode::PcQr {
        let payload = QrPayload {
            v: 1,
            ip: local_ip.to_string(),
            p: local_port,
            c: pin.clone(),
            t: token.clone(),
            d: device_name.clone(),
        };
        let compact = encode_qr_payload(&payload)?;
        let svg_str = render_qr_svg(&compact)?;
        (Some(svg_str), Some(compact))
    } else {
        (None, None)
    };

    let mut tasks = Vec::new();

    // Spawn HTTP accept loop.
    let app_http = app.clone();
    let state_http = Arc::clone(state);
    tasks.push(tokio::spawn(async move {
        run_host_http_loop(listener, app_http, state_http).await;
    }));

    // Spawn UDP discovery responder when hosting a 6-digit Pairing Code (Phone -> PC).
    if mode == SyncSenderMode::PhonePin {
        let pin_hash = sha256_hex(pin.as_bytes());
        let reply_json = serde_json::to_string(&UdpBeaconReply {
            ip: local_ip.to_string(),
            port: local_port,
            token: token.clone(),
        })
        .unwrap_or_default();
        tasks.push(tokio::spawn(async move {
            run_udp_discovery_responder(pin_hash, reply_json).await;
        }));
    }

    // Spawn strict 5-minute auto-expiry cleanup task.
    let app_ttl = app.clone();
    let state_ttl = Arc::clone(state);
    let expected_token = token.clone();
    tasks.push(tokio::spawn(async move {
        tokio::time::sleep(SESSION_TTL).await;
        expire_host_session_if_matching(&app_ttl, &state_ttl, &expected_token).await;
    }));

    let view = {
        let mut guard = state.sync.inner.lock().await;
        wipe_sessions(&mut guard);
        guard.last_terminal = None;
        guard.host = Some(ActiveHostSession {
            mode,
            pin,
            token,
            local_ip: local_ip.to_string(),
            local_port,
            local_name: device_name,
            qr_svg,
            compact_code,
            expires_at,
            expires_at_unix,
            bundle: Some(bundle),
            manga_count,
            failed_attempts: 0,
            cooldown_until: None,
            peer_name: None,
            peer_ip: None,
            transfer_ticket: None,
            decision: HostDecision::Pending,
            tasks,
        });
        snapshot_from_inner(&guard)
    };

    let _ = app.emit("sync://state-changed", &view);
    info!(ip = %local_ip, port = local_port, ?mode, manga_count, "started 5-minute LAN sync sender");
    Ok(view)
}

/// Called on the Sender device when the user clicks Approve or Reject on the peer confirmation prompt.
///
/// # Errors
///
/// Returns [`AppError::NotFound`] if no host session is awaiting confirmation.
pub(crate) async fn host_respond(
    app: &AppHandle,
    state: &AppState,
    decision: SyncDecision,
) -> Result<SyncStatusView, AppError> {
    let view = {
        let mut guard = state.sync.inner.lock().await;
        let Some(host) = guard.host.as_mut() else {
            return Err(AppError::NotFound {
                message: "no active sync session".to_owned(),
            });
        };

        if decision == SyncDecision::Approve {
            host.decision = HostDecision::Approved;
            snapshot_from_inner(&guard)
        } else {
            host.decision = HostDecision::Rejected;
            let cancelled = SyncStatusView {
                phase: SyncPhase::Cancelled,
                local_device_name: host.local_name.clone(),
                max_attempts: MAX_FAILED_ATTEMPTS,
                message: Some("Transfer declined by sender.".to_owned()),
                ..SyncStatusView::default()
            };
            wipe_sessions(&mut guard);
            guard.last_terminal = Some(cancelled.clone());
            cancelled
        }
    };

    let _ = app.emit("sync://state-changed", &view);
    Ok(view)
}

/// Connects from a Receiver (Phone) to a PC sender using a scanned QR / compact pairing payload.
///
/// # Errors
///
/// Returns [`AppError`] on malformed QR payload, non-LAN IP, wrong PIN/token, cooldown, or network error.
pub(crate) async fn connect_via_qr(
    app: &AppHandle,
    state: &AppState,
    raw_qr: &str,
) -> Result<SyncStatusView, AppError> {
    let payload = decode_qr_payload(raw_qr)?;
    let ip: IpAddr = payload.ip.parse().map_err(|_| AppError::InvalidUrl {
        message: "invalid LAN IP in QR code".to_owned(),
    })?;
    if !is_allowed_sync_peer_ip(ip) {
        return Err(AppError::InvalidUrl {
            message: "sync is only permitted on local LAN networks".to_owned(),
        });
    }

    let base_url = format_http_origin(ip, payload.p);
    pair_as_receiver(app, state, base_url, payload.c, Some(payload.t)).await
}

/// Connects from a Receiver (PC) to a Phone sender using its 6-digit Pairing Code, discovering
/// the phone over UDP LAN broadcast or connecting directly via optional `manual_addr` (`IP:Port`).
///
/// # Errors
///
/// Returns [`AppError`] if the PIN format is invalid, device cannot be found on LAN, or pairing fails.
pub(crate) async fn connect_via_pin(
    app: &AppHandle,
    state: &AppState,
    raw_pin: &str,
    manual_addr: Option<&str>,
) -> Result<SyncStatusView, AppError> {
    let pin: String = raw_pin.chars().filter(char::is_ascii_digit).collect();
    if pin.len() != 6 {
        return Err(AppError::InvalidUrl {
            message: "pairing code must be exactly 6 digits".to_owned(),
        });
    }

    let (ip, port, discovered_token) =
        if let Some(addr_raw) = manual_addr.map(str::trim).filter(|s| !s.is_empty()) {
            let sock: SocketAddr = addr_raw.parse().map_err(|_| AppError::InvalidUrl {
                message: "invalid manual LAN address (expected IP:Port)".to_owned(),
            })?;
            if !is_allowed_sync_peer_ip(sock.ip()) {
                return Err(AppError::InvalidUrl {
                    message: "manual address must be a local LAN IP".to_owned(),
                });
            }
            (sock.ip(), sock.port(), None)
        } else {
            let reply = discover_phone_on_lan(&pin).await?;
            let ip: IpAddr = reply.ip.parse().map_err(|_| AppError::Network {
                message: "discovered device returned an invalid IP".to_owned(),
            })?;
            if !is_allowed_sync_peer_ip(ip) {
                return Err(AppError::InvalidUrl {
                    message: "discovered address is not on local LAN".to_owned(),
                });
            }
            (ip, reply.port, Some(reply.token))
        };

    let base_url = format_http_origin(ip, port);
    pair_as_receiver(app, state, base_url, pin, discovered_token).await
}

/// Called on the Receiver device after inspecting the Sender's Device Name & LAN IP.
///
/// When `decision` is [`SyncDecision::Approve`], waits for the Sender's approval (if still pending),
/// pulls the [`SyncBundle`], merges it into SQLite, and returns the completed [`SyncStatusView`].
///
/// # Errors
///
/// Returns [`AppError`] if no receiver session is active, Sender rejects/expires, or merge fails.
pub(crate) async fn receiver_confirm(
    app: &AppHandle,
    state: &AppState,
    decision: SyncDecision,
) -> Result<SyncStatusView, AppError> {
    let session = {
        let mut guard = state.sync.inner.lock().await;
        let Some(rec) = guard.receiver.clone() else {
            return Err(AppError::NotFound {
                message: "no pending sync transfer to confirm".to_owned(),
            });
        };
        if decision == SyncDecision::Reject {
            wipe_sessions(&mut guard);
        }
        rec
    };

    if Instant::now() >= session.expires_at {
        return Ok(record_terminal(
            app,
            state,
            SyncPhase::Expired,
            "Sync session expired after 5 minutes.",
        )
        .await);
    }

    let client = build_lan_http_client(Duration::from_secs(15))?;

    if decision == SyncDecision::Reject {
        let _ = client
            .post(format!("{}/sync/reject", session.base_url))
            .json(&serde_json::json!({ "ticket": session.transfer_ticket }))
            .send()
            .await;

        return Ok(record_terminal(app, state, SyncPhase::Cancelled, "Transfer declined.").await);
    }

    // Show "Waiting for sender confirmation..." state.
    let waiting_view = SyncStatusView {
        phase: SyncPhase::ReceiverWaitingHost,
        local_device_name: local_device_name(),
        peer_name: Some(session.peer_name.clone()),
        peer_ip: Some(session.peer_ip.clone()),
        manga_count: session.manga_count,
        expires_at_unix: Some(session.expires_at_unix),
        max_attempts: MAX_FAILED_ATTEMPTS,
        ..SyncStatusView::default()
    };
    let _ = app.emit("sync://state-changed", &waiting_view);

    match poll_host_approval(&client, &session).await {
        Ok(true) => {}
        Ok(false) => {
            if session.cancelled.load(Ordering::Acquire) {
                return Ok(state.sync.status().await);
            }
            return Ok(record_terminal(
                app,
                state,
                SyncPhase::Cancelled,
                "Sender declined or ended the transfer.",
            )
            .await);
        }
        Err(err) => {
            if session.cancelled.load(Ordering::Acquire) {
                return Ok(state.sync.status().await);
            }
            let _ = record_terminal(app, state, SyncPhase::Error, &err.to_string()).await;
            return Err(err);
        }
    }

    match pull_and_merge_bundle(app, state, &client, session.clone()).await {
        Ok(done) => Ok(done),
        Err(err) => {
            if session.cancelled.load(Ordering::Acquire) {
                return Ok(state.sync.status().await);
            }
            let _ = record_terminal(app, state, SyncPhase::Error, &err.to_string()).await;
            Err(err)
        }
    }
}

async fn record_terminal(
    app: &AppHandle,
    state: &AppState,
    phase: SyncPhase,
    message: &str,
) -> SyncStatusView {
    let view = SyncStatusView {
        phase,
        local_device_name: local_device_name(),
        max_attempts: MAX_FAILED_ATTEMPTS,
        message: Some(message.to_owned()),
        ..SyncStatusView::default()
    };
    {
        let mut guard = state.sync.inner.lock().await;
        wipe_sessions(&mut guard);
        guard.last_terminal = Some(view.clone());
    }
    let _ = app.emit("sync://state-changed", &view);
    view
}

/// Polls the host until it returns `"approved"` (`Ok(true)`) or `"rejected"` (`Ok(false)`).
async fn poll_host_approval(
    client: &reqwest::Client,
    session: &ActiveReceiverSession,
) -> Result<bool, AppError> {
    loop {
        if session.cancelled.load(Ordering::Acquire) {
            return Ok(false);
        }
        if Instant::now() >= session.expires_at {
            return Err(AppError::Network {
                message: "sync session expired while waiting for sender confirmation".to_owned(),
            });
        }

        let resp = client
            .get(format!(
                "{}/sync/status?ticket={}",
                session.base_url, session.transfer_ticket
            ))
            .send()
            .await
            .map_err(|e| AppError::Network {
                message: format!("lost connection to sender device: {e}"),
            })?;

        if resp.status() == reqwest::StatusCode::GONE
            || resp.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Ok(false);
        }
        if !resp.status().is_success() {
            return Err(AppError::Network {
                message: "sender ended or rejected the sync session".to_owned(),
            });
        }

        let status: StatusResponseBody = resp.json().await.map_err(|e| AppError::Network {
            message: format!("invalid sync status response: {e}"),
        })?;

        match status.decision.as_str() {
            "approved" => return Ok(true),
            "rejected" => return Ok(false),
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}

async fn pull_and_merge_bundle(
    app: &AppHandle,
    state: &AppState,
    client: &reqwest::Client,
    session: ActiveReceiverSession,
) -> Result<SyncStatusView, AppError> {
    if session.cancelled.load(Ordering::Acquire) {
        return Err(AppError::NotFound {
            message: "sync session was cancelled".to_owned(),
        });
    }
    let transferring = SyncStatusView {
        phase: SyncPhase::Transferring,
        local_device_name: local_device_name(),
        peer_name: Some(session.peer_name.clone()),
        peer_ip: Some(session.peer_ip.clone()),
        manga_count: session.manga_count,
        max_attempts: MAX_FAILED_ATTEMPTS,
        ..SyncStatusView::default()
    };
    let _ = app.emit("sync://state-changed", &transferring);

    let pull_resp = client
        .post(format!("{}/sync/pull", session.base_url))
        .json(&serde_json::json!({ "ticket": session.transfer_ticket }))
        .send()
        .await
        .map_err(|e| AppError::Network {
            message: format!("failed to download sync bundle: {e}"),
        })?;

    if !pull_resp.status().is_success() {
        return Err(AppError::Network {
            message: format!("sync pull rejected (HTTP {})", pull_resp.status().as_u16()),
        });
    }

    let bytes = pull_resp.bytes().await.map_err(|e| AppError::Network {
        message: format!("failed to read sync bundle: {e}"),
    })?;
    if bytes.len() > MAX_SYNC_BUNDLE_BYTES {
        return Err(AppError::Internal {
            message: "incoming sync bundle exceeds size limit".to_owned(),
        });
    }

    let bundle: SyncBundle = serde_json::from_slice(&bytes).map_err(|e| AppError::Internal {
        message: format!("invalid sync bundle format: {e}"),
    })?;

    if session.cancelled.load(Ordering::Acquire) {
        return Err(AppError::NotFound {
            message: "sync session was cancelled".to_owned(),
        });
    }

    let report = state.db.merge_sync_bundle(&bundle).await?;
    let done = SyncStatusView {
        phase: SyncPhase::Completed,
        local_device_name: local_device_name(),
        peer_name: Some(session.peer_name),
        peer_ip: Some(session.peer_ip),
        manga_count: report.total_received,
        max_attempts: MAX_FAILED_ATTEMPTS,
        report: Some(report),
        ..SyncStatusView::default()
    };

    {
        let mut guard = state.sync.inner.lock().await;
        wipe_sessions(&mut guard);
        guard.last_terminal = Some(done.clone());
    }
    let _ = app.emit("sync://state-changed", &done);
    Ok(done)
}

async fn pair_as_receiver(
    app: &AppHandle,
    state: &AppState,
    base_url: String,
    pin: String,
    token: Option<String>,
) -> Result<SyncStatusView, AppError> {
    let local_name = resolve_local_device_name(app, state).await;
    let client = build_lan_http_client(Duration::from_secs(10))?;
    let resp = client
        .post(format!("{base_url}/sync/pair"))
        .json(&serde_json::json!({
            "pin": pin,
            "token": token,
            "device_name": local_name,
        }))
        .send()
        .await
        .map_err(|e| AppError::Network {
            message: format!("could not connect to peer over LAN: {e}"),
        })?;

    let status = resp.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(AppError::Network {
            message: "please wait 3 seconds before trying another pairing code".to_owned(),
        });
    }
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::GONE {
        let msg = resp.text().await.unwrap_or_default();
        return Err(AppError::InvalidUrl {
            message: if msg.trim().is_empty() {
                "invalid or expired pairing code".to_owned()
            } else {
                msg
            },
        });
    }
    if !status.is_success() {
        return Err(AppError::Network {
            message: format!("pairing failed (HTTP {})", status.as_u16()),
        });
    }

    let body: PairResponseBody = resp.json().await.map_err(|e| AppError::Network {
        message: format!("invalid pairing response: {e}"),
    })?;

    let expires_at = Instant::now() + SESSION_TTL;
    let expires_at_unix = now_unix_secs() + i64::try_from(SESSION_TTL.as_secs()).unwrap_or(300);

    let view = {
        let mut guard = state.sync.inner.lock().await;
        wipe_sessions(&mut guard);
        guard.last_terminal = None;
        guard.receiver = Some(ActiveReceiverSession {
            base_url,
            peer_name: body.peer_name,
            peer_ip: body.peer_ip,
            manga_count: body.manga_count,
            transfer_ticket: body.transfer_ticket,
            expires_at,
            expires_at_unix,
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        snapshot_from_inner(&guard)
    };

    let _ = app.emit("sync://state-changed", &view);
    Ok(view)
}

async fn run_host_http_loop(listener: TcpListener, app: AppHandle, state: Arc<AppState>) {
    loop {
        let Ok((stream, peer_addr)) = listener.accept().await else {
            break;
        };
        if !is_allowed_sync_peer_ip(peer_addr.ip()) {
            warn!(ip = %peer_addr.ip(), "rejected non-LAN sync connection attempt");
            drop(stream);
            continue;
        }
        let app_c = app.clone();
        let state_c = Arc::clone(&state);
        tokio::spawn(async move {
            handle_host_connection(stream, peer_addr, app_c, state_c).await;
        });
    }
}

async fn handle_host_connection(
    mut stream: TcpStream,
    peer_addr: SocketAddr,
    app: AppHandle,
    state: Arc<AppState>,
) {
    let Ok(Ok(req)) =
        tokio::time::timeout(Duration::from_secs(15), read_http_request(&mut stream)).await
    else {
        return;
    };

    let (status, content_type, body) = match (req.method.as_str(), req.path_only()) {
        ("POST", "/sync/pair") => handle_pair_route(&app, &state, peer_addr.ip(), &req.body).await,
        ("GET", "/sync/status") => handle_status_route(&state, req.query_param("ticket")).await,
        ("POST", "/sync/pull") => handle_pull_route(&app, &state, &req.body).await,
        ("POST", "/sync/reject") => handle_reject_route(&app, &state, &req.body).await,
        _ => (404, "text/plain", b"not found".to_vec()),
    };

    let _ = write_http_response(&mut stream, status, content_type, &body).await;
}

async fn handle_pair_route(
    app: &AppHandle,
    state: &AppState,
    peer_ip: IpAddr,
    body: &[u8],
) -> (u16, &'static str, Vec<u8>) {
    let Ok(req) = serde_json::from_slice::<PairRequestBody>(body) else {
        return (400, "text/plain", b"invalid json payload".to_vec());
    };

    let mut guard = state.sync.inner.lock().await;
    let Some(host) = guard.host.as_mut() else {
        return (410, "text/plain", b"sync session expired".to_vec());
    };

    if Instant::now() >= host.expires_at {
        wipe_sessions(&mut guard);
        return (410, "text/plain", b"sync session expired".to_vec());
    }

    if let Some(until) = host.cooldown_until
        && Instant::now() < until
    {
        return (
            429,
            "text/plain",
            b"cooldown active; wait 3 seconds".to_vec(),
        );
    }

    // Only one peer may pair with a single-use session.
    if host.transfer_ticket.is_some() {
        return (409, "text/plain", b"session already paired".to_vec());
    }

    let pin_ok = constant_time_eq(req.pin.trim().as_bytes(), host.pin.as_bytes());
    let token_ok = match host.mode {
        SyncSenderMode::PcQr => req
            .token
            .as_deref()
            .is_some_and(|t| constant_time_eq(t.trim().as_bytes(), host.token.as_bytes())),
        SyncSenderMode::PhonePin => req
            .token
            .as_deref()
            .is_none_or(|t| constant_time_eq(t.trim().as_bytes(), host.token.as_bytes())),
    };

    if !pin_ok || !token_ok {
        host.failed_attempts = host.failed_attempts.saturating_add(1);
        host.cooldown_until = Some(Instant::now() + FAILED_ATTEMPT_COOLDOWN);
        let attempts = host.failed_attempts;

        if attempts >= MAX_FAILED_ATTEMPTS {
            let locked = SyncStatusView {
                phase: SyncPhase::LockedOut,
                local_device_name: host.local_name.clone(),
                failed_attempts: attempts,
                max_attempts: MAX_FAILED_ATTEMPTS,
                message: Some("Session terminated after too many failed attempts.".to_owned()),
                ..SyncStatusView::default()
            };
            wipe_sessions(&mut guard);
            guard.last_terminal = Some(locked.clone());
            drop(guard);
            let _ = app.emit("sync://state-changed", &locked);
            return (
                410,
                "text/plain",
                b"too many failed attempts; session closed".to_vec(),
            );
        }

        let view = snapshot_from_inner(&guard);
        drop(guard);
        let _ = app.emit("sync://state-changed", &view);
        return (
            403,
            "text/plain",
            format!("invalid pairing code ({attempts}/{MAX_FAILED_ATTEMPTS} attempts)")
                .into_bytes(),
        );
    }

    let ticket = generate_hex_token();
    let clean_peer = req.device_name.trim();
    let peer_name = if clean_peer.is_empty() {
        "RtMaga Device".to_owned()
    } else {
        clean_peer.chars().take(64).collect()
    };

    host.peer_name = Some(peer_name);
    host.peer_ip = Some(peer_ip.to_string());
    host.transfer_ticket = Some(ticket.clone());
    host.decision = HostDecision::Pending;

    let reply = PairResponseBody {
        peer_name: host.local_name.clone(),
        peer_ip: host.local_ip.clone(),
        manga_count: host.manga_count,
        transfer_ticket: ticket,
    };
    let view = snapshot_from_inner(&guard);
    drop(guard);

    let _ = app.emit("sync://state-changed", &view);
    let json = serde_json::to_vec(&reply).unwrap_or_default();
    (200, "application/json", json)
}

async fn handle_status_route(
    state: &AppState,
    ticket: Option<&str>,
) -> (u16, &'static str, Vec<u8>) {
    let Some(t) = ticket else {
        return (400, "text/plain", b"missing ticket".to_vec());
    };
    let mut guard = state.sync.inner.lock().await;
    let Some(host) = guard.host.as_ref() else {
        return (410, "text/plain", b"session closed".to_vec());
    };
    if Instant::now() >= host.expires_at {
        wipe_sessions(&mut guard);
        return (410, "text/plain", b"sync session expired".to_vec());
    }
    let Some(expected) = host.transfer_ticket.as_deref() else {
        return (403, "text/plain", b"not paired".to_vec());
    };
    if !constant_time_eq(t.as_bytes(), expected.as_bytes()) {
        return (403, "text/plain", b"invalid ticket".to_vec());
    }
    let decision = match host.decision {
        HostDecision::Pending => "pending",
        HostDecision::Approved => "approved",
        HostDecision::Rejected => "rejected",
    };
    let body = serde_json::to_vec(&StatusResponseBody {
        decision: decision.to_owned(),
    })
    .unwrap_or_default();
    (200, "application/json", body)
}

async fn handle_pull_route(
    app: &AppHandle,
    state: &AppState,
    body: &[u8],
) -> (u16, &'static str, Vec<u8>) {
    let Ok(req) = serde_json::from_slice::<TicketBody>(body) else {
        return (400, "text/plain", b"invalid ticket payload".to_vec());
    };

    let mut guard = state.sync.inner.lock().await;
    let Some(host) = guard.host.as_mut() else {
        return (410, "text/plain", b"session closed".to_vec());
    };
    if Instant::now() >= host.expires_at {
        wipe_sessions(&mut guard);
        return (410, "text/plain", b"sync session expired".to_vec());
    }
    let Some(expected) = host.transfer_ticket.as_deref() else {
        return (403, "text/plain", b"not paired".to_vec());
    };
    if !constant_time_eq(req.ticket.as_bytes(), expected.as_bytes()) {
        return (403, "text/plain", b"invalid ticket".to_vec());
    }
    if host.decision != HostDecision::Approved {
        return (403, "text/plain", b"sender has not confirmed yet".to_vec());
    }

    let Some(bundle) = host.bundle.take() else {
        return (410, "text/plain", b"bundle already transferred".to_vec());
    };
    let json = serde_json::to_vec(&bundle).unwrap_or_default();

    let done = SyncStatusView {
        phase: SyncPhase::Completed,
        local_device_name: host.local_name.clone(),
        peer_name: host.peer_name.clone(),
        peer_ip: host.peer_ip.clone(),
        manga_count: host.manga_count,
        max_attempts: MAX_FAILED_ATTEMPTS,
        message: Some("Data transferred successfully. Session closed.".to_owned()),
        ..SyncStatusView::default()
    };

    // Single-use session: immediately close sockets and scrub tokens & payload from RAM.
    wipe_sessions(&mut guard);
    guard.last_terminal = Some(done.clone());
    drop(guard);

    let _ = app.emit("sync://state-changed", &done);
    (200, "application/json", json)
}

async fn handle_reject_route(
    app: &AppHandle,
    state: &AppState,
    body: &[u8],
) -> (u16, &'static str, Vec<u8>) {
    let Ok(req) = serde_json::from_slice::<TicketBody>(body) else {
        return (400, "text/plain", b"invalid payload".to_vec());
    };
    let mut guard = state.sync.inner.lock().await;
    let Some(host) = guard.host.as_ref() else {
        return (200, "text/plain", b"ok".to_vec());
    };
    if host
        .transfer_ticket
        .as_deref()
        .is_some_and(|exp| constant_time_eq(req.ticket.as_bytes(), exp.as_bytes()))
    {
        let cancelled = SyncStatusView {
            phase: SyncPhase::Cancelled,
            local_device_name: host.local_name.clone(),
            max_attempts: MAX_FAILED_ATTEMPTS,
            message: Some("Peer declined the sync transfer.".to_owned()),
            ..SyncStatusView::default()
        };
        wipe_sessions(&mut guard);
        guard.last_terminal = Some(cancelled.clone());
        drop(guard);
        let _ = app.emit("sync://state-changed", &cancelled);
    }
    (200, "text/plain", b"ok".to_vec())
}

async fn expire_host_session_if_matching(app: &AppHandle, state: &AppState, expected_token: &str) {
    let view = {
        let mut guard = state.sync.inner.lock().await;
        let matches = guard
            .host
            .as_ref()
            .is_some_and(|h| constant_time_eq(h.token.as_bytes(), expected_token.as_bytes()));
        if !matches {
            return;
        }
        let name = guard
            .host
            .as_ref()
            .map_or_else(local_device_name, |h| h.local_name.clone());
        wipe_sessions(&mut guard);
        let expired = SyncStatusView {
            phase: SyncPhase::Expired,
            local_device_name: name,
            max_attempts: MAX_FAILED_ATTEMPTS,
            message: Some("Sync session expired after 5 minutes.".to_owned()),
            ..SyncStatusView::default()
        };
        guard.last_terminal = Some(expired.clone());
        expired
    };
    let _ = app.emit("sync://state-changed", &view);
    debug!("LAN sync session expired and was wiped from memory");
}

async fn run_udp_discovery_responder(expected_pin_hash: String, reply_json: String) {
    let Ok(sock) = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, LAN_DISCOVERY_PORT)).await else {
        debug!("UDP discovery port busy; manual IP:Port fallback remains available");
        return;
    };
    let mut buf = [0_u8; 512];
    loop {
        let Ok((n, peer)) = sock.recv_from(&mut buf).await else {
            break;
        };
        if !is_allowed_sync_peer_ip(peer.ip()) {
            continue;
        }
        let Ok(msg) = std::str::from_utf8(&buf[..n]) else {
            continue;
        };
        if let Some(hash) = msg.trim().strip_prefix("RTMAGA_SYNC_DISCOVER_V1:")
            && constant_time_eq(hash.as_bytes(), expected_pin_hash.as_bytes())
        {
            let _ = sock.send_to(reply_json.as_bytes(), peer).await;
        }
    }
}

async fn discover_phone_on_lan(pin: &str) -> Result<UdpBeaconReply, AppError> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .await
        .map_err(|e| AppError::Network {
            message: format!("failed to open LAN discovery socket: {e}"),
        })?;
    sock.set_broadcast(true).map_err(|e| AppError::Network {
        message: format!("failed to enable LAN broadcast: {e}"),
    })?;

    let probe = format!("RTMAGA_SYNC_DISCOVER_V1:{}", sha256_hex(pin.as_bytes()));
    let target = SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), LAN_DISCOVERY_PORT);
    let mut buf = [0_u8; 1024];

    for _ in 0..3 {
        let _ = sock.send_to(probe.as_bytes(), target).await;
        if let Ok(Ok((n, peer))) =
            tokio::time::timeout(Duration::from_millis(750), sock.recv_from(&mut buf)).await
            && is_allowed_sync_peer_ip(peer.ip())
            && let Ok(reply) = serde_json::from_slice::<UdpBeaconReply>(&buf[..n])
        {
            return Ok(reply);
        }
    }

    Err(AppError::Network {
        message:
            "no matching device found on LAN — check the 6-digit code or enter IP:Port manually"
                .to_owned(),
    })
}

fn wipe_sessions(inner: &mut SyncManagerInner) {
    if let Some(mut host) = inner.host.take() {
        for task in host.tasks.drain(..) {
            task.abort();
        }
        host.pin.clear();
        host.token.clear();
        host.bundle = None;
        host.transfer_ticket = None;
    }
    if let Some(mut rec) = inner.receiver.take() {
        rec.cancelled.store(true, Ordering::Release);
        rec.transfer_ticket.clear();
    }
}

fn snapshot_from_inner(inner: &SyncManagerInner) -> SyncStatusView {
    if let Some(host) = inner.host.as_ref() {
        let phase = if host.transfer_ticket.is_some() {
            match host.decision {
                HostDecision::Pending => SyncPhase::HostConfirmPeer,
                HostDecision::Approved => SyncPhase::HostWaitingPull,
                HostDecision::Rejected => SyncPhase::Cancelled,
            }
        } else {
            SyncPhase::HostingWaiting
        };
        return SyncStatusView {
            phase,
            mode: Some(host.mode),
            local_device_name: host.local_name.clone(),
            local_ip: Some(host.local_ip.clone()),
            local_port: Some(host.local_port),
            pairing_code: Some(host.pin.clone()),
            qr_svg: host.qr_svg.clone(),
            compact_code: host.compact_code.clone(),
            expires_at_unix: Some(host.expires_at_unix),
            manga_count: host.manga_count,
            peer_name: host.peer_name.clone(),
            peer_ip: host.peer_ip.clone(),
            failed_attempts: host.failed_attempts,
            max_attempts: MAX_FAILED_ATTEMPTS,
            report: None,
            message: None,
        };
    }

    if let Some(rec) = inner.receiver.as_ref() {
        return SyncStatusView {
            phase: SyncPhase::ReceiverConfirmPeer,
            local_device_name: local_device_name(),
            expires_at_unix: Some(rec.expires_at_unix),
            manga_count: rec.manga_count,
            peer_name: Some(rec.peer_name.clone()),
            peer_ip: Some(rec.peer_ip.clone()),
            max_attempts: MAX_FAILED_ATTEMPTS,
            ..SyncStatusView::default()
        };
    }

    inner
        .last_terminal
        .clone()
        .unwrap_or_else(|| SyncStatusView {
            phase: SyncPhase::Idle,
            local_device_name: local_device_name(),
            max_attempts: MAX_FAILED_ATTEMPTS,
            ..SyncStatusView::default()
        })
}

struct ParsedHttpRequest {
    method: String,
    target: String,
    body: Vec<u8>,
}

impl ParsedHttpRequest {
    fn path_only(&self) -> &str {
        self.target.split('?').next().unwrap_or(&self.target)
    }

    fn query_param(&self, key: &str) -> Option<&str> {
        let query = self.target.split_once('?')?.1;
        for pair in query.split('&') {
            if let Some((k, v)) = pair.split_once('=')
                && k == key
            {
                return Some(v);
            }
        }
        None
    }
}

async fn read_http_request(stream: &mut TcpStream) -> Result<ParsedHttpRequest, std::io::Error> {
    let mut raw = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];
    let header_end;

    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "closed before headers",
            ));
        }
        raw.extend_from_slice(&chunk[..n]);
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
        if raw.len() > 32 * 1024 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "headers too large",
            ));
        }
    }

    let head_str = String::from_utf8_lossy(&raw[..header_end]);
    let mut lines = head_str.lines();
    let req_line = lines.next().unwrap_or("");
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_owned();
    let target = parts.next().unwrap_or("").to_owned();

    let mut content_len: usize = 0;
    for line in lines {
        if let Some((k, v)) = line.split_once(':')
            && k.trim().eq_ignore_ascii_case("content-length")
        {
            content_len = v.trim().parse::<usize>().unwrap_or(0);
        }
    }
    if content_len > MAX_HTTP_REQ_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "body too large",
        ));
    }

    let mut body = raw[header_end..].to_vec();
    while body.len() < content_len {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(content_len);

    Ok(ParsedHttpRequest {
        method,
        target,
        body,
    })
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<(), std::io::Error> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        410 => "Gone",
        429 => "Too Many Requests",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

fn build_lan_http_client(timeout: Duration) -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::Network {
            message: format!("failed to create LAN client: {e}"),
        })
}

fn encode_qr_payload(payload: &QrPayload) -> Result<String, AppError> {
    let json = serde_json::to_vec(payload).map_err(|e| AppError::Internal {
        message: format!("failed to serialize QR payload: {e}"),
    })?;
    Ok(format!("{QR_CODE_PREFIX}{}", URL_SAFE_NO_PAD.encode(json)))
}

fn decode_qr_payload(raw: &str) -> Result<QrPayload, AppError> {
    let trimmed = raw.trim();
    if let Some(b64) = trimmed.strip_prefix(QR_CODE_PREFIX) {
        let bytes = URL_SAFE_NO_PAD
            .decode(b64.trim())
            .map_err(|_| AppError::InvalidUrl {
                message: "invalid QR sync payload encoding".to_owned(),
            })?;
        return serde_json::from_slice(&bytes).map_err(|_| AppError::InvalidUrl {
            message: "invalid QR sync payload content".to_owned(),
        });
    }
    serde_json::from_str(trimmed).map_err(|_| AppError::InvalidUrl {
        message: "unrecognized RtMaga QR code".to_owned(),
    })
}

fn render_qr_svg(content: &str) -> Result<String, AppError> {
    let code = QrCode::new(content.as_bytes()).map_err(|e| AppError::Internal {
        message: format!("failed to generate QR code: {e}"),
    })?;
    Ok(code
        .render::<svg::Color<'_>>()
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#111114"))
        .light_color(svg::Color("#ffffff"))
        .build())
}

/// Checks whether `ip` belongs strictly to a private local area network (RFC1918 / link-local / ULA).
pub(crate) fn is_allowed_sync_peer_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || (cfg!(test) && v4.is_loopback()),
        IpAddr::V6(v6) => {
            let s0 = v6.segments()[0];
            let is_ula = (s0 & 0xfe00) == 0xfc00;
            let is_link_local = (s0 & 0xffc0) == 0xfe80;
            is_ula
                || is_link_local
                || v6.to_ipv4_mapped().is_some_and(|v4| {
                    v4.is_private() || v4.is_link_local() || (cfg!(test) && v4.is_loopback())
                })
                || (cfg!(test) && v6.is_loopback())
        }
    }
}

fn detect_lan_ipv4() -> Option<Ipv4Addr> {
    for probe in [
        "192.168.1.1:80",
        "10.0.0.1:80",
        "172.16.0.1:80",
        "1.1.1.1:80",
    ] {
        if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0")
            && sock.connect(probe).is_ok()
            && let Ok(SocketAddr::V4(addr)) = sock.local_addr()
            && (addr.ip().is_private() || addr.ip().is_link_local())
        {
            return Some(*addr.ip());
        }
    }
    None
}

fn format_http_origin(ip: IpAddr, port: u16) -> String {
    match ip {
        IpAddr::V4(v4) => format!("http://{v4}:{port}"),
        IpAddr::V6(v6) => format!("http://[{v6}]:{port}"),
    }
}

async fn resolve_local_device_name(app: &AppHandle, state: &AppState) -> String {
    let custom = state.current_settings().await.device.custom_name;
    crate::android_env::resolve_display_device_name(Some(app), custom.as_deref())
}

fn local_device_name() -> String {
    crate::android_env::detect_os_device_name()
}

fn generate_six_digit_pin() -> String {
    let num: u32 = rand::thread_rng().gen_range(100_000..=999_999);
    format!("{num:06}")
}

fn generate_hex_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    let mut out = String::with_capacity(64);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn sha256_hex(input: &[u8]) -> String {
    let digest = Sha256::digest(input);
    let mut out = String::with_capacity(64);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (&x, &y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enforces_lan_only_ip_restrictions() {
        assert!(is_allowed_sync_peer_ip(IpAddr::V4(Ipv4Addr::new(
            192, 168, 1, 20
        ))));
        assert!(is_allowed_sync_peer_ip(IpAddr::V4(Ipv4Addr::new(
            10, 0, 0, 5
        ))));
        assert!(is_allowed_sync_peer_ip(IpAddr::V4(Ipv4Addr::new(
            172, 16, 4, 2
        ))));
        assert!(!is_allowed_sync_peer_ip(IpAddr::V4(Ipv4Addr::new(
            8, 8, 8, 8
        ))));
        assert!(!is_allowed_sync_peer_ip(IpAddr::V4(Ipv4Addr::new(
            1, 1, 1, 1
        ))));
    }

    #[test]
    fn qr_payload_and_svg_round_trip() {
        let p = QrPayload {
            v: 1,
            ip: "192.168.1.42".to_owned(),
            p: 54_321,
            c: "482910".to_owned(),
            t: generate_hex_token(),
            d: "My-PC".to_owned(),
        };
        let encoded = encode_qr_payload(&p).unwrap();
        assert!(encoded.starts_with(QR_CODE_PREFIX));
        let decoded = decode_qr_payload(&encoded).unwrap();
        assert_eq!(decoded.ip, "192.168.1.42");
        assert_eq!(decoded.c, "482910");

        let svg_str = render_qr_svg(&encoded).unwrap();
        assert!(svg_str.contains("<svg"), "must produce valid inline SVG QR");
    }

    #[test]
    fn constant_time_equality_works() {
        assert!(constant_time_eq(b"123456", b"123456"));
        assert!(!constant_time_eq(b"123456", b"123457"));
        assert!(!constant_time_eq(b"123456", b"12345"));
    }
}
