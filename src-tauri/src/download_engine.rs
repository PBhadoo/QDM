use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use futures::StreamExt;
use reqwest::header;
use tauri::{AppHandle, Emitter, Manager};
use uuid::Uuid;
use chrono::Utc;

use crate::hls_engine;
use crate::yt_dlp;
use crate::types::*;

// ── Constants ────────────────────────────────────────────────────────────────

/// How long a segment can go without receiving data before it is considered stalled.
const SEGMENT_STALL_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum number of times a failed segment is retried before giving up.
const SEGMENT_MAX_RETRIES: u32 = 5;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Returns true if the URL contains CDN-signed parameters that commonly expire.
fn is_signed_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    // AWS S3 pre-signed: X-Amz-Expires or X-Amz-Date
    // Azure SAS: se= (signed expiry)
    // CloudFront: Expires= signed cookies
    lower.contains("x-amz-expires") || lower.contains("x-amz-credential")
        || lower.contains("x-amz-date")
        || lower.contains("&se=") || lower.contains("?se=")
        || lower.contains("&expires=") || lower.contains("?expires=")
        || lower.contains("x-goog-signature") // GCS signed
}

fn is_retryable_segment_error(e: &str) -> bool {
    if e == "cancelled" || e == "link_expired" || e.starts_with("auth_required:") {
        return false;
    }
    // Parse "HTTP <status>" errors — only retry specific server-side codes
    if let Some(code_str) = e.strip_prefix("HTTP ") {
        let code: u16 = code_str.trim().parse().unwrap_or(0);
        return matches!(code, 408 | 429 | 500 | 502 | 503 | 504);
    }
    // Network / IO errors (connection reset, timeout, DNS) → retryable
    true
}

/// Parse the total file size from a `Content-Range: bytes 0-0/12345` header.
fn parse_content_range_total(value: &str) -> i64 {
    value
        .rsplit('/')
        .next()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(-1)
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn category_subfolder_name(cat: &DownloadCategory) -> &'static str {
    match cat {
        DownloadCategory::Videos     => "Video",
        DownloadCategory::Music      => "Music",
        DownloadCategory::Documents  => "Documents",
        DownloadCategory::Compressed => "Compressed",
        DownloadCategory::Programs   => "Programs",
        DownloadCategory::Other | DownloadCategory::All => "Other",
    }
}

fn get_category(file_name: &str) -> DownloadCategory {
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "zst" | "cab" | "iso" | "img" => {
            DownloadCategory::Compressed
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "csv"
        | "epub" | "mobi" => DownloadCategory::Documents,
        "mp3" | "flac" | "wav" | "aac" | "ogg" | "wma" | "m4a" | "opus" => DownloadCategory::Music,
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v" | "ts" | "m3u8" => {
            DownloadCategory::Videos
        }
        "exe" | "msi" | "dmg" | "deb" | "rpm" | "appimage" | "apk" | "app" => {
            DownloadCategory::Programs
        }
        _ => DownloadCategory::Other,
    }
}

fn generate_id() -> String {
    Uuid::new_v4().to_string().replace('-', "")[..16].to_string()
}

fn sanitize_filename(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || (c as u32) < 32 {
                '_'
            } else {
                c
            }
        })
        .collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "download".to_string()
    } else {
        s[..s.len().min(255)].to_string()
    }
}

/// Return a filename that does not collide with existing files in `dir`.
/// If `name` is "video.mp4" and it exists, tries "video_1.mp4", "video_2.mp4", …
fn unique_filename(dir: &str, name: &str) -> String {
    let path = Path::new(dir).join(name);
    if !path.exists() {
        return name.to_string();
    }
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    for i in 1..=999 {
        let candidate = if ext.is_empty() {
            format!("{}_{}", stem, i)
        } else {
            format!("{}_{}.{}", stem, i, ext)
        };
        if !Path::new(dir).join(&candidate).exists() {
            return candidate;
        }
    }
    name.to_string()
}

fn get_filename_from_url(url_str: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url_str) {
        let path = parsed.path();
        if let Some(name) = path.split('/').next_back() {
            if !name.is_empty() {
                let decoded = urlencoding::decode(name).unwrap_or_else(|_| name.into());
                return sanitize_filename(&decoded);
            }
        }
        // Check query params
        for (k, v) in parsed.query_pairs() {
            if k == "filename" || k == "file" || k == "name" {
                return sanitize_filename(&v);
            }
        }
    }
    format!("download_{}", Utc::now().timestamp())
}

fn get_filename_from_headers(headers: &reqwest::header::HeaderMap, url: &str) -> String {
    if let Some(cd) = headers.get("content-disposition") {
        if let Ok(cd_str) = cd.to_str() {
            // RFC5987: filename*=UTF-8''encoded%20name
            if let Some(pos) = cd_str.to_lowercase().find("filename*") {
                let rest = &cd_str[pos..];
                if let Some(eq) = rest.find('=') {
                    let val = rest[eq + 1..].trim();
                    // Strip encoding prefix like UTF-8''
                    let name_enc = if let Some(idx) = val.find("''") {
                        &val[idx + 2..]
                    } else {
                        val
                    };
                    let name_enc = name_enc.split(';').next().unwrap_or("").trim();
                    let name_enc = name_enc.trim_matches('"').trim_matches('\'');
                    if let Ok(decoded) = urlencoding::decode(name_enc) {
                        if !decoded.is_empty() {
                            return sanitize_filename(&decoded);
                        }
                    }
                }
            }
            // Simple filename="..."
            if let Some(pos) = cd_str.to_lowercase().find("filename=") {
                let rest = &cd_str[pos + 9..].trim_start_matches(' ');
                let name = if rest.starts_with('"') {
                    rest[1..].split('"').next().unwrap_or("")
                } else {
                    rest.split(';').next().unwrap_or("").trim()
                };
                if !name.is_empty() {
                    return sanitize_filename(name);
                }
            }
        }
    }
    get_filename_from_url(url)
}

// ── Engine state ─────────────────────────────────────────────────────────────

pub struct DownloadEngine {
    pub state: Arc<Mutex<EngineState>>,
    pub config: Arc<Mutex<AppConfig>>,
    pub app_handle: AppHandle,
    pub client: reqwest::Client,
    pub db_path: PathBuf,
    pub queue_notify: Arc<tokio::sync::Notify>,
}

impl DownloadEngine {
    pub fn new(config: AppConfig, app_handle: AppHandle) -> Arc<Self> {
        let db_path = PathBuf::from(&config.download_dir).join(".qdm_data");
        std::fs::create_dir_all(&db_path).ok();

        let engine = Arc::new(Self {
            state: Arc::new(Mutex::new(EngineState {
                downloads: HashMap::new(),
                active_tokens: HashMap::new(),
                pending_queue: Vec::new(),
                active_count: 0,
            })),
            config: Arc::new(Mutex::new(config)),
            app_handle,
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
                .redirect(reqwest::redirect::Policy::limited(10))
                // Raised from 30s — large segment reads need time
                .timeout(Duration::from_secs(60))
                .connect_timeout(Duration::from_secs(15))
                // Force HTTP/1.1: avoids HTTP/2 multiplexing all segments over one TCP
                // connection, which defeats the purpose of parallel byte-range requests.
                .http1_only()
                // Tune connection pool for multi-segment downloads
                .pool_max_idle_per_host(16)
                .pool_idle_timeout(Duration::from_secs(90))
                .tcp_keepalive(Duration::from_secs(30))
                .tcp_nodelay(true)
                .connection_verbose(false)
                .build()
                .expect("Failed to build HTTP client"),
            db_path,
            queue_notify: Arc::new(tokio::sync::Notify::new()),
        });

        // Load saved state synchronously
        engine.load_state_sync();

        // Spawn background queue processor
        let eng = Arc::clone(&engine);
        tauri::async_runtime::spawn(async move {
            loop {
                eng.queue_notify.notified().await;
                loop {
                    let (max, next_id) = {
                        let config = eng.config.lock().await;
                        let max = config.max_concurrent_downloads;
                        drop(config);
                        let mut state = eng.state.lock().await;
                        if state.active_count >= max || state.pending_queue.is_empty() {
                            break;
                        }
                        let next = state.pending_queue.remove(0);
                        let still_queued = state
                            .downloads
                            .get(&next)
                            .map(|i| i.status == DownloadStatus::Queued)
                            .unwrap_or(false);
                        if still_queued {
                            (max, next)
                        } else {
                            continue;
                        }
                    };
                    let _ = max;
                    let engine2 = Arc::clone(&eng);
                    tauri::async_runtime::spawn(async move {
                        engine2.start_download_inner(next_id).await;
                    });
                }
            }
        });

        engine
    }

