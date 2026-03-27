use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use futures::{SinkExt as _, StreamExt as _};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;
use chrono::Utc;

use crate::types::{BrowserMonitorConfig, MediaItem};

pub type EventCallback = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync + 'static>;

// ── WebSocket broadcast hub ───────────────────────────────────────────────────

/// Lightweight broadcast hub for the extension WebSocket clients.
/// All connected extension instances share one `broadcast::Sender`.
/// Dropped receivers are silently ignored.
#[derive(Clone)]
pub struct WsHub {
    tx: tokio::sync::broadcast::Sender<String>,
}

impl WsHub {
    pub fn new() -> Self {
        // Channel capacity: 64 messages.  Slow receivers are dropped (lagged).
        let (tx, _) = tokio::sync::broadcast::channel(64);
        Self { tx }
    }

    /// Broadcast a JSON message to all connected extension WebSocket clients.
    pub fn broadcast(&self, msg: impl Into<String>) {
        self.tx.send(msg.into()).ok(); // ignore "no receivers" error
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.tx.subscribe()
    }
}

#[derive(Clone)]
pub struct BrowserMonitorState {
    pub media_list: Arc<Mutex<Vec<MediaItem>>>,
    pub config: Arc<Mutex<BrowserMonitorConfig>>,
    pub emit: EventCallback,
    pub ws_hub: WsHub,
    /// Random token generated at startup.  The extension receives it via /sync
    /// and must include it as `X-QDM-Token` on every subsequent request.
    pub session_token: Arc<String>,
}

