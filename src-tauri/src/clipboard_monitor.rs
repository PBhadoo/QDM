use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

pub type EventCallback = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync + 'static>;

pub struct ClipboardMonitor {
    enabled: Arc<Mutex<bool>>,
    emit: EventCallback,
    shutdown_tx: Arc<Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl ClipboardMonitor {
    pub fn new(emit: EventCallback) -> Self {
        Self {
            enabled: Arc::new(Mutex::new(true)),
            emit,
            shutdown_tx: Arc::new(Mutex::new(None)),
        }
    }

    pub fn start(&self) {
        let enabled = Arc::clone(&self.enabled);
        let emit = Arc::clone(&self.emit);
        let shutdown_arc = Arc::clone(&self.shutdown_tx);

        tauri::async_runtime::spawn(async move {
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
            {
                let mut lock = shutdown_arc.lock().await;
                *lock = Some(shutdown_tx);
            }

            let mut last_text = String::new();

            // Read initial clipboard value
            if let Ok(mut clipboard) = arboard::Clipboard::new() {
                last_text = clipboard.get_text().unwrap_or_default();
            }

            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    _ = tokio::time::sleep(Duration::from_millis(1500)) => {}
                }

                let is_enabled = *enabled.lock().await;
                if !is_enabled {
                    continue;
                }

                let text = match arboard::Clipboard::new() {
                    Ok(mut cb) => cb.get_text().unwrap_or_default(),
                    Err(_) => continue,
                };

                let text = text.trim().to_string();
                if text.is_empty() || text == last_text {
                    last_text = text;
                    continue;
                }

                last_text = text.clone();

                if is_url(&text) && is_downloadable_url(&text) {
                    emit("clipboard:url", serde_json::json!({ "url": text }));
                }
            }
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

    pub async fn set_enabled(&self, enabled: bool) {
        *self.enabled.lock().await = enabled;
    }

    pub async fn is_enabled(&self) -> bool {
        *self.enabled.lock().await
    }
}

fn is_url(text: &str) -> bool {
    url::Url::parse(text)
        .map(|u| u.scheme() == "http" || u.scheme() == "https" || u.scheme() == "ftp")
        .unwrap_or(false)
}

fn is_downloadable_url(url: &str) -> bool {
    let lower = url.to_lowercase();

    let file_exts = [
        ".zip", ".rar", ".7z", ".tar", ".gz",
        ".exe", ".msi", ".dmg", ".deb", ".rpm", ".apk",
        ".pdf", ".doc", ".docx", ".xls", ".xlsx",
        ".mp3", ".flac", ".wav", ".aac", ".ogg", ".m4a",
        ".mp4", ".mkv", ".avi", ".mov", ".webm", ".m4v",
        ".iso", ".img", ".torrent",
    ];

    if file_exts.iter().any(|ext| lower.contains(ext)) {
        return true;
    }

    let download_hosts = [
        "drive.google.com",
        "dropbox.com",
        "mega.nz",
        "mediafire.com",
        "github.com/releases",
        "sourceforge.net",
        "download.",
    ];

    download_hosts.iter().any(|h| lower.contains(h))
}