    fn load_state_sync(&self) {
        let state_file = self.db_path.join("downloads.json");
        if let Ok(content) = std::fs::read_to_string(&state_file) {
            if let Ok(mut items) = serde_json::from_str::<Vec<DownloadItem>>(&content) {
                // Use try_lock since we're in a sync context during setup
                if let Ok(mut state) = self.state.try_lock() {
                    for item in &mut items {
                        if item.status == DownloadStatus::Downloading
                            || item.status == DownloadStatus::Assembling
                        {
                            item.status = DownloadStatus::Paused;
                            item.speed = 0.0;
                            item.eta = 0;
                        }
                        state.downloads.insert(item.id.clone(), item.clone());
                    }
                }
            }
        }
    }

    pub async fn save_state(&self) {
        // Serialise while holding the lock, then release it before the I/O so
        // we don't block the tokio worker pool on a disk write.
        let json = {
            let state = self.state.lock().await;
            let items: Vec<&DownloadItem> = state.downloads.values().collect();
            serde_json::to_string_pretty(&items).ok()
        };
        if let Some(json) = json {
            let state_file = self.db_path.join("downloads.json");
            tokio::fs::write(&state_file, json).await.ok();
        }
    }

    pub fn emit<S: serde::Serialize + Clone>(&self, event: &str, payload: S) {
        self.app_handle.emit(event, payload).ok();
    }

    // ── URL Probe ─────────────────────────────────────────────────────────

    pub async fn probe_url(
        &self,
        url: &str,
        extra_headers: Option<&HashMap<String, String>>,
    ) -> Result<ProbeResult, String> {
        // First attempt: HEAD request (lightweight, no body)
        match self.probe_via_head(url, extra_headers).await {
            Ok(probe) if probe.file_size > 0 || probe.resumable => return Ok(probe),
            _ => {}
        }
        // Fallback: some CDNs (CloudFront, S3 signed URLs) reject HEAD with 403/405.
        // Send GET Range:bytes=0-0 instead — 206 response confirms resumability.
        self.probe_via_range_get(url, extra_headers).await
    }

    async fn probe_via_head(
        &self,
        url: &str,
        extra_headers: Option<&HashMap<String, String>>,
    ) -> Result<ProbeResult, String> {
        let mut req = self.client.head(url);
        if let Some(hdrs) = extra_headers {
            for (k, v) in hdrs {
                req = req.header(k.as_str(), v.as_str());
            }
        }

        let response = req.send().await.map_err(|e| e.to_string())?;
        let status = response.status();

        if status.is_redirection() {
            if let Some(location) = response.headers().get("location") {
                if let Ok(new_url) = location.to_str() {
                    let absolute = url::Url::parse(url)
                        .and_then(|base| base.join(new_url))
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| new_url.to_string());
                    return Box::pin(self.probe_url(&absolute, extra_headers)).await;
                }
            }
        }

        if status.is_client_error() || status.is_server_error() {
            return Err(format!("HTTP {}", status));
        }

        let file_size = response
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(-1);

        let resumable = response
            .headers()
            .get("accept-ranges")
            .and_then(|v| v.to_str().ok())
            .map(|v| v == "bytes")
            .unwrap_or(false)
            && file_size > 0;

        let file_name = get_filename_from_headers(response.headers(), url);
        let final_url = response.url().to_string();

