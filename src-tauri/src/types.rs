use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Paused,
    Completed,
    Failed,
    Assembling,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DownloadCategory {
    All,
    Compressed,
    Documents,
    Music,
    Videos,
    Programs,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSegment {
    pub id: String,
    pub offset: u64,
    pub length: i64, // -1 if unknown
    pub downloaded: u64,
    pub state: u8, // 0=NotStarted, 1=Downloading, 2=Finished, 3=Failed
    pub speed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadItem {
    pub id: String,
    pub url: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "fileSize")]
    pub file_size: i64,
    pub downloaded: u64,
    pub progress: f64,
    pub speed: f64,
    pub eta: i64,
    pub status: DownloadStatus,
    pub category: DownloadCategory,
    #[serde(rename = "dateAdded")]
    pub date_added: String,
    #[serde(rename = "dateCompleted", skip_serializing_if = "Option::is_none")]
    pub date_completed: Option<String>,
    #[serde(rename = "savePath")]
    pub save_path: String,
    pub resumable: bool,
    pub segments: Vec<DownloadSegment>,
    #[serde(rename = "maxSegments")]
    pub max_segments: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    /// Cookie string kept in-memory only — never serialized to disk so
    /// expired session cookies don't persist across app restarts.
    #[serde(skip, default)]
    pub runtime_cookie: Option<String>,
    /// `Authorization` header value (e.g. "Basic dXNlcjpwYXNz") provided by
    /// the user after a 401 challenge.  Kept in-memory only; never persisted.
    #[serde(skip, default)]
    pub runtime_auth: Option<String>,
    /// Page URL from which the download was captured. Used to re-open the page
    /// when a CDN-signed URL expires mid-download.
    #[serde(rename = "sourcePageUrl", skip_serializing_if = "Option::is_none")]
    pub source_page_url: Option<String>,
    /// yt-dlp quality selection: "best" | "1080p" | "720p" | "480p" | "360p" | "audio"
    #[serde(rename = "ytdlpQuality", default, skip_serializing_if = "Option::is_none")]
    pub ytdlp_quality: Option<String>,
    /// Netscape cookies.txt content from the browser extension. Written to a temp
    /// file and passed to yt-dlp via `--cookies`. Never serialized to disk.
    #[serde(skip, default)]
    pub ytdlp_cookies: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDownloadRequest {
    pub url: String,
    #[serde(rename = "fileName", default)]
    pub file_name: Option<String>,
    #[serde(rename = "savePath", default)]
    pub save_path: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
    #[serde(rename = "maxSegments", default)]
    pub max_segments: Option<u32>,
    #[serde(rename = "autoStart", default)]
    pub auto_start: Option<bool>,
    /// Page the download was captured from — used to recover from expired CDN links.
    #[serde(rename = "sourcePageUrl", default)]
    pub source_page_url: Option<String>,
    /// yt-dlp quality: "best" | "1080p" | "720p" | "480p" | "360p" | "audio"
    #[serde(rename = "ytdlpQuality", default)]
    pub ytdlp_quality: Option<String>,
    /// Netscape cookies.txt content from browser extension.
    #[serde(rename = "ytdlpCookies", default)]
    pub ytdlp_cookies: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub id: String,
    pub downloaded: u64,
    pub progress: f64,
    pub speed: f64,
    pub eta: i64,
    pub segments: Vec<DownloadSegment>,
    pub status: DownloadStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(rename = "downloadDir")]
    pub download_dir: String,
    #[serde(rename = "maxConcurrentDownloads")]
    pub max_concurrent_downloads: u32,
    #[serde(rename = "maxSegmentsPerDownload")]
    pub max_segments_per_download: u32,
    #[serde(rename = "speedLimit")]
    pub speed_limit: u64,
    #[serde(rename = "showNotifications")]
    pub show_notifications: bool,
    #[serde(rename = "minimizeToTray")]
    pub minimize_to_tray: bool,
    #[serde(rename = "startWithWindows")]
    pub start_with_windows: bool,
    pub theme: String,
    /// Path to yt-dlp binary. Empty = auto-detect.
    #[serde(rename = "ytdlpPath", default)]
    pub ytdlp_path: String,
    /// Browser to extract cookies from for yt-dlp ("chrome", "firefox", "edge", …).
    /// Empty = no cookie extraction.
    #[serde(rename = "ytdlpBrowser", default = "default_ytdlp_browser")]
    pub ytdlp_browser: String,
}

fn default_ytdlp_browser() -> String { "chrome".to_string() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    #[serde(rename = "fileSize")]
    pub file_size: i64,
    pub resumable: bool,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "finalUrl")]
    pub final_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// Queue types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadQueue {
    pub id: String,
    pub name: String,
    #[serde(rename = "downloadIds")]
    pub download_ids: Vec<String>,
    #[serde(rename = "maxConcurrent")]
    pub max_concurrent: u32,
    pub enabled: bool,
    pub schedule: Option<QueueSchedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSchedule {
    pub enabled: bool,
    #[serde(rename = "startTime")]
    pub start_time: String, // HH:mm
    #[serde(rename = "endTime")]
    pub end_time: String, // HH:mm
    pub days: Vec<u8>, // 0=Sun..6=Sat
}

// Browser monitor types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "tabUrl")]
    pub tab_url: String,
    #[serde(rename = "tabId")]
    pub tab_id: String,
    #[serde(rename = "dateAdded")]
    pub date_added: String,
    pub url: String,
    #[serde(rename = "audioUrl", skip_serializing_if = "Option::is_none")]
    pub audio_url: Option<String>,
    #[serde(rename = "type")]
    pub media_type: String,
    pub size: i64,
    #[serde(rename = "contentType")]
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cookies: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserMonitorConfig {
    pub enabled: bool,
    #[serde(rename = "fileExtensions")]
    pub file_extensions: Vec<String>,
    #[serde(rename = "videoExtensions")]
    pub video_extensions: Vec<String>,
    #[serde(rename = "blockedHosts")]
    pub blocked_hosts: Vec<String>,
    #[serde(rename = "mediaTypes")]
    pub media_types: Vec<String>,
    #[serde(rename = "tabsWatcher")]
    pub tabs_watcher: Vec<String>,
    #[serde(rename = "matchingHosts")]
    pub matching_hosts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserMonitorStatus {
    pub running: bool,
    pub port: u16,
    pub config: BrowserMonitorConfig,
    #[serde(rename = "mediaCount")]
    pub media_count: usize,
}

/// Shared runtime state for the download engine.
/// Lives in `types` so `hls_engine` can reference it without circular dependencies.
pub struct EngineState {
    pub downloads: HashMap<String, DownloadItem>,
    pub active_tokens: HashMap<String, CancellationToken>,
    pub pending_queue: Vec<String>,
    pub active_count: u32,
}