pub struct BrowserMonitor {
    state: BrowserMonitorState,
    port: u16,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl BrowserMonitor {
    /// Broadcast a JSON string to all connected extension WebSocket clients.
    /// Call this from `lib.rs` when download events should be pushed to the extension.
    pub fn broadcast(&self, msg: impl Into<String>) {
        self.state.ws_hub.broadcast(msg);
    }

    pub fn new(emit: EventCallback) -> Self {
        let config = BrowserMonitorConfig {
            enabled: true,
            file_extensions: vec![
                ".zip", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz",
                ".exe", ".msi", ".dmg", ".deb", ".rpm", ".appimage", ".apk",
                ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
                ".mp3", ".flac", ".wav", ".aac", ".ogg", ".wma", ".m4a", ".opus",
                ".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm", ".m4v",
                ".iso", ".img", ".bin", ".torrent",
                ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".svg", ".webp",
                ".epub", ".mobi", ".azw3",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            video_extensions: vec![
                ".mp4", ".mkv", ".webm", ".avi", ".mov", ".flv", ".m4v",
                ".ts", ".m3u8", ".mpd", ".f4m",
                ".mp3", ".m4a", ".aac", ".ogg", ".opus", ".flac", ".wav",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            blocked_hosts: vec![
                "update.googleapis.com",
                "safebrowsing.googleapis.com",
                "clients2.google.com",
                "clients1.google.com",
                "translate.googleapis.com",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            media_types: vec![
                "audio/".to_string(),
                "video/".to_string(),
                "application/vnd.apple.mpegurl".to_string(),
                "application/x-mpegurl".to_string(),
                "application/dash+xml".to_string(),
                "video/vnd.mpeg.dash.mpd".to_string(),
                "application/f4m+xml".to_string(),
                "application/vnd.ms-sstr+xml".to_string(),
            ],
            tabs_watcher: vec![".youtube.".to_string(), "/watch?v=".to_string()],
            matching_hosts: vec!["googlevideo".to_string()],
        };

        // Generate a random session token.  The extension receives it on the
        // first /sync call and must echo it as X-QDM-Token on all later requests.
        let session_token = Arc::new(Uuid::new_v4().simple().to_string());

        Self {
            state: BrowserMonitorState {
                media_list: Arc::new(Mutex::new(Vec::new())),
                config: Arc::new(Mutex::new(config)),
                emit,
                ws_hub: WsHub::new(),
                session_token,
            },
            port: 8597,
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self) {
        let state = self.state.clone();
        let port = self.port;
        let shutdown_arc = Arc::clone(&self.shutdown_tx);

        tauri::async_runtime::spawn(async move {
            let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
            {
                let mut lock = shutdown_arc.lock().await;
                *lock = Some(shutdown_tx);
            }

            let cors = CorsLayer::new()
                .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
                .allow_origin(Any)
                .allow_headers(Any);

            let app = Router::new()
                .route("/download", post(handle_download))
                .route("/media", post(handle_media).get(handle_get_media))
                .route("/vid", post(handle_vid))
                .route("/tab-update", post(handle_tab_update))
                .route("/clear", post(handle_clear))
                .route("/link", post(handle_link))
                .route("/sync", get(handle_sync))
                .route("/show", get(handle_show))
                // WebSocket endpoint — extension connects here for real-time push
                .route("/ws", get(handle_ws_upgrade))
                .layer(cors)
                .with_state(state);

            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            log::info!("[BrowserMonitor] Listening on port {}", port);

            // Try up to 3 times with a short delay — handles the case where a
            // previous QDM instance hasn't released the port yet.
            let listener = {
                let mut last_err = String::new();
                let mut result = None;
                for attempt in 0..3 {
                    match tokio::net::TcpListener::bind(addr).await {
                        Ok(l) => { result = Some(l); break; }
                        Err(e) => {
                            last_err = e.to_string();
                            log::warn!("[BrowserMonitor] Bind attempt {} failed: {}", attempt + 1, e);
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        }
                    }
                }
                match result {
                    Some(l) => l,
                    None => {
                        log::error!("[BrowserMonitor] Could not bind port {} after 3 attempts: {}", port, last_err);
                        return;
                    }
                }
            };

            axum::serve(listener, app)
                .with_graceful_shutdown(async { shutdown_rx.await.ok(); })
                .await
                .ok();
        });
    }

    pub fn stop(&self) {
        let shutdown_arc = Arc::clone(&self.shutdown_tx);
        tauri::async_runtime::spawn(async move {
            let mut lock = shutdown_arc.lock().await;
            if let Some(tx) = lock.take() {
                tx.send(()).ok();
            }
        });
    }

    pub async fn get_media_list(&self) -> Vec<MediaItem> {
        self.state.media_list.lock().await.clone()
    }

    pub async fn clear_media_list(&self) {
        self.state.media_list.lock().await.clear();
    }

    pub async fn get_config(&self) -> BrowserMonitorConfig {
        self.state.config.lock().await.clone()
    }

    pub async fn set_config(&self, config: BrowserMonitorConfig) {
        let mut c = self.state.config.lock().await;
        *c = config;
    }

    pub fn get_port(&self) -> u16 {
        self.port
    }
}

// ── Request bodies ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct DownloadRequest {
    url: Option<String>,
    file: Option<String>,
    method: Option<String>,
    #[serde(rename = "requestHeaders", default)]
    request_headers: Option<HashMap<String, String>>,
    #[serde(rename = "responseHeaders", default)]
    response_headers: Option<HashMap<String, String>>,
    cookie: Option<String>,
    #[serde(rename = "ytdlpCookies")]
    ytdlp_cookies: Option<String>,
    #[serde(rename = "tabUrl")]
    tab_url: Option<String>,
    #[serde(rename = "contentType")]
    content_type: Option<String>,
    #[serde(rename = "contentLength")]
    content_length: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
struct MediaRequest {
    url: Option<String>,
    file: Option<String>,
    #[serde(rename = "contentType")]
    content_type: Option<String>,
    #[serde(rename = "contentLength")]
    content_length: Option<i64>,
    #[serde(rename = "tabUrl")]
    tab_url: Option<String>,
    #[serde(rename = "tabTitle")]
    tab_title: Option<String>,
    #[serde(rename = "tabId")]
    tab_id: Option<String>,
    cookie: Option<String>,
    #[serde(rename = "requestHeaders", default)]
    request_headers: Option<HashMap<String, String>>,
    quality: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct VidRequest {
    url: Option<String>,
    file: Option<String>,
    #[serde(rename = "tabUrl")]
    tab_url: Option<String>,
    #[serde(rename = "ytdlpCookies")]
    ytdlp_cookies: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct TabUpdateRequest {
    #[serde(rename = "tabId")]
    tab_id: Option<String>,
    #[serde(rename = "tabTitle")]
    tab_title: Option<String>,
    #[serde(rename = "tabUrl")]
    tab_url: Option<String>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// Rewrite site-specific share/redirect URLs into direct download links.
fn rewrite_download_url(url: String) -> String {
    // Dropbox: ?dl=0 (view page) → ?dl=1 (direct download)
    if url.contains("dropbox.com") {
        if url.contains("?dl=0") {
            return url.replace("?dl=0", "?dl=1");
        } else if !url.contains("dl=") {
            let sep = if url.contains('?') { "&" } else { "?" };
            return format!("{}{}dl=1", url, sep);
        }
    }
    // Google Drive: convert share URL to direct export link
    // https://drive.google.com/file/d/{id}/view → https://drive.google.com/uc?id={id}&export=download&confirm=1
    if url.contains("drive.google.com/file/d/") {
        if let Some(start) = url.find("/file/d/") {
            let rest = &url[start + 8..];
            let id_end = rest.find('/').unwrap_or(rest.len());
            let file_id = &rest[..id_end];
            if !file_id.is_empty() {
                return format!("https://drive.google.com/uc?id={}&export=download&confirm=1", file_id);
            }
        }
    }
    // OneDrive: redir → direct download parameter
    if url.contains("onedrive.live.com") && url.contains("redir") {
        if let Ok(mut parsed) = url::Url::parse(&url) {
            parsed.query_pairs_mut().append_pair("download", "1");
            return parsed.to_string();
        }
    }
    url
}

fn clean_headers(headers: Option<HashMap<String, String>>) -> Option<HashMap<String, String>> {
    headers.map(|hdrs| {
        let blocked = ["host", "connection", "accept-encoding", "content-length", "transfer-encoding"];
        hdrs.into_iter()
            .filter(|(k, _)| !blocked.contains(&k.to_lowercase().as_str()))
            .collect()
    })
}

async fn handle_download(
    State(state): State<BrowserMonitorState>,
    headers: HeaderMap,
    body: axum::extract::Json<serde_json::Value>,
) -> StatusCode {
    if !valid_token(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let req: DownloadRequest = serde_json::from_value(body.0).unwrap_or_default();

    let url = match req.url {
        Some(u) if !u.is_empty() => u,
        _ => return StatusCode::BAD_REQUEST,
    };

    // Site-specific URL rewriters — convert share/redirect URLs into direct download links
    let url = rewrite_download_url(url);

    let lower_url = url.to_lowercase();
    {
        let config = state.config.lock().await;
        if !config.enabled {
            return StatusCode::OK;
        }
        // Blocked hosts only — the extension has already done type classification;
        // don't second-guess it here (that caused yt-dlp / headerless URLs to be dropped).
        if config.blocked_hosts.iter().any(|h| lower_url.contains(h.as_str())) {
            return StatusCode::OK;
        }
    }

    let mut headers = clean_headers(req.request_headers);
    if let Some(cookie) = req.cookie {
        if !cookie.is_empty() {
            headers.get_or_insert_with(HashMap::new).insert("Cookie".to_string(), cookie);
        }
    }
    if let Some(ref tab_url) = req.tab_url {
        headers.get_or_insert_with(HashMap::new).insert("Referer".to_string(), tab_url.clone());
    }

    let file_name = req.file.clone().unwrap_or_default();
    let tab_url = req.tab_url.clone().unwrap_or_default();
    // Keep cookie as a top-level field too so lib.rs yt-dlp branch can find it
    let cookie_val = headers.as_ref()
        .and_then(|h| h.get("Cookie"))
        .cloned()
        .unwrap_or_default();
    let mut payload = serde_json::json!({
        "url": url,
        "fileName": file_name,
        "headers": headers,
        "cookie": cookie_val,
        "tabUrl": tab_url,
    });
    if let Some(ref ytc) = req.ytdlp_cookies {
        if !ytc.is_empty() {
            payload["ytdlpCookies"] = serde_json::Value::String(ytc.clone());
        }
    }

    (state.emit)("browser:download", payload);
    StatusCode::OK
}

async fn handle_media(
    State(state): State<BrowserMonitorState>,
    headers: HeaderMap,
    body: axum::extract::Json<serde_json::Value>,
) -> StatusCode {
    if !valid_token(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let req: MediaRequest = serde_json::from_value(body.0).unwrap_or_default();

    let url = match req.url {
        Some(u) if !u.is_empty() => u,
        _ => return StatusCode::OK,
    };

    let config = state.config.lock().await;
    if !config.enabled {
        return StatusCode::OK;
    }

    let content_type = req.content_type.clone().unwrap_or_default();
    let lower_url = url.to_lowercase();
    let lower_ct = content_type.to_lowercase();

    let is_media = config.media_types.iter().any(|mt| lower_ct.starts_with(mt.as_str()) || lower_ct == mt.as_str())
        || config.video_extensions.iter().any(|ext| lower_url.contains(ext.as_str()))
        || config.matching_hosts.iter().any(|h| lower_url.contains(h.as_str()));

    if !is_media {
        return StatusCode::OK;
    }

    if config
        .blocked_hosts
        .iter()
        .any(|h| lower_url.contains(h.as_str()))
    {
        return StatusCode::OK;
    }
    drop(config);

    let tab_id = req.tab_id.clone().unwrap_or_default();
    let tab_url = req.tab_url.clone().unwrap_or_default();
    let name = req
        .file
        .clone()
        .or_else(|| {
            url::Url::parse(&url)
                .ok()
                .and_then(|u| u.path_segments().and_then(|s| s.last().map(String::from)))
        })
        .unwrap_or_else(|| "video".to_string());

    let description = format!(
        "{}{}",
        req.quality
            .as_deref()
            .map(|q| format!("{} • ", q))
            .unwrap_or_default(),
        req.tab_title.as_deref().unwrap_or("")
    );

    let media_type = if lower_url.contains(".m3u8") || lower_url.contains(".mpd") {
        "hls"
    } else if lower_ct.starts_with("audio/") {
        "audio"
    } else {
        "video"
    };

    let item = MediaItem {
        id: Uuid::new_v4().to_string().replace('-', "")[..16].to_string(),
        name,
        description,
        tab_url,
        tab_id,
        date_added: Utc::now().to_rfc3339(),
        url: url.clone(),
        audio_url: None,
        media_type: media_type.to_string(),
        size: req.content_length.unwrap_or(-1),
        content_type: content_type.clone(),
        headers: clean_headers(req.request_headers),
        cookies: req.cookie,
    };

    let mut list = state.media_list.lock().await;
    // Avoid duplicates
    if !list.iter().any(|m| m.url == url) {
        list.push(item.clone());
        let new_count = list.len();
        drop(list);
        (state.emit)("browser:media", serde_json::to_value(&item).unwrap_or_default());
        // Push a status update to extension WebSocket clients
        state.ws_hub.broadcast(
            serde_json::json!({ "type": "media_added", "mediaCount": new_count }).to_string()
        );
    }

    StatusCode::OK
}

async fn handle_vid(
    State(state): State<BrowserMonitorState>,
    headers: HeaderMap,
    body: axum::extract::Json<serde_json::Value>,
) -> StatusCode {
    if !valid_token(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let req: VidRequest = serde_json::from_value(body.0).unwrap_or_default();

    if let Some(url) = req.url {
        if !url.is_empty() {
            // Look up stored headers/cookies from the media list
            let stored = {
                let list = state.media_list.lock().await;
                list.iter().find(|m| m.url == url).map(|item| {
                    (item.headers.clone(), item.cookies.clone())
                })
            };
            let (headers, cookie) = stored.unwrap_or((None, None));

            let mut payload = serde_json::json!({
                "url": url,
                "fileName": req.file.unwrap_or_default(),
                "tabUrl": req.tab_url.unwrap_or_default(),
            });
            if let Some(h) = headers {
                payload["headers"] = serde_json::to_value(h).unwrap_or_default();
            }
            if let Some(c) = cookie {
                payload["cookie"] = serde_json::Value::String(c);
            }
            if let Some(ref ytc) = req.ytdlp_cookies {
                if !ytc.is_empty() {
                    payload["ytdlpCookies"] = serde_json::Value::String(ytc.clone());
                }
            }
            (state.emit)("browser:vid-download", payload);
        }
    }
    StatusCode::OK
}

async fn handle_tab_update(
    State(state): State<BrowserMonitorState>,
    headers: HeaderMap,
    body: axum::extract::Json<serde_json::Value>,
) -> StatusCode {
    if !valid_token(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    let req: TabUpdateRequest = serde_json::from_value(body.0).unwrap_or_default();

    if let (Some(tab_id), Some(tab_title)) = (req.tab_id, req.tab_title) {
        let mut list = state.media_list.lock().await;
        for item in list.iter_mut() {
            if item.tab_id == tab_id && item.name == item.tab_url {
                item.name = tab_title.clone();
            }
        }
    }
    StatusCode::OK
}

async fn handle_clear(
    State(state): State<BrowserMonitorState>,
    headers: HeaderMap,
) -> StatusCode {
    if !valid_token(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    state.media_list.lock().await.clear();
    StatusCode::OK
}

async fn handle_link(
    State(state): State<BrowserMonitorState>,
    headers: HeaderMap,
    body: axum::extract::Json<serde_json::Value>,
) -> StatusCode {
    if !valid_token(&state, &headers) {
        return StatusCode::UNAUTHORIZED;
    }
    // Batch link collection - emit event for each URL
    if let Some(urls) = body.get("urls").and_then(|v| v.as_array()) {
        for url_val in urls {
            if let Some(url) = url_val.as_str() {
                (state.emit)("browser:link", serde_json::json!({ "url": url }));
            }
        }
    }
    StatusCode::OK
}

// ── WebSocket handlers ────────────────────────────────────────────────────────

async fn handle_ws_upgrade(
    State(state): State<BrowserMonitorState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_ws_client(socket, state))
}

async fn handle_ws_client(socket: WebSocket, state: BrowserMonitorState) {
    let mut rx = state.ws_hub.subscribe();
    let (mut sender, mut receiver) = socket.split();

    // Send an immediate status snapshot so the extension badge updates on connect
    let media_count = state.media_list.lock().await.len();
    let hello = serde_json::json!({ "type": "status", "mediaCount": media_count });
    sender
        .send(Message::Text(hello.to_string().into()))
        .await
        .ok();

    loop {
        tokio::select! {
            // Forward broadcast messages to this WebSocket client
            msg = rx.recv() => {
                match msg {
                    Ok(text) => {
                        if sender.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            // Handle pings / client-initiated close
            frame = receiver.next() => {
                match frame {
                    Some(Ok(Message::Ping(data))) => {
                        sender.send(Message::Pong(data)).await.ok();
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

#[derive(Serialize)]
struct SyncResponse {
    status: String,
    config: BrowserMonitorConfig,
    #[serde(rename = "mediaCount")]
    media_count: usize,
    /// Session token the extension must echo back on all subsequent requests.
    token: String,
}

/// Returns true if the request carries the correct session token.
fn valid_token(state: &BrowserMonitorState, headers: &HeaderMap) -> bool {
    headers
        .get("x-qdm-token")
        .and_then(|v| v.to_str().ok())
        .map(|t| t == state.session_token.as_str())
        .unwrap_or(false)
}

async fn handle_get_media(
    State(state): State<BrowserMonitorState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if !valid_token(&state, &headers) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({}))).into_response();
    }
    let list = state.media_list.lock().await;
    Json(serde_json::json!({ "items": *list })).into_response()
}

async fn handle_show(State(state): State<BrowserMonitorState>) -> StatusCode {
    (state.emit)("browser:show", serde_json::json!({}));
    StatusCode::OK
}

async fn handle_sync(State(state): State<BrowserMonitorState>) -> Json<SyncResponse> {
    let config = state.config.lock().await.clone();
    let media_count = state.media_list.lock().await.len();
    Json(SyncResponse {
        status: "ok".to_string(),
        config,
        media_count,
        token: state.session_token.as_ref().clone(),
    })
}