        Ok(ProbeResult { file_size, resumable, file_name, final_url, error: None })
    }

    async fn probe_via_range_get(
        &self,
        url: &str,
        extra_headers: Option<&HashMap<String, String>>,
    ) -> Result<ProbeResult, String> {
        let mut req = self.client.get(url).header("Range", "bytes=0-0");
        if let Some(hdrs) = extra_headers {
            for (k, v) in hdrs {
                req = req.header(k.as_str(), v.as_str());
            }
        }

        let response = req.send().await.map_err(|e| e.to_string())?;
        let status = response.status();

        // 206 = server honours ranges; anything else means non-resumable
        let resumable = status.as_u16() == 206;

        // Content-Range: bytes 0-0/TOTAL — extract TOTAL
        let file_size = response
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .map(parse_content_range_total)
            .unwrap_or_else(|| {
                // Fallback: Content-Length from a 200 response
                response
                    .headers()
                    .get(header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(-1)
            });

        let file_name = get_filename_from_headers(response.headers(), url);
        let final_url = response.url().to_string();

        Ok(ProbeResult { file_size, resumable, file_name, final_url, error: None })
    }

    // ── Add Download ──────────────────────────────────────────────────────

    pub async fn add_download(
        self: &Arc<Self>,
        request: NewDownloadRequest,
    ) -> Result<DownloadItem, String> {
        // Reject relative URLs — reqwest can't request them and they indicate
        // a misclassified web resource (JS, audio notification, etc.)
        if !request.url.starts_with("http://") && !request.url.starts_with("https://") {
            return Err(format!("Relative URL rejected: {}", &request.url));
        }

        let mut file_size = -1i64;
        let mut resumable = false;

        // For yt-dlp URLs, skip probing (it's an HTML page) and derive a sensible
        // placeholder from the video ID; yt-dlp will set the real name after download.
        let is_ytdlp = crate::yt_dlp::is_yt_dlp_url(&request.url);
        let mut file_name = if is_ytdlp {
            request.file_name.clone().unwrap_or_else(|| {
                // Extract video ID from URL for a readable placeholder
                let id = request.url
                    .find("v=").map(|i| &request.url[i+2..])
                    .or_else(|| {
                        let lower = request.url.to_lowercase();
                        if let Some(pos) = lower.find("youtu.be/") {
                            Some(&request.url[pos+9..])
                        } else if let Some(pos) = lower.find("/shorts/") {
                            Some(&request.url[pos+8..])
                        } else if let Some(pos) = lower.find("/live/") {
                            Some(&request.url[pos+6..])
                        } else {
                            None
                        }
                    })
                    .map(|s| s.split(|c| c == '&' || c == '?' || c == '/').next().unwrap_or(s))
                    .unwrap_or("video");
                format!("{}.mp4", id)
            })
        } else {
            // Always sanitize the filename — it may come from a tab title containing
            // Windows-invalid characters like `|`, `:`, `?`, etc.
            sanitize_filename(
                &request.file_name.clone().unwrap_or_else(|| get_filename_from_url(&request.url))
            )
        };

        if !is_ytdlp {
            match self.probe_url(&request.url, request.headers.as_ref()).await {
                Ok(probe) => {
                    file_size = probe.file_size;
                    resumable = probe.resumable;
                    if request.file_name.is_none() && !probe.file_name.is_empty() {
                        file_name = sanitize_filename(&probe.file_name);
                    }
                }
                Err(e) => log::warn!("Probe failed for {}: {}", &request.url, e),
            }
        }

        let (base_save_path, max_segs) = {
            let config = self.config.lock().await;
            (
                request
                    .save_path
                    .clone()
                    .unwrap_or_else(|| config.download_dir.clone()),
                request
                    .max_segments
                    .unwrap_or(config.max_segments_per_download),
            )
        };

        // Route into category subfolder (only when using the default download dir).
        // Use two separate joins to guarantee OS-native path separators.
        let category = get_category(&file_name);
        let save_path = if request.save_path.is_none() {
            PathBuf::from(&base_save_path)
                .join("QDM")
                .join(category_subfolder_name(&category))
                .to_string_lossy()
                .to_string()
        } else {
            base_save_path
        };

        std::fs::create_dir_all(&save_path).ok();

        // Ensure the filename is unique within the save directory
        file_name = unique_filename(&save_path, &file_name);

        // Strip Cookie from the persisted headers so expired session cookies
        // never survive an app restart.  The cookie is kept in `runtime_cookie`
        // (a #[serde(skip)] field) and re-injected into the segment request at
        // download time.
        let (item_headers, runtime_cookie) = {
            let mut hdrs = request.headers.clone().unwrap_or_default();
            let cookie = hdrs.remove("Cookie").or_else(|| hdrs.remove("cookie"));
            let opt = if hdrs.is_empty() { None } else { Some(hdrs) };
            (opt, cookie)
        };

        let item = DownloadItem {
            id: generate_id(),
            url: request.url.clone(),
            file_name: file_name.clone(),
            file_size,
            downloaded: 0,
            progress: 0.0,
            speed: 0.0,
            eta: 0,
            status: DownloadStatus::Queued,
            category: get_category(&file_name),
            date_added: Utc::now().to_rfc3339(),
            date_completed: None,
            save_path,
            resumable,
            segments: Vec::new(),
            max_segments: max_segs,
            error: None,
            headers: item_headers,
            runtime_cookie,
            runtime_auth: None,
            source_page_url: request.source_page_url.clone(),
            ytdlp_quality: request.ytdlp_quality.clone(),
            ytdlp_cookies: request.ytdlp_cookies.clone(),
        };

        let id = item.id.clone();
        {
            let mut state = self.state.lock().await;
            state.downloads.insert(id.clone(), item.clone());
        }
        self.save_state().await;
        self.emit("download:added", &item);

        let auto_start = request.auto_start.unwrap_or(true);
        if auto_start {
            let engine = Arc::clone(self);
            tauri::async_runtime::spawn(async move {
                engine.start_download_inner(id).await;
            });
        }

        Ok(item)
    }

    // ── Start Download ────────────────────────────────────────────────────

    pub async fn start_download(self: &Arc<Self>, id: &str) -> Option<DownloadItem> {
        let item = {
            let state = self.state.lock().await;
            state.downloads.get(id).cloned()
        };

        if item.is_none() {
            return None;
        }
        let item = item.unwrap();
        if item.status == DownloadStatus::Downloading {
            return Some(item);
        }

        let engine = Arc::clone(self);
        let id_owned = id.to_string();
        tauri::async_runtime::spawn(async move {
            engine.start_download_inner(id_owned).await;
        });

        let state = self.state.lock().await;
        state.downloads.get(id).cloned()
    }

    async fn start_download_inner(self: Arc<Self>, id: String) {
        // Check concurrency
        let max_concurrent = {
            let config = self.config.lock().await;
            config.max_concurrent_downloads
        };

        let should_queue = {
            let state = self.state.lock().await;
            state.active_count >= max_concurrent
        };

        if should_queue {
            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(&id) {
                item.status = DownloadStatus::Queued;
            }
            if !state.pending_queue.contains(&id) {
                state.pending_queue.push(id.clone());
            }
            return;
        }

        // Initialize segments if needed
        {
            let mut state = self.state.lock().await;
            let item = match state.downloads.get_mut(&id) {
                Some(i) => i,
                None => return,
            };

            if item.segments.is_empty() {
                item.segments = Self::init_segments(item);
            }

            item.status = DownloadStatus::Downloading;
            item.error = None;
            state.active_count += 1;
        }

        let cancel_token = CancellationToken::new();
        {
            let mut state = self.state.lock().await;
            state
                .active_tokens
                .insert(id.clone(), cancel_token.clone());
        }

        self.emit("download:started", serde_json::json!({ "id": &id }));

        // Get item snapshot for spawning segment tasks
        let (url, headers, resumable, segments, db_path, speed_limit) = {
            let state = self.state.lock().await;
            let item = match state.downloads.get(&id) {
                Some(i) => i.clone(),
                None => return,
            };
            let speed_limit = {
                drop(state);
                let config = self.config.lock().await;
                config.speed_limit
            };
            let state = self.state.lock().await;
            let item2 = state.downloads.get(&id).unwrap();
            // Re-inject runtime-only auth headers (Cookie, Authorization) so
            // segments can authenticate.  These fields are #[serde(skip)] and
            // are never written to downloads.json.
            let mut hdrs = item2.headers.clone().unwrap_or_default();
            if let Some(cookie) = &item2.runtime_cookie {
                hdrs.insert("Cookie".to_string(), cookie.clone());
            }
            if let Some(auth) = &item2.runtime_auth {
                hdrs.insert("Authorization".to_string(), auth.clone());
            }
            let headers_with_cookie = if hdrs.is_empty() { None } else { Some(hdrs) };
            (
                item2.url.clone(),
                headers_with_cookie,
                item2.resumable,
                item2.segments.clone(),
                self.db_path.clone(),
                speed_limit,
            )
        };

        let temp_dir = db_path.join(&id);
        std::fs::create_dir_all(&temp_dir).ok();

        // Progress reporting task
        let engine_prog = Arc::clone(&self);
        let id_prog = id.clone();
        let cancel_prog = cancel_token.clone();
        let prog_handle = tauri::async_runtime::spawn(async move {
            let mut last_bytes = 0u64;
            let mut last_time = Instant::now();
            let mut speed_ema = 0f64;

            loop {
                tokio::select! {
                    _ = cancel_prog.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                }

                let (downloaded, file_size, status, segs, state_progress, state_eta) = {
                    let state = engine_prog.state.lock().await;
                    match state.downloads.get(&id_prog) {
                        Some(item) => (
                            item.downloaded,
                            item.file_size,
                            item.status.clone(),
                            item.segments.clone(),
                            item.progress,
                            item.eta,
                        ),
                        None => break,
                    }
                };

                if status != DownloadStatus::Downloading {
                    break;
                }

                let elapsed = last_time.elapsed().as_secs_f64();
                if elapsed > 0.0 {
                    let instant_speed = (downloaded.saturating_sub(last_bytes)) as f64 / elapsed;
                    speed_ema = if speed_ema > 0.0 {
                        0.3 * instant_speed + 0.7 * speed_ema
                    } else {
                        instant_speed
                    };
                    speed_ema = speed_ema.max(0.0);
                }
                last_bytes = downloaded;
                last_time = Instant::now();

                // If file_size is known, calculate progress from bytes; otherwise use
                // whatever the yt-dlp stdout task already wrote into state.
                let (progress, eta) = if file_size > 0 {
                    let prog = ((downloaded as f64 / file_size as f64) * 100.0)
                        .min(100.0)
                        .floor();
                    let eta = if speed_ema > 0.0 {
                        ((file_size as u64).saturating_sub(downloaded) as f64 / speed_ema).ceil()
                            as i64
                    } else {
                        0
                    };
                    (prog, eta)
                } else {
                    (state_progress, state_eta)
                };

                // Update item with calculated speed; only override progress/eta when
                // file_size is known (otherwise yt-dlp's own progress values are correct).
                {
                    let mut state = engine_prog.state.lock().await;
                    if let Some(item) = state.downloads.get_mut(&id_prog) {
                        item.speed = speed_ema;
                        if file_size > 0 {
                            item.progress = progress;
                            item.eta = eta;
                        }
                    }
                }

                let prog = DownloadProgress {
                    id: id_prog.clone(),
                    downloaded,
                    progress,
                    speed: speed_ema,
                    eta,
                    segments: segs,
                    status: DownloadStatus::Downloading,
                };
                engine_prog.emit("download:progress", &prog);
            }
        });

        // ── yt-dlp branch ─────────────────────────────────────────────────────
        if yt_dlp::is_yt_dlp_url(&url) {
            let (save_path_yt, runtime_cookie_yt, quality_yt, ytdlp_cookies_yt) = {
                let state = self.state.lock().await;
                let item = state.downloads.get(&id);
                (
                    item.map(|i| i.save_path.clone()).unwrap_or_default(),
                    item.and_then(|i| i.runtime_cookie.clone()),
                    item.and_then(|i| i.ytdlp_quality.clone()),
                    item.and_then(|i| i.ytdlp_cookies.clone()),
                )
            };

            // ── Locate managed tools ─────────────────────────────────────
            let tools_dir = crate::tools::tools_dir(&self.app_handle);

            // Write Netscape cookies to a temp file so yt-dlp can use --cookies FILE.
            // This is more reliable than --add-headers Cookie: for YouTube auth.
            let cookies_file: Option<std::path::PathBuf> = if let Some(ref c) = ytdlp_cookies_yt {
                let path = tools_dir.join(format!("{}_cookies.txt", id));
                let _ = tokio::fs::write(&path, c.as_bytes()).await;
                Some(path)
            } else {
                None
            };
            let managed_ytdlp = crate::tools::ytdlp_bin(&tools_dir);
            let managed_ffmpeg = crate::tools::ffmpeg_bin(&tools_dir);

            // ffmpeg_dir — point yt-dlp at our managed binary if present
            let ffmpeg_dir = if managed_ffmpeg.is_file() {
                Some(tools_dir.clone())
            } else {
                None
            };

            // Find yt-dlp: managed > user path > PATH > common locations
            let resource_dir = self.app_handle.path().resource_dir().ok();
            let ytdlp_bin = {
                let config = self.config.lock().await;
                if managed_ytdlp.is_file() {
                    Some(managed_ytdlp)
                } else {
                    yt_dlp::find_yt_dlp(
                        if config.ytdlp_path.is_empty() { None } else { Some(&config.ytdlp_path) },
                        resource_dir.as_deref(),
                    )
                }
            };

            let yt_result = match ytdlp_bin {
                None => Err("yt-dlp not found. Open Settings → Tools and click Install.".to_string()),
                Some(bin) => {
                    yt_dlp::run_yt_dlp(
                        bin,
                        id.clone(),
                        url.clone(),
                        save_path_yt,
                        runtime_cookie_yt,
                        quality_yt,
                        ffmpeg_dir,
                        cookies_file.clone(),
                        cancel_token.clone(),
                        Arc::clone(&self.state),
                        self.app_handle.clone(),
                    )
                    .await
                }
            };

            // Clean up temp cookies file
            if let Some(ref path) = cookies_file {
                tokio::fs::remove_file(path).await.ok();
            }

            cancel_token.cancel();
            prog_handle.await.ok();

            let final_status = {
                let state = self.state.lock().await;
                state.downloads.get(&id).map(|i| i.status.clone()).unwrap_or(DownloadStatus::Failed)
            };

            if final_status == DownloadStatus::Paused || final_status == DownloadStatus::Stopped {
                let mut state = self.state.lock().await;
                state.active_count = state.active_count.saturating_sub(1);
                state.active_tokens.remove(&id);
                drop(state);
                self.save_state().await;
                self.emit_progress(&id).await;
            } else {
                match yt_result {
                    Ok((fname, total_bytes)) => {
                        // Update file_name with what yt-dlp determined
                        if !fname.is_empty() {
                            let mut st = self.state.lock().await;
                            if let Some(item) = st.downloads.get_mut(&id) {
                                item.file_name = fname;
                            }
                        }
                        self.finalize_hls_success(&id, total_bytes).await;
                    }
                    Err(e) => self.finalize_hls_failed(&id, e).await,
                }
            }
            self.queue_notify.notify_one();
            return;
        }

        // ── DASH branch ───────────────────────────────────────────────────────
        if hls_engine::is_dash_url(&url) {
            let (save_path_dash, file_name_dash) = {
                let state = self.state.lock().await;
                let item = state.downloads.get(&id).unwrap();
                (item.save_path.clone(), item.file_name.clone())
            };
            let mp4_name = hls_engine::dash_output_filename(&file_name_dash);
            if mp4_name != file_name_dash {
                let mut state = self.state.lock().await;
                if let Some(item) = state.downloads.get_mut(&id) {
                    item.file_name = mp4_name.clone();
                }
            }
            let dash_output = PathBuf::from(&save_path_dash).join(&mp4_name);
            let dash_temp = temp_dir.join("dash_segs");

            let dash_result = hls_engine::run_dash(
                self.client.clone(),
                id.clone(),
                url.clone(),
                headers,
                dash_output,
                dash_temp,
                cancel_token.clone(),
                Arc::clone(&self.state),
            )
            .await;

            cancel_token.cancel();
            prog_handle.await.ok();

            let final_status = {
                let state = self.state.lock().await;
                state
                    .downloads
                    .get(&id)
                    .map(|i| i.status.clone())
                    .unwrap_or(DownloadStatus::Failed)
            };

            if final_status == DownloadStatus::Paused
                || final_status == DownloadStatus::Stopped
            {
                let mut state = self.state.lock().await;
                state.active_count = state.active_count.saturating_sub(1);
                state.active_tokens.remove(&id);
                drop(state);
                self.save_state().await;
                self.emit_progress(&id).await;
            } else {
                match dash_result {
                    Ok(total_bytes) => self.finalize_hls_success(&id, total_bytes).await,
                    Err(e) => self.finalize_hls_failed(&id, e).await,
                }
            }
            self.queue_notify.notify_one();
            return;
        }

        // ── HLS branch ────────────────────────────────────────────────────────
        // Detect .m3u8 manifest URLs and route them through the HLS engine
        // instead of the multi-segment HTTP-Range downloader.
        if hls_engine::is_hls_url(&url) {
            let (save_path_hls, file_name_hls) = {
                let state = self.state.lock().await;
                let item = state.downloads.get(&id).unwrap();
                (item.save_path.clone(), item.file_name.clone())
            };
            // Rename .m3u8 → .ts so the output is a proper TS container
            let ts_name = hls_engine::hls_output_filename(&file_name_hls);
            if ts_name != file_name_hls {
                let mut state = self.state.lock().await;
                if let Some(item) = state.downloads.get_mut(&id) {
                    item.file_name = ts_name.clone();
                }
            }
            let hls_output = PathBuf::from(&save_path_hls).join(&ts_name);
            let hls_temp = temp_dir.join("hls_segs");

            let hls_result = hls_engine::run_hls(
                self.client.clone(),
                id.clone(),
                url.clone(),
                headers,
                hls_output,
                hls_temp,
                cancel_token.clone(),
                Arc::clone(&self.state),
            )
            .await;

            // Stop progress reporter before touching state
            cancel_token.cancel();
            prog_handle.await.ok();

            // Check if the download was paused/stopped externally
            let final_status = {
                let state = self.state.lock().await;
                state
                    .downloads
                    .get(&id)
                    .map(|i| i.status.clone())
                    .unwrap_or(DownloadStatus::Failed)
            };

            if final_status == DownloadStatus::Paused
                || final_status == DownloadStatus::Stopped
            {
                // Paused/stopped by the user — just clean up the active slot
                let mut state = self.state.lock().await;
                state.active_count = state.active_count.saturating_sub(1);
                state.active_tokens.remove(&id);
                drop(state);
                self.save_state().await;
                self.emit_progress(&id).await;
            } else {
                match hls_result {
                    Ok(total_bytes) => self.finalize_hls_success(&id, total_bytes).await,
                    Err(e) => self.finalize_hls_failed(&id, e).await,
                }
            }
            self.queue_notify.notify_one();
            return;
        }

        // ── Segment resume validation ─────────────────────────────────────────
        // On app restart, a segment that was Downloading (state=1) when the app
        // crashed may have fewer bytes on disk than `seg.downloaded` records.
        // Truncate the in-memory counter to match the actual file size so the
        // byte-range request resumes from the right offset.
        let mut segments = segments;
        for seg in &mut segments {
            if seg.state == 1 {
                let part_path = temp_dir.join(format!("{}.part", seg.id));
                let actual = tokio::fs::metadata(&part_path)
                    .await
                    .map(|m| m.len())
                    .unwrap_or(0);
                if actual < seg.downloaded {
                    log::warn!(
                        "[{}] seg {} on-disk bytes ({}) < recorded ({}); resetting to {}",
                        id, seg.id, actual, seg.downloaded, actual
                    );
                    // Update both the local copy and the persistent state
                    seg.downloaded = actual;
                    let mut state = self.state.lock().await;
                    if let Some(item) = state.downloads.get_mut(&id) {
                        if let Some(s) = item.segments.iter_mut().find(|s| s.id == seg.id) {
                            s.downloaded = actual;
                        }
                    }
                }
            }
        }

        // Download all pending segments
        let pending_segs: Vec<DownloadSegment> =
            segments.into_iter().filter(|s| s.state != 2).collect();

        let results = self
            .run_segments(&id, pending_segs, &url, &headers, resumable, &temp_dir, &cancel_token, speed_limit)
            .await;

        // Stop progress reporter
        cancel_token.cancel();
        prog_handle.await.ok();

        // Check if paused/stopped before proceeding
        let current_status = {
            let state = self.state.lock().await;
            state
                .downloads
                .get(&id)
                .map(|i| i.status.clone())
                .unwrap_or(DownloadStatus::Stopped)
        };

        if current_status == DownloadStatus::Paused || current_status == DownloadStatus::Stopped {
            self.finalize_download(&id, results).await;
            return;
        }

        self.finalize_download(&id, results).await;

        // Signal the background queue processor to check for pending downloads
        self.queue_notify.notify_one();
    }

    /// Run segments with IDM-style dynamic splitting.
    ///
    /// A mpsc channel carries completion signals back to the driver loop.
    /// On each completion `try_split_segment` is called: if the largest
    /// not-yet-started segment has ≥ 8 MB remaining it is halved and the
    /// second half is spawned as a new task immediately.
    /// Only NotStarted (state=0) segments are split, so byte ranges never
    /// overlap and the assembly step remains correct.
    async fn run_segments(
        &self,
        id: &str,
        segments: Vec<DownloadSegment>,
        url: &str,
        headers: &Option<HashMap<String, String>>,
        resumable: bool,
        temp_dir: &Path,
        cancel_token: &CancellationToken,
        speed_limit: u64,
    ) -> bool {
        // Channel: tasks send (seg_id, succeeded) on completion.
        // We keep `tx` alive for the duration of the driver loop so `rx.recv()`
        // never spuriously returns None; we exit via `pending_count == 0` instead.
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(String, bool)>(64);
        let mut pending_count: usize = 0;

        // Captures for the inner spawn helper
        let client_ref = &self.client;
        let state_ref = &self.state;

        // Spawn a single segment task and increment pending_count
        macro_rules! spawn_one {
            ($seg:expr) => {{
                let seg: DownloadSegment = $seg;
                let client = client_ref.clone();
                let url_s = url.to_string();
                let hdrs = headers.clone();
                let td = temp_dir.to_path_buf();
                let ct = cancel_token.clone();
                let sa = Arc::clone(state_ref);
                let iid = id.to_string();
                let seg_id = seg.id.clone();
                let tx2 = tx.clone();

                tauri::async_runtime::spawn(async move {
                    let ok = download_segment_with_retry(
                        client, url_s, hdrs, seg, resumable, td, ct,
                        sa.clone(), iid.clone(), speed_limit,
                    )
                    .await;

                    let mut st = sa.lock().await;
                    if let Some(item) = st.downloads.get_mut(&iid) {
                        if let Some(s) = item.segments.iter_mut().find(|s| s.id == seg_id) {
                            match ok {
                                Ok(_) => s.state = 2,
                                Err(ref e) => {
                                    if s.state != 2 { s.state = 3; }
                                    // Preserve auth_required errors for finalize_download
                                    // to surface to the user; don't overwrite with a later
                                    // generic error.
                                    if item.error.is_none() {
                                        item.error = Some(e.clone());
                                    }
                                }
                            }
                        }
                    }
                    drop(st);
                    tx2.send((seg_id, ok.is_ok())).await.ok();
                });

                pending_count += 1;
            }};
        }

        // Start all initial segments
        for seg in segments {
            spawn_one!(seg);
        }

        // Driver loop — wait for completions and fire dynamic splits
        while pending_count > 0 {
            match rx.recv().await {
                None => break, // channel closed unexpectedly
                Some((_seg_id, _ok)) => {
                    pending_count -= 1;

                    // Try to grow parallelism by splitting the largest pending segment
                    let split = {
                        let mut st = self.state.lock().await;
                        st.downloads.get_mut(id).and_then(try_split_segment)
                    };
                    if let Some(new_seg) = split {
                        {
                            let mut st = self.state.lock().await;
                            if let Some(item) = st.downloads.get_mut(id) {
                                item.segments.push(new_seg.clone());
                            }
                        }
                        spawn_one!(new_seg);
                    }
                }
            }
        }

        let state = self.state.lock().await;
        state
            .downloads
            .get(id)
            .map(|item| item.segments.iter().all(|s| s.state == 2))
            .unwrap_or(false)
    }

    async fn finalize_download(&self, id: &str, all_done: bool) {
        // Decrement active count and clean up token
        {
            let mut state = self.state.lock().await;
            state.active_count = state.active_count.saturating_sub(1);
            state.active_tokens.remove(id);
        }

        let current_status = {
            let state = self.state.lock().await;
            state
                .downloads
                .get(id)
                .map(|i| i.status.clone())
                .unwrap_or(DownloadStatus::Failed)
        };

        if current_status == DownloadStatus::Paused || current_status == DownloadStatus::Stopped {
            self.save_state().await;
            self.emit_progress(id).await;
            return;
        }

        if all_done {
            // Assemble file
            let (segments, save_path, file_name, db_path, file_size) = {
                let state = self.state.lock().await;
                let item = state.downloads.get(id).unwrap();
                (
                    item.segments.clone(),
                    item.save_path.clone(),
                    item.file_name.clone(),
                    self.db_path.clone(),
                    item.file_size,
                )
            };

            let temp_dir = db_path.join(id);
            let output_path = PathBuf::from(&save_path).join(&file_name);
            let expected_size = if file_size > 0 { Some(file_size) } else { None };

            let assemble_result = if segments.len() == 1 {
                let seg_file = temp_dir.join(format!("{}.part", segments[0].id));
                if seg_file.exists() {
                    // Try rename first (fast, same filesystem); fall back to copy+delete
                    // when source and destination are on different volumes.
                    match tokio::fs::rename(&seg_file, &output_path).await {
                        Ok(()) => Ok(()),
                        Err(_) => {
                            tokio::fs::copy(&seg_file, &output_path)
                                .await
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                                .map(|_| {
                                    let td = temp_dir.clone();
                                    tauri::async_runtime::spawn(async move {
                                        tokio::fs::remove_dir_all(&td).await.ok();
                                    });
                                })
                        }
                    }
                } else {
                    Err("Segment file not found".to_string())
                }
            } else {
                {
                    let mut state = self.state.lock().await;
                    if let Some(item) = state.downloads.get_mut(id) {
                        item.status = DownloadStatus::Assembling;
                    }
                }
                self.emit_progress(id).await;
                assemble_segments_verified(&segments, &temp_dir, &output_path, expected_size).await
            };

            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(id) {
                match assemble_result {
                    Ok(_) => {
                        item.status = DownloadStatus::Completed;
                        item.progress = 100.0;
                        item.speed = 0.0;
                        item.eta = 0;
                        item.date_completed = Some(Utc::now().to_rfc3339());
                        let completed_item = item.clone();
                        drop(state);
                        self.emit("download:completed", &completed_item);
                        if {
                            let config = self.config.try_lock().ok();
                            config.map(|c| c.show_notifications).unwrap_or(true)
                        } {
                            self.emit(
                                "download:notify",
                                serde_json::json!({
                                    "title": "Download Complete",
                                    "body": completed_item.file_name
                                }),
                            );
                        }
                    }
                    Err(e) => {
                        item.status = DownloadStatus::Failed;
                        item.error = Some(e);
                        let id_str = item.id.clone();
                        let err = item.error.clone();
                        drop(state);
                        self.emit("download:failed", serde_json::json!({ "id": id_str, "error": err }));
                    }
                }
            } else {
                drop(state);
            }
        } else if current_status != DownloadStatus::Paused
            && current_status != DownloadStatus::Stopped
        {
            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(id) {
                // Preserve a specific error (e.g. "auth_required:Basic") that was
                // set by the segment task, falling back to a generic message.
                let specific_err = item.error.take();
                let error_msg = specific_err.clone().unwrap_or_else(|| {
                    "Some segments failed to download".to_string()
                });
                item.status = DownloadStatus::Failed;
                item.error = Some(error_msg.clone());
                let id_str = item.id.clone();
                drop(state);
                self.emit("download:failed", serde_json::json!({ "id": id_str, "error": &error_msg }));
                // Emit a dedicated event so the UI can show a credentials dialog
                if error_msg.starts_with("auth_required:") {
                    let scheme = error_msg.trim_start_matches("auth_required:").to_string();
                    self.emit("download:auth_required", serde_json::json!({
                        "id": id_str,
                        "scheme": scheme,
                    }));
                }
                // Emit a dedicated event so the UI can offer to re-open the source page
                if error_msg == "link_expired" {
                    let source_page = {
                        let state = self.state.lock().await;
                        state.downloads.get(&id_str)
                            .and_then(|i| i.source_page_url.clone())
                    };
                    self.emit("download:link_expired", serde_json::json!({
                        "id": id_str,
                        "sourcePageUrl": source_page,
                    }));
                }
            } else {
                drop(state);
            }
        }

        self.save_state().await;
        self.emit_progress(id).await;
    }

    async fn emit_progress(&self, id: &str) {
        let state = self.state.lock().await;
        if let Some(item) = state.downloads.get(id) {
            let prog = DownloadProgress {
                id: item.id.clone(),
                downloaded: item.downloaded,
                progress: item.progress,
                speed: item.speed,
                eta: item.eta,
                segments: item.segments.clone(),
                status: item.status.clone(),
            };
            drop(state);
            self.emit("download:progress", &prog);
        }
    }

    // ── HLS completion helpers ────────────────────────────────────────────────

    /// Called after `hls_engine::run_hls` succeeds.
    /// Updates state to Completed and emits the standard completion events.
    async fn finalize_hls_success(&self, id: &str, total_bytes: u64) {
        {
            let mut state = self.state.lock().await;
            state.active_count = state.active_count.saturating_sub(1);
            state.active_tokens.remove(id);
        }
        {
            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(id) {
                item.status = DownloadStatus::Completed;
                item.downloaded = total_bytes;
                item.file_size = total_bytes as i64;
                item.progress = 100.0;
                item.speed = 0.0;
                item.eta = 0;
                item.date_completed = Some(Utc::now().to_rfc3339());
                let completed_item = item.clone();
                drop(state);
                self.emit("download:completed", &completed_item);
                if self.config.try_lock().map(|c| c.show_notifications).unwrap_or(true) {
                    self.emit(
                        "download:notify",
                        serde_json::json!({
                            "title": "Download complete",
                            "body": &completed_item.file_name,
                        }),
                    );
                }
            } else {
                drop(state);
            }
        }
        self.save_state().await;
        self.emit_progress(id).await;
    }

    /// Called after `hls_engine::run_hls` returns an error (and not paused/stopped).
    async fn finalize_hls_failed(&self, id: &str, error: String) {
        {
            let mut state = self.state.lock().await;
            state.active_count = state.active_count.saturating_sub(1);
            state.active_tokens.remove(id);
        }
        {
            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(id) {
                item.status = DownloadStatus::Failed;
                item.error = Some(error.clone());
                let id_str = item.id.clone();
                drop(state);
                self.emit("download:failed", serde_json::json!({ "id": id_str, "error": error }));
            } else {
                drop(state);
            }
        }
        self.save_state().await;
        self.emit_progress(id).await;
    }

    fn init_segments(item: &DownloadItem) -> Vec<DownloadSegment> {
        if item.file_size <= 0 || !item.resumable {
            return vec![DownloadSegment {
                id: generate_id(),
                offset: 0,
                length: item.file_size,
                downloaded: 0,
                state: 0,
                speed: 0.0,
            }];
        }

        let min_seg_size = 512 * 1024u64; // 512 KB minimum per segment
        let seg_count = (item.max_segments as u64)
            .min((item.file_size as u64 / min_seg_size).max(1)) as usize;
        let seg_size = item.file_size as u64 / seg_count as u64;

        (0..seg_count)
            .map(|i| {
                let offset = i as u64 * seg_size;
                let length = if i == seg_count - 1 {
                    item.file_size as u64 - offset
                } else {
                    seg_size
                } as i64;
                DownloadSegment {
                    id: generate_id(),
                    offset,
                    length,
                    downloaded: 0,
                    state: 0,
                    speed: 0.0,
                }
            })
            .collect()
    }

    // ── Pause / Resume / Cancel / Remove / Retry ──────────────────────────

    pub async fn pause_download(&self, id: &str) -> Option<DownloadItem> {
        let token = {
            let state = self.state.lock().await;
            state.active_tokens.get(id).cloned()
        };

        if let Some(token) = token {
            token.cancel();
        }

        let mut state = self.state.lock().await;
        // Remove from pending queue
        state.pending_queue.retain(|i| i != id);

        if let Some(item) = state.downloads.get_mut(id) {
            item.status = DownloadStatus::Paused;
            item.speed = 0.0;
            item.eta = 0;
            let item_clone = item.clone();
            drop(state);
            self.emit("download:paused", &item_clone);
            self.save_state().await;
            Some(item_clone)
        } else {
            None
        }
    }

    pub async fn resume_download(self: &Arc<Self>, id: &str) -> Option<DownloadItem> {
        {
            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(id) {
                if item.status != DownloadStatus::Paused && item.status != DownloadStatus::Failed {
                    return Some(item.clone());
                }
                // Reset failed segments
                for seg in &mut item.segments {
                    if seg.state == 3 {
                        seg.state = 0;
                    }
                }
            }
        }
        self.start_download(id).await
    }

    pub async fn cancel_download(&self, id: &str) -> bool {
        let token = {
            let state = self.state.lock().await;
            state.active_tokens.get(id).cloned()
        };

        if let Some(token) = token {
            token.cancel();
        }

        let mut state = self.state.lock().await;
        state.pending_queue.retain(|i| i != id);

        if let Some(item) = state.downloads.get_mut(id) {
            item.status = DownloadStatus::Stopped;
            item.speed = 0.0;
        }

        let temp_dir = self.db_path.join(id);
        std::fs::remove_dir_all(&temp_dir).ok();

        drop(state);
        self.emit("download:cancelled", serde_json::json!({ "id": id }));
        self.save_state().await;
        true
    }

    pub async fn remove_download(&self, id: &str, delete_file: bool) -> bool {
        self.cancel_download(id).await;

        let file_path = {
            let state = self.state.lock().await;
            state.downloads.get(id).map(|item| {
                PathBuf::from(&item.save_path).join(&item.file_name)
            })
        };

        if delete_file {
            if let Some(path) = file_path {
                std::fs::remove_file(&path).ok();
            }
        }

        let mut state = self.state.lock().await;
        state.downloads.remove(id);
        state.active_tokens.remove(id);
        drop(state);

        self.emit("download:removed", serde_json::json!({ "id": id }));
        self.save_state().await;
        true
    }

    pub async fn retry_download(self: &Arc<Self>, id: &str) -> Option<DownloadItem> {
        {
            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(id) {
                item.segments = Vec::new();
                item.downloaded = 0;
                item.progress = 0.0;
                item.error = None;
            }
        }

        let temp_dir = self.db_path.join(id);
        std::fs::remove_dir_all(&temp_dir).ok();

        self.start_download(id).await
    }

    pub async fn pause_all(&self) {
        let active_ids: Vec<String> = {
            let state = self.state.lock().await;
            state
                .downloads
                .values()
                .filter(|d| d.status == DownloadStatus::Downloading)
                .map(|d| d.id.clone())
                .collect()
        };
        {
            let mut state = self.state.lock().await;
            state.pending_queue.clear();
        }
        for id in active_ids {
            // Cancel all tokens
            let token = {
                let state = self.state.lock().await;
                state.active_tokens.get(&id).cloned()
            };
            if let Some(token) = token {
                token.cancel();
            }
            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(&id) {
                item.status = DownloadStatus::Paused;
                item.speed = 0.0;
                item.eta = 0;
            }
        }
        self.save_state().await;
    }

    pub async fn resume_all(self: &Arc<Self>) {
        let paused_ids: Vec<String> = {
            let state = self.state.lock().await;
            state
                .downloads
                .values()
                .filter(|d| {
                    d.status == DownloadStatus::Paused || d.status == DownloadStatus::Failed
                })
                .map(|d| d.id.clone())
                .collect()
        };
        for id in paused_ids {
            self.start_download(&id).await;
        }
    }

    pub async fn get_all_downloads(&self) -> Vec<DownloadItem> {
        let state = self.state.lock().await;
        let mut items: Vec<DownloadItem> = state.downloads.values().cloned().collect();
        items.sort_by(|a, b| b.date_added.cmp(&a.date_added));
        items
    }

    pub async fn open_file(&self, id: &str) -> bool {
        let (save_path, file_name) = {
            let state = self.state.lock().await;
            match state.downloads.get(id) {
                Some(item) if item.status == DownloadStatus::Completed => {
                    (item.save_path.clone(), item.file_name.clone())
                }
                Some(item) => {
                    log::warn!("[open_file] id={} status={:?} — not completed, refusing", id, item.status);
                    return false;
                }
                None => {
                    log::warn!("[open_file] id={} — not found in state", id);
                    return false;
                }
            }
        };
        let path = PathBuf::from(&save_path).join(&file_name);
        log::info!("[open_file] id={} save_path={} file_name={} → full_path={}",
            id, save_path, file_name, path.display());

        // If file not at expected location, try to find it by scanning the directory
        let path = if !path.exists() {
            log::warn!("[open_file] file not found at '{}', scanning dir for stem", path.display());
            let stem = Path::new(&file_name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&file_name);
            let found = std::fs::read_dir(&save_path)
                .ok()
                .and_then(|mut entries| {
                    entries.find_map(|e| {
                        let e = e.ok()?;
                        let name = e.file_name();
                        let name_str = name.to_str()?;
                        if name_str.starts_with(stem) { Some(e.path()) } else { None }
                    })
                });
            match found {
                Some(p) => {
                    log::info!("[open_file] fallback found: {}", p.display());
                    p
                }
                None => {
                    log::warn!("[open_file] no matching file found in dir '{}' for stem '{}'", save_path, stem);
                    path
                }
            }
        } else {
            log::info!("[open_file] file exists at expected path");
            path
        };

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            // cmd /c start "" "path" opens the file with its default app, handles spaces correctly
            let path_str = path.to_str().unwrap_or("").replace('"', "\"\"");
            let cmd_arg = format!("/c start \"\" \"{}\"", path_str);
            log::info!("[open_file] running: cmd.exe {}", cmd_arg);
            let ok = std::process::Command::new("cmd.exe")
                .creation_flags(0x08000000)
                .raw_arg(&cmd_arg)
                .spawn()
                .is_ok();
            log::info!("[open_file] spawn ok={}", ok);
            ok
        }
        #[cfg(target_os = "macos")]
        {
            let ok = std::process::Command::new("open").arg(&path).spawn().is_ok();
            log::info!("[open_file] open spawn ok={}", ok);
            ok
        }
        #[cfg(target_os = "linux")]
        {
            let ok = std::process::Command::new("xdg-open").arg(&path).spawn().is_ok();
            log::info!("[open_file] xdg-open spawn ok={}", ok);
            ok
        }
    }

    pub async fn open_folder(&self, id: &str) -> bool {
        let (save_path, file_name) = {
            let state = self.state.lock().await;
            match state.downloads.get(id) {
                Some(item) => (item.save_path.clone(), item.file_name.clone()),
                None => {
                    log::warn!("[open_folder] id={} — not found in state", id);
                    return false;
                }
            }
        };
        let file_path = PathBuf::from(&save_path).join(&file_name);
        log::info!("[open_folder] id={} save_path={} file_name={} full_path={} exists={}",
            id, save_path, file_name, file_path.display(), file_path.exists());

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            // raw_arg lets us put inner quotes around the path so Explorer handles spaces correctly
            let arg = if file_path.exists() {
                let p = file_path.to_str().unwrap_or(&save_path).replace('"', "\"\"");
                format!("/select,\"{}\"", p)
            } else {
                // File not found — just open the folder
                log::warn!("[open_folder] file not found, opening folder only");
                format!("\"{}\"", save_path.replace('"', "\"\""))
            };
            log::info!("[open_folder] running: explorer.exe {}", arg);
            let ok = std::process::Command::new("explorer.exe")
                .creation_flags(0x08000000)
                .raw_arg(&arg)
                .spawn()
                .is_ok();
            log::info!("[open_folder] spawn ok={}", ok);
            return ok;
        }
        #[cfg(target_os = "macos")]
        {
            let ok = std::process::Command::new("open")
                .args(["-R", file_path.to_str().unwrap_or(&save_path)])
                .spawn()
                .is_ok();
            log::info!("[open_folder] open -R spawn ok={}", ok);
            return ok;
        }
        #[cfg(target_os = "linux")]
        {
            let ok = std::process::Command::new("xdg-open").arg(&save_path).spawn().is_ok();
            log::info!("[open_folder] xdg-open spawn ok={}", ok);
            return ok;
        }
        #[allow(unreachable_code)]
        false
    }


    pub async fn update_config(&self, config: AppConfig) {
        let mut c = self.config.lock().await;
        *c = config;
    }

    /// Store user-provided credentials for a download that returned 401,
    /// reset all failed segments, and restart the download.
    pub async fn provide_auth(
        self: &Arc<Self>,
        id: &str,
        username: &str,
        password: &str,
    ) -> bool {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(format!("{}:{}", username, password));
        let auth_value = format!("Basic {}", encoded);

        {
            let mut state = self.state.lock().await;
            if let Some(item) = state.downloads.get_mut(id) {
                item.runtime_auth = Some(auth_value);
                item.status = DownloadStatus::Queued;
                item.error = None;
                // Reset all failed/incomplete segments so they retry from scratch
                for seg in &mut item.segments {
                    if seg.state != 2 {
                        seg.state = 0;
                        seg.downloaded = 0;
                    }
                }
            } else {
                return false;
            }
        }

        // Restart the download with the new credentials
        let engine = Arc::clone(self);
        let id_s = id.to_string();
        tauri::async_runtime::spawn(async move {
            engine.start_download_inner(id_s).await;
        });
        true
    }
}

// ── Segment downloader ────────────────────────────────────────────────────────

/// Retry wrapper: retries `download_segment_task` up to SEGMENT_MAX_RETRIES times
/// with exponential backoff (500 ms → 1 s → 2 s … capped at 30 s).
/// On each retry the segment's partial progress is reverted so the next attempt
/// starts fresh from an empty file, avoiding double-counting downloaded bytes.
async fn download_segment_with_retry(
    client: reqwest::Client,
    url: String,
    extra_headers: Option<HashMap<String, String>>,
    segment: DownloadSegment,
    resumable: bool,
    temp_dir: PathBuf,
    cancel_token: CancellationToken,
    state_arc: Arc<Mutex<EngineState>>,
    item_id: String,
    speed_limit: u64,
) -> Result<(), String> {
    let seg_file = temp_dir.join(format!("{}.part", segment.id));
    let mut delay_ms: u64 = 500;

    for attempt in 0..=SEGMENT_MAX_RETRIES {
        if attempt > 0 {
            // Revert any bytes counted in the previous failed attempt so the
            // progress counter stays accurate when we restart the segment.
            {
                let mut state = state_arc.lock().await;
                if let Some(item) = state.downloads.get_mut(&item_id) {
                    if let Some(s) = item.segments.iter_mut().find(|s| s.id == segment.id) {
                        item.downloaded = item.downloaded.saturating_sub(s.downloaded);
                        s.downloaded = 0;
                    }
                }
            }
            tokio::fs::remove_file(&seg_file).await.ok();

            tokio::select! {
                _ = cancel_token.cancelled() => return Err("cancelled".to_string()),
                _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
            }
            delay_ms = (delay_ms * 2).min(30_000);
        }

        // Re-read the segment from state so the downloaded offset is accurate.
        let current_seg = {
            let state = state_arc.lock().await;
            state
                .downloads
                .get(&item_id)
                .and_then(|item| item.segments.iter().find(|s| s.id == segment.id))
                .cloned()
                .unwrap_or_else(|| DownloadSegment { downloaded: 0, ..segment.clone() })
        };

        match download_segment_task(
            client.clone(),
            url.clone(),
            extra_headers.clone(),
            current_seg,
            resumable,
            temp_dir.clone(),
            cancel_token.clone(),
            state_arc.clone(),
            item_id.clone(),
            speed_limit,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(ref e) if e == "cancelled" => return Err(e.clone()),
            Err(e) => {
                if !is_retryable_segment_error(&e) || attempt >= SEGMENT_MAX_RETRIES {
                    return Err(e);
                }
                log::warn!(
                    "Segment {} attempt {}/{} failed: {}. Retrying in {}ms",
                    segment.id,
                    attempt + 1,
                    SEGMENT_MAX_RETRIES,
                    e,
                    delay_ms
                );
            }
        }
    }

    Err("max retries exceeded".to_string())
}

// ── Dynamic segment splitter ──────────────────────────────────────────────────

/// IDM-style dynamic splitter.
///
/// Finds the largest **not-yet-started** (state=0) segment that has at least
/// `MIN_SPLIT_BYTES * 2` bytes remaining, halves it, and returns a new segment
/// covering the second half.  Only NotStarted segments are touched so there is
/// no byte-range overlap with an already-running download.
///
/// The caller is responsible for:
/// 1. Pushing the returned segment into `item.segments`.
/// 2. Spawning a download task for it.
fn try_split_segment(item: &mut DownloadItem) -> Option<DownloadSegment> {
    const MIN_SPLIT_BYTES: u64 = 4 * 1024 * 1024; // 4 MB

    // Count how many segment slots are still available
    let used = item.segments.len() as u32;
    if used >= item.max_segments {
        return None;
    }

    // Find the not-yet-started segment with the most remaining bytes
    let (best_idx, remaining) = item
        .segments
        .iter()
        .enumerate()
        .filter(|(_, s)| s.state == 0 && s.length > 0)
        .map(|(i, s)| (i, (s.length as u64).saturating_sub(s.downloaded)))
        .max_by_key(|&(_, r)| r)?;

    if remaining < MIN_SPLIT_BYTES * 2 {
        return None;
    }

    let split_bytes = remaining / 2;
    let orig = &mut item.segments[best_idx];

    // Shrink the original segment to cover only its first half of remaining
    let new_end_of_orig = orig.offset + orig.downloaded + split_bytes;
    let new_seg_offset = new_end_of_orig;
    let new_seg_length = (orig.offset + orig.length as u64) - new_seg_offset;

    // Update original length
    orig.length = (orig.downloaded + split_bytes) as i64;

    Some(DownloadSegment {
        id: generate_id(),
        offset: new_seg_offset,
        length: new_seg_length as i64,
        downloaded: 0,
        state: 0,
        speed: 0.0,
    })
}

/// Core single-attempt segment download.  Includes a per-segment stall timer:
/// if no data is received for SEGMENT_STALL_TIMEOUT the task returns a
/// retryable error so the retry wrapper can restart it.
async fn download_segment_task(
    client: reqwest::Client,
    url: String,
    extra_headers: Option<HashMap<String, String>>,
    segment: DownloadSegment,
    resumable: bool,
    temp_dir: PathBuf,
    cancel_token: CancellationToken,
    state_arc: Arc<Mutex<EngineState>>,
    item_id: String,
    speed_limit: u64,
) -> Result<(), String> {
    let seg_file = temp_dir.join(format!("{}.part", segment.id));
    let already_downloaded = segment.downloaded;

    // Verify/fix partial file size against what is actually on disk
    let actual_downloaded = if already_downloaded > 0 && seg_file.exists() {
        let file_size = tokio::fs::metadata(&seg_file).await.map(|m| m.len()).unwrap_or(0);
        file_size.min(already_downloaded)
    } else {
        0
    };

    // If segment already fully done, skip
    if segment.length > 0 && actual_downloaded >= segment.length as u64 {
        return Ok(());
    }

    let mut req = client.get(&url);

    if let Some(hdrs) = &extra_headers {
        for (k, v) in hdrs {
            req = req.header(k.as_str(), v.as_str());
        }
    }

    if resumable && segment.length > 0 {
        let start = segment.offset + actual_downloaded;
        let end = segment.offset + segment.length as u64 - 1;
        req = req.header("Range", format!("bytes={}-{}", start, end));
    }

    let response = req.send().await.map_err(|e| e.to_string())?;

    let status = response.status();
    // 401 Unauthorized — surface as a special error so the engine can emit
    // download:auth_required and prompt the user for credentials.
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("auth_required:Basic".to_string());
    }
    // 403/410 on a CDN-signed URL — the link has likely expired.
    if (status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::GONE)
        && is_signed_url(&url)
    {
        return Err("link_expired".to_string());
    }
    if status.is_client_error() || status.is_server_error() {
        return Err(format!("HTTP {}", status));
    }

    let file = if actual_downloaded > 0 {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&seg_file)
            .await
            .map_err(|e| e.to_string())?
    } else {
        tokio::fs::File::create(&seg_file)
            .await
            .map_err(|e| e.to_string())?
    };

    let mut writer = tokio::io::BufWriter::new(file);
    let mut stream = response.bytes_stream();

    // Speed throttle state
    let mut bytes_this_second = 0u64;
    let mut window_start = Instant::now();

    // Stall detection: reset on every received chunk
    let stall_sleep = tokio::time::sleep(SEGMENT_STALL_TIMEOUT);
    tokio::pin!(stall_sleep);

    loop {
        tokio::select! {
            _ = cancel_token.cancelled() => {
                writer.flush().await.ok();
                return Err("cancelled".to_string());
            }
            // No data for SEGMENT_STALL_TIMEOUT → retryable stall error
            _ = &mut stall_sleep => {
                writer.flush().await.ok();
                return Err("segment stalled (no data)".to_string());
            }
            chunk = stream.next() => {
                match chunk {
                    None => break,
                    Some(Err(e)) => {
                        writer.flush().await.ok();
                        return Err(e.to_string());
                    }
                    Some(Ok(data)) => {
                        // Reset stall timer on every successful chunk
                        stall_sleep.as_mut().reset(
                            tokio::time::Instant::now() + SEGMENT_STALL_TIMEOUT
                        );

                        let bytes = data.len() as u64;
                        writer.write_all(&data).await.map_err(|e| e.to_string())?;

                        {
                            let mut state = state_arc.lock().await;
                            if let Some(item) = state.downloads.get_mut(&item_id) {
                                item.downloaded += bytes;
                                if let Some(seg) = item.segments.iter_mut().find(|s| s.id == segment.id) {
                                    seg.downloaded += bytes;
                                }
                            }
                        }

                        // Speed throttling
                        if speed_limit > 0 {
                            bytes_this_second += bytes;
                            let elapsed_ms = window_start.elapsed().as_millis() as u64;

                            if elapsed_ms >= 1000 {
                                bytes_this_second = bytes;
                                window_start = Instant::now();
                            } else {
                                let allowed = speed_limit * elapsed_ms / 1000;
                                if bytes_this_second > allowed {
                                    let overshoot_ms =
                                        (bytes_this_second - allowed) * 1000 / speed_limit;
                                    let sleep_ms = overshoot_ms.min(500);
                                    tokio::time::sleep(Duration::from_millis(sleep_ms)).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    writer.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

// ── File assembly ─────────────────────────────────────────────────────────────

/// Assemble segment `.part` files into the final output file.
/// Verifies each segment's size on disk before copying and checks the total
/// assembled size against `expected_size` (from Content-Length) when known.
async fn assemble_segments_verified(
    segments: &[DownloadSegment],
    temp_dir: &Path,
    output_path: &Path,
    expected_size: Option<i64>,
) -> Result<(), String> {
    let mut sorted = segments.to_vec();
    sorted.sort_by_key(|s| s.offset);

    let output_file = tokio::fs::File::create(output_path)
        .await
        .map_err(|e| format!("Failed to create output file: {}", e))?;
    let mut writer = tokio::io::BufWriter::new(output_file);
    let mut total_bytes: u64 = 0;

    for seg in &sorted {
        let seg_file = temp_dir.join(format!("{}.part", seg.id));

        if !seg_file.exists() {
            return Err(format!("Segment file missing: {}", seg.id));
        }

        // Verify each part's size before copying to catch truncated downloads
        if seg.length > 0 {
            let actual = tokio::fs::metadata(&seg_file)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if actual != seg.length as u64 {
                return Err(format!(
                    "Segment {} size mismatch: expected {} B, got {} B",
                    seg.id, seg.length, actual
                ));
            }
        }

        let mut reader = tokio::fs::File::open(&seg_file)
            .await
            .map_err(|e| e.to_string())?;
        let copied = tokio::io::copy(&mut reader, &mut writer)
            .await
            .map_err(|e| e.to_string())?;
        total_bytes += copied;
    }

    writer.flush().await.map_err(|e| e.to_string())?;

    // Final total-size sanity check
    if let Some(expected) = expected_size {
        if expected > 0 && total_bytes != expected as u64 {
            tokio::fs::remove_file(output_path).await.ok();
            return Err(format!(
                "Assembly size mismatch: expected {} B, got {} B",
                expected, total_bytes
            ));
        }
    }

    tokio::fs::remove_dir_all(temp_dir).await.ok();
    Ok(())
}
