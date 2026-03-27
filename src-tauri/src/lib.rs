mod browser_monitor;
mod clipboard_monitor;
mod download_engine;
mod hls_engine;
mod queue_manager;
mod tools;
mod types;
mod yt_dlp;

use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener, Manager, State};

use browser_monitor::{BrowserMonitor, EventCallback};
use clipboard_monitor::ClipboardMonitor;
use download_engine::DownloadEngine;
use queue_manager::QueueManager;
use types::*;

// ── App state ──────────────────────────────────────────────────────────────────

pub struct AppState {
    pub engine: Arc<DownloadEngine>,
    pub browser_monitor: Arc<BrowserMonitor>,
    pub clipboard_monitor: Arc<ClipboardMonitor>,
    pub queue_manager: Arc<QueueManager>,
}

// ── Config helpers ─────────────────────────────────────────────────────────────

fn config_path(app: &AppHandle) -> PathBuf {
    app.path().app_config_dir().unwrap_or_default().join("config.json")
}

fn default_config(app: &AppHandle) -> AppConfig {
    AppConfig {
        download_dir: app
            .path()
            .download_dir()
            .unwrap_or_else(|_| dirs::download_dir().unwrap_or_else(|| PathBuf::from(".")))
            .to_string_lossy()
            .to_string(),
        max_concurrent_downloads: 3,
        max_segments_per_download: 8,
        speed_limit: 0,
        show_notifications: true,
        minimize_to_tray: true,
        start_with_windows: false,
        theme: "dark".to_string(),
        ytdlp_path: String::new(),
        ytdlp_browser: "chrome".to_string(),
    }
}

fn load_config(app: &AppHandle) -> AppConfig {
    let path = config_path(app);
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(stored) = serde_json::from_str::<serde_json::Value>(&content) {
            let defaults = default_config(app);
            return AppConfig {
                download_dir: stored["downloadDir"]
                    .as_str()
                    .unwrap_or(&defaults.download_dir)
                    .to_string(),
                max_concurrent_downloads: stored["maxConcurrentDownloads"]
                    .as_u64()
                    .unwrap_or(defaults.max_concurrent_downloads as u64)
                    as u32,
                max_segments_per_download: stored["maxSegmentsPerDownload"]
                    .as_u64()
                    .unwrap_or(defaults.max_segments_per_download as u64)
                    as u32,
                speed_limit: stored["speedLimit"]
                    .as_u64()
                    .unwrap_or(defaults.speed_limit),
                show_notifications: stored["showNotifications"]
                    .as_bool()
                    .unwrap_or(defaults.show_notifications),
                minimize_to_tray: stored["minimizeToTray"]
                    .as_bool()
                    .unwrap_or(defaults.minimize_to_tray),
                start_with_windows: stored["startWithWindows"]
                    .as_bool()
                    .unwrap_or(defaults.start_with_windows),
                theme: stored["theme"]
                    .as_str()
                    .unwrap_or(&defaults.theme)
                    .to_string(),
                ytdlp_path: stored["ytdlpPath"]
                    .as_str()
                    .unwrap_or(&defaults.ytdlp_path)
                    .to_string(),
                ytdlp_browser: stored["ytdlpBrowser"]
                    .as_str()
                    .unwrap_or(&defaults.ytdlp_browser)
                    .to_string(),
            };
        }
    }
    default_config(app)
}

fn save_config(app: &AppHandle, config: &AppConfig) {
    let path = config_path(app);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        std::fs::write(&path, json).ok();
    }
}

// ── Tauri commands ─────────────────────────────────────────────────────────────

// Window controls — sync, no State reference
#[tauri::command]
fn window_minimize(window: tauri::WebviewWindow) {
    window.minimize().ok();
}

#[tauri::command]
fn window_maximize(window: tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) {
        window.unmaximize().ok();
    } else {
        window.maximize().ok();
    }
}

#[tauri::command]
fn window_close(window: tauri::WebviewWindow) {
    window.close().ok();
}

#[tauri::command]
fn window_is_maximized(window: tauri::WebviewWindow) -> bool {
    window.is_maximized().unwrap_or(false)
}

// Download commands — must return Result when async with State
#[tauri::command]
async fn download_add(
    state: State<'_, AppState>,
    request: NewDownloadRequest,
) -> Result<DownloadItem, String> {
    state.engine.add_download(request).await
}

#[tauri::command]
async fn download_start(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<DownloadItem>, String> {
    Ok(state.engine.start_download(&id).await)
}

#[tauri::command]
async fn download_pause(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<DownloadItem>, String> {
    Ok(state.engine.pause_download(&id).await)
}

#[tauri::command]
async fn download_resume(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<DownloadItem>, String> {
    Ok(state.engine.resume_download(&id).await)
}

#[tauri::command]
async fn download_cancel(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    Ok(state.engine.cancel_download(&id).await)
}

#[tauri::command]
async fn download_remove(
    state: State<'_, AppState>,
    id: String,
    delete_file: bool,
) -> Result<bool, String> {
    Ok(state.engine.remove_download(&id, delete_file).await)
}

#[tauri::command]
async fn download_retry(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<DownloadItem>, String> {
    Ok(state.engine.retry_download(&id).await)
}

#[tauri::command]
async fn download_get_all(state: State<'_, AppState>) -> Result<Vec<DownloadItem>, String> {
    Ok(state.engine.get_all_downloads().await)
}

#[tauri::command]
async fn download_open_file(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    Ok(state.engine.open_file(&id).await)
}

#[tauri::command]
async fn download_open_folder(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    Ok(state.engine.open_folder(&id).await)
}

#[tauri::command]
async fn download_pause_all(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.pause_all().await;
    Ok(())
}

#[tauri::command]
async fn download_resume_all(state: State<'_, AppState>) -> Result<(), String> {
    state.engine.resume_all().await;
    Ok(())
}

#[tauri::command]
async fn download_probe(
    state: State<'_, AppState>,
    url: String,
    headers: Option<std::collections::HashMap<String, String>>,
) -> Result<ProbeResult, String> {
    Ok(match state.engine.probe_url(&url, headers.as_ref()).await {
        Ok(result) => result,
        Err(e) => ProbeResult {
            file_size: -1,
            resumable: false,
            file_name: String::new(),
            final_url: url,
            error: Some(e),
        },
    })
}

// Browser monitor commands
#[tauri::command]
async fn browser_get_media_list(
    state: State<'_, AppState>,
) -> Result<Vec<MediaItem>, String> {
    Ok(state.browser_monitor.get_media_list().await)
}

#[tauri::command]
async fn browser_clear_media(state: State<'_, AppState>) -> Result<bool, String> {
    state.browser_monitor.clear_media_list().await;
    Ok(true)
}

#[tauri::command]
async fn browser_download_media(
    state: State<'_, AppState>,
    media_id: String,
) -> Result<Option<DownloadItem>, String> {
    let media = state
        .browser_monitor
        .get_media_list()
        .await
        .into_iter()
        .find(|m| m.id == media_id);

    if let Some(media) = media {
        let mut headers = media.headers.clone().unwrap_or_default();
        if let Some(cookies) = &media.cookies {
            headers.insert("Cookie".to_string(), cookies.clone());
        }
        if !media.tab_url.is_empty() {
            headers.insert("Referer".to_string(), media.tab_url.clone());
        }
        headers.insert(
            "User-Agent".to_string(),
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36".to_string(),
        );
        let source_page = if media.tab_url.is_empty() { None } else { Some(media.tab_url.clone()) };
        let req = NewDownloadRequest {
            url: media.url,
            file_name: Some(media.name),
            save_path: None,
            headers: if headers.is_empty() { None } else { Some(headers) },
            max_segments: None,
            auto_start: Some(true),
            source_page_url: source_page,
            ytdlp_quality: None,
            ytdlp_cookies: None,
        };
        Ok(state.engine.add_download(req).await.ok())
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn browser_get_status(
    state: State<'_, AppState>,
) -> Result<BrowserMonitorStatus, String> {
    Ok(BrowserMonitorStatus {
        running: true,
        port: state.browser_monitor.get_port(),
        config: state.browser_monitor.get_config().await,
        media_count: state.browser_monitor.get_media_list().await.len(),
    })
}

#[tauri::command]
async fn browser_set_config(
    state: State<'_, AppState>,
    config: BrowserMonitorConfig,
) -> Result<BrowserMonitorConfig, String> {
    state.browser_monitor.set_config(config).await;
    Ok(state.browser_monitor.get_config().await)
}

// Clipboard commands
#[tauri::command]
async fn clipboard_get_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.clipboard_monitor.is_enabled().await)
}

#[tauri::command]
async fn clipboard_set_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    state.clipboard_monitor.set_enabled(enabled).await;
    Ok(enabled)
}

// Queue commands
#[tauri::command]
async fn queue_get_all(state: State<'_, AppState>) -> Result<Vec<DownloadQueue>, String> {
    Ok(state.queue_manager.get_queues().await)
}

#[tauri::command]
async fn queue_create(
    state: State<'_, AppState>,
    name: String,
    max_concurrent: u32,
) -> Result<DownloadQueue, String> {
    Ok(state.queue_manager.create_queue(name, max_concurrent).await)
}

#[tauri::command]
async fn queue_update(
    state: State<'_, AppState>,
    id: String,
    updates: serde_json::Value,
) -> Result<Option<DownloadQueue>, String> {
    Ok(state.queue_manager.update_queue(&id, updates).await)
}

#[tauri::command]
async fn queue_delete(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    Ok(state.queue_manager.delete_queue(&id).await)
}

#[tauri::command]
async fn queue_add_downloads(
    state: State<'_, AppState>,
    queue_id: String,
    download_ids: Vec<String>,
) -> Result<bool, String> {
    Ok(state.queue_manager.add_to_queue(&queue_id, download_ids).await)
}

#[tauri::command]
async fn queue_set_schedule(
    state: State<'_, AppState>,
    queue_id: String,
    schedule: Option<QueueSchedule>,
) -> Result<bool, String> {
    Ok(state.queue_manager.set_schedule(&queue_id, schedule).await)
}

// Config commands
#[tauri::command]
async fn config_get(state: State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.engine.config.lock().await.clone())
}

#[tauri::command]
async fn config_set(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> Result<AppConfig, String> {
    let current = state.engine.config.lock().await.clone();
    let new_config = AppConfig {
        download_dir: config["downloadDir"]
            .as_str()
            .unwrap_or(&current.download_dir)
            .to_string(),
        max_concurrent_downloads: config["maxConcurrentDownloads"]
            .as_u64()
            .unwrap_or(current.max_concurrent_downloads as u64) as u32,
        max_segments_per_download: config["maxSegmentsPerDownload"]
            .as_u64()
            .unwrap_or(current.max_segments_per_download as u64) as u32,
        speed_limit: config["speedLimit"]
            .as_u64()
            .unwrap_or(current.speed_limit),
        show_notifications: config["showNotifications"]
            .as_bool()
            .unwrap_or(current.show_notifications),
        minimize_to_tray: config["minimizeToTray"]
            .as_bool()
            .unwrap_or(current.minimize_to_tray),
        start_with_windows: config["startWithWindows"]
            .as_bool()
            .unwrap_or(current.start_with_windows),
        theme: config["theme"].as_str().unwrap_or(&current.theme).to_string(),
        ytdlp_path: config["ytdlpPath"].as_str().unwrap_or(&current.ytdlp_path).to_string(),
        ytdlp_browser: config["ytdlpBrowser"].as_str().unwrap_or(&current.ytdlp_browser).to_string(),
    };
    drop(current);
    state.engine.update_config(new_config.clone()).await;
    save_config(&app_handle, &new_config);
    Ok(new_config)
}

// Dialog command
#[tauri::command]
async fn dialog_select_folder(app_handle: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::{DialogExt, FilePath};
    let (tx, rx) = tokio::sync::oneshot::channel::<Option<FilePath>>();
    app_handle.dialog().file().pick_folder(move |path| {
        tx.send(path).ok();
    });
    Ok(rx
        .await
        .ok()
        .flatten()
        .map(|p| format!("{}", p)))
}

// Shell command
#[tauri::command]
async fn shell_open_external(app_handle: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_shell::ShellExt;
    app_handle.shell().open(&url, None).map_err(|e| e.to_string())
}

// Auth challenge command
#[tauri::command]
async fn download_provide_auth(
    state: State<'_, AppState>,
    id: String,
    username: String,
    password: String,
) -> Result<bool, String> {
    Ok(state.engine.provide_auth(&id, &username, &password).await)
}

/// Open the source page of a download in the system browser so the user can
/// re-capture a fresh (non-expired) link and resume the download.
#[tauri::command]
async fn download_reopen_source(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    id: String,
) -> Result<bool, String> {
    use tauri_plugin_shell::ShellExt as _;
    let source_url = {
        let engine_state = state.engine.state.lock().await;
        engine_state
            .downloads
            .get(&id)
            .and_then(|i| i.source_page_url.clone())
    };
    match source_url {
        Some(url) => {
            app_handle.shell().open(&url, None).map_err(|e| e.to_string())?;
            Ok(true)
        }
        None => Ok(false),
    }
}

// Update commands
#[tauri::command]
async fn update_check() -> Result<serde_json::Value, String> {
    let client = reqwest::Client::builder()
        .user_agent("QDM/1.0.3")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get("https://api.github.com/repos/PBhadoo/QDM/releases/latest")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let latest = json["tag_name"].as_str().unwrap_or("v0.0.0");
    let latest_ver = latest.trim_start_matches('v');
    let current = env!("CARGO_PKG_VERSION");
    let update_available = latest_ver > current;

    Ok(serde_json::json!({
        "updateAvailable": update_available,
        "latestVersion": latest_ver,
        "currentVersion": current,
        "releaseUrl": json["html_url"].as_str().unwrap_or(""),
        "releaseNotes": json["body"].as_str().unwrap_or(""),
    }))
}

#[tauri::command]
fn update_get_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
async fn update_open_release(
    app_handle: AppHandle,
    version: Option<String>,
) -> Result<(), String> {
    use tauri_plugin_shell::ShellExt;
    let url = if let Some(v) = version {
        format!("https://github.com/PBhadoo/QDM/releases/tag/v{}", v)
    } else {
        "https://github.com/PBhadoo/QDM/releases/latest".to_string()
    };
    app_handle.shell().open(&url, None).map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_download_install(
    app_handle: AppHandle,
    version: String,
) -> Result<(), String> {
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .user_agent("QDM/1.0.3")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let tag = format!("v{}", version.trim_start_matches('v'));
    let resp = client
        .get(format!("https://api.github.com/repos/PBhadoo/QDM/releases/tags/{}", tag))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;

    let assets = json["assets"].as_array()
        .ok_or_else(|| "No assets found in release".to_string())?;

    let asset = assets.iter().find(|a| {
        let name = a["name"].as_str().unwrap_or("");
        if cfg!(windows) {
            name.ends_with("-setup.exe")
        } else if cfg!(target_os = "macos") {
            name.ends_with(".dmg")
        } else {
            name.ends_with(".AppImage")
        }
    }).ok_or_else(|| "No installer asset found for this platform in the release".to_string())?;

    let download_url = asset["browser_download_url"].as_str()
        .ok_or_else(|| "Missing download URL".to_string())?
        .to_string();
    let file_name = asset["name"].as_str().unwrap_or("qdm_update");
    let temp_dest = std::env::temp_dir().join(file_name);

    let dl_client = reqwest::Client::builder()
        .user_agent("QDM/1.0.3")
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = dl_client.get(&download_url).send().await
        .map_err(|e| format!("Download failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} downloading update", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut downloaded = 0u64;
    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&temp_dest).await
        .map_err(|e| format!("Cannot create temp file: {}", e))?;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if total > 0 {
            let pct = (downloaded * 100 / total) as u32;
            let mb = downloaded as f64 / 1_048_576.0;
            let total_mb = total as f64 / 1_048_576.0;
            app_handle.emit("update:progress", serde_json::json!({
                "pct": pct,
                "msg": format!("{:.1} / {:.1} MB", mb, total_mb),
                "done": false,
            })).ok();
        }
    }
    file.flush().await.ok();
    drop(file);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new(&temp_dest)
            .creation_flags(0x00000008) // DETACHED_PROCESS
            .spawn()
            .map_err(|e| format!("Failed to launch installer: {}", e))?;
        app_handle.emit("update:installing", ()).ok();
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        app_handle.exit(0);
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&temp_dest)
            .spawn()
            .map_err(|e| format!("Failed to open DMG: {}", e))?;
        app_handle.emit("update:progress", serde_json::json!({
            "pct": 100,
            "msg": "DMG opened — drag Quantum Download Manager to Applications to finish",
            "done": true,
        })).ok();
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp_dest, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod failed: {}", e))?;
        let folder = temp_dest.parent().unwrap_or(std::path::Path::new("/tmp"));
        let _ = std::process::Command::new("xdg-open").arg(folder).spawn();
        app_handle.emit("update:progress", serde_json::json!({
            "pct": 100,
            "msg": format!("AppImage saved — run it from {}", temp_dest.display()),
            "done": true,
        })).ok();
    }

    Ok(())
}

// ── yt-dlp management commands ─────────────────────────────────────────────────

#[tauri::command]
async fn ytdlp_list_formats(url: String, state: State<'_, AppState>) -> Result<yt_dlp::FormatResult, String> {
    let app_data_dir = state.engine.app_handle.path().app_data_dir().unwrap_or_default();
    let tools_dir = app_data_dir.join("tools");
    let managed = if cfg!(windows) { tools_dir.join("yt-dlp.exe") } else { tools_dir.join("yt-dlp") };

    let bin = if managed.is_file() {
        Some(managed)
    } else {
        let config = state.engine.config.lock().await;
        let user_path = if config.ytdlp_path.is_empty() { None } else { Some(config.ytdlp_path.clone()) };
        let res_dir = state.engine.app_handle.path().resource_dir().ok();
        drop(config);
        yt_dlp::find_yt_dlp(user_path.as_deref(), res_dir.as_deref())
    };

    match bin {
        None => Err("yt-dlp not found. Open Settings → Tools and click Install.".to_string()),
        Some(b) => yt_dlp::list_formats(&b, &url).await,
    }
}

#[tauri::command]
async fn ytdlp_get_version(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (user_path, resource_dir) = {
        let config = state.engine.config.lock().await;
        let p = if config.ytdlp_path.is_empty() { None } else { Some(config.ytdlp_path.clone()) };
        let r = state.engine.app_handle.path().resource_dir().ok();
        (p, r)
    };
    let bin = yt_dlp::find_yt_dlp(user_path.as_deref(), resource_dir.as_deref());

    let installed = match &bin {
        Some(p) => yt_dlp::get_installed_version(p).await,
        None => None,
    };

    Ok(serde_json::json!({
        "installed": installed,
        "path": bin.as_ref().map(|p| p.display().to_string()),
        "found": bin.is_some(),
    }))
}

#[tauri::command]
async fn ytdlp_check_update(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let (user_path, resource_dir) = {
        let config = state.engine.config.lock().await;
        let p = if config.ytdlp_path.is_empty() { None } else { Some(config.ytdlp_path.clone()) };
        let r = state.engine.app_handle.path().resource_dir().ok();
        (p, r)
    };
    let bin = yt_dlp::find_yt_dlp(user_path.as_deref(), resource_dir.as_deref());

    let installed = match &bin {
        Some(p) => yt_dlp::get_installed_version(p).await,
        None => None,
    };

    let (latest_tag, download_url) = yt_dlp::get_latest_release().await?;

    Ok(serde_json::json!({
        "installed": installed,
        "latest": latest_tag,
        "updateAvailable": installed.as_deref().map(|v| v != latest_tag.as_str()).unwrap_or(true),
        "downloadUrl": download_url,
        "path": bin.as_ref().map(|p| p.display().to_string()),
        "found": bin.is_some(),
    }))
}

#[tauri::command]
async fn ytdlp_download_update(
    state: State<'_, AppState>,
    download_url: String,
    dest_path: Option<String>,
) -> Result<serde_json::Value, String> {
    // Determine destination: user-specified path, or the existing binary location,
    // or fallback to app data dir / yt-dlp.exe
    let dest = if let Some(p) = dest_path {
        std::path::PathBuf::from(p)
    } else {
        let (user_path, resource_dir) = {
            let config = state.engine.config.lock().await;
            let p = if config.ytdlp_path.is_empty() { None } else { Some(config.ytdlp_path.clone()) };
            let r = state.engine.app_handle.path().resource_dir().ok();
            (p, r)
        };
        yt_dlp::find_yt_dlp(user_path.as_deref(), resource_dir.as_deref())
            .unwrap_or_else(|| {
                let name = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
                state.engine.app_handle.path().app_data_dir()
                    .unwrap_or_default()
                    .join(name)
            })
    };

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }

    yt_dlp::download_yt_dlp(&download_url, &dest).await?;

    // Verify new version
    let version = yt_dlp::get_installed_version(&dest).await;
    Ok(serde_json::json!({
        "path": dest.display().to_string(),
        "version": version,
    }))
}

// ── Integrated tools commands ──────────────────────────────────────────────────

/// Get status of managed yt-dlp and ffmpeg binaries.
#[tauri::command]
async fn tools_get_status(app: AppHandle) -> Result<tools::ToolsStatus, String> {
    Ok(tools::get_status(&app).await)
}

/// Download / update yt-dlp into the managed tools directory.
/// Streams `tools:progress` events during download.
#[tauri::command]
async fn tools_install_ytdlp(app: AppHandle) -> Result<String, String> {
    tools::install_ytdlp(&app).await
}

/// Download / install ffmpeg into the managed tools directory.
/// Streams `tools:progress` events during download.
#[tauri::command]
async fn tools_install_ffmpeg(app: AppHandle) -> Result<(), String> {
    tools::install_ffmpeg(&app).await
}

// ── System tray ────────────────────────────────────────────────────────────────

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
    use tauri::tray::{TrayIconBuilder, TrayIconEvent};

    let show = MenuItemBuilder::with_id("show", "Show QDM").build(app)?;
    let new_dl = MenuItemBuilder::with_id("new_download", "New Download").build(app)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let pause_all = MenuItemBuilder::with_id("pause_all", "Pause All").build(app)?;
    let resume_all = MenuItemBuilder::with_id("resume_all", "Resume All").build(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit QDM").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&new_dl)
        .item(&sep1)
        .item(&pause_all)
        .item(&resume_all)
        .item(&sep2)
        .item(&quit)
        .build()?;

    let app_handle2 = app.handle().clone();

    let mut tray_builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Quantum Download Manager")
        .on_menu_event(move |app, event| {
            let state = app.state::<AppState>();
            match event.id().as_ref() {
                "show" => {
                    if let Some(w) = app.get_webview_window("main") {
                        w.show().ok();
                        w.set_focus().ok();
                    }
                }
                "new_download" => {
                    if let Some(w) = app.get_webview_window("main") {
                        w.show().ok();
                        w.set_focus().ok();
                        w.emit("show-new-download", ()).ok();
                    }
                }
                "pause_all" => {
                    let engine = Arc::clone(&state.engine);
                    tauri::async_runtime::spawn(async move {
                        engine.pause_all().await;
                    });
                }
                "resume_all" => {
                    let engine = Arc::clone(&state.engine);
                    tauri::async_runtime::spawn(async move {
                        engine.resume_all().await;
                    });
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(move |_tray, event| {
            if let TrayIconEvent::Click { .. } = event {
                if let Some(w) = app_handle2.get_webview_window("main") {
                    w.show().ok();
                    w.set_focus().ok();
                }
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray_builder = tray_builder.icon(icon);
    }

    tray_builder.build(app)?;
    Ok(())
}

// ── App entry point ────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            let config = load_config(&app_handle);
            let db_path = PathBuf::from(&config.download_dir).join(".qdm_data");
            std::fs::create_dir_all(&db_path).ok();

            // Build shared emit callback (Rust → Frontend)
            let app_for_emit = app_handle.clone();
            let emit_fn: EventCallback =
                Arc::new(move |event: &str, payload: serde_json::Value| {
                    app_for_emit.emit(event, payload).ok();
                });

            let engine = DownloadEngine::new(config.clone(), app_handle.clone());

            // Browser monitor → auto-start downloads
            let engine_for_browser = Arc::clone(&engine);
            let app_for_browser = app_handle.clone();
            let browser_emit: EventCallback = Arc::new(move |event: &str, payload: serde_json::Value| {
                if event == "browser:download" || event == "browser:vid-download" {
                    let url = payload["url"].as_str().map(String::from);
                    let file_name = payload["fileName"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .map(String::from);

                    // Merge headers + cookie + referer into one map
                    let mut headers: std::collections::HashMap<String, String> =
                        serde_json::from_value(payload["headers"].clone()).unwrap_or_default();
                    if let Some(cookie) = payload["cookie"].as_str() {
                        if !cookie.is_empty() {
                            headers.insert("Cookie".to_string(), cookie.to_string());
                        }
                    }
                    if let Some(tab_url) = payload["tabUrl"].as_str() {
                        if !tab_url.is_empty() && !headers.contains_key("Referer") {
                            headers.insert("Referer".to_string(), tab_url.to_string());
                        }
                    }
                    let headers_opt = if headers.is_empty() { None } else { Some(headers) };

                    if let Some(raw_url) = url {
                        let engine = Arc::clone(&engine_for_browser);
                        let tab_url = payload["tabUrl"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(String::from);
                        let ytdlp_cookies = payload["ytdlpCookies"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .map(String::from);

                        // If the intercepted URL is a YouTube stream (googlevideo.com)
                        // but the tab is a YouTube watch page, use the watch URL instead
                        // so yt-dlp handles it properly with full quality + cookies.
                        let (actual_url, actual_file_name) = {
                            let is_yt_stream = raw_url.contains("googlevideo.com");
                            let tab_is_yt = tab_url.as_deref().map(|t|
                                crate::yt_dlp::is_yt_dlp_url(t)
                            ).unwrap_or(false);
                            if is_yt_stream && tab_is_yt {
                                // Use the YouTube watch URL — yt-dlp will pick the right format
                                (tab_url.clone().unwrap_or(raw_url), None)
                            } else {
                                (raw_url, file_name)
                            }
                        };

                        let source_page = tab_url;

                        // YouTube URLs → ask the user for quality before starting
                        if crate::yt_dlp::is_yt_dlp_url(&actual_url) {
                            app_for_browser.emit("download:quality_required", serde_json::json!({
                                "url": actual_url,
                                "fileName": actual_file_name,
                                "sourcePageUrl": source_page,
                                "ytdlpCookies": ytdlp_cookies,
                                "headers": headers_opt,
                            })).ok();
                        } else {
                            tauri::async_runtime::spawn(async move {
                                engine
                                    .add_download(NewDownloadRequest {
                                        url: actual_url,
                                        file_name: actual_file_name,
                                        save_path: None,
                                        headers: headers_opt,
                                        max_segments: None,
                                        auto_start: Some(true),
                                        source_page_url: source_page,
                                        ytdlp_quality: None,
                                        ytdlp_cookies,
                                    })
                                    .await
                                    .ok();
                            });
                        }
                    }
                }
                // Show/focus window when a download is intercepted or explicitly requested
                if event == "browser:show" || event == "browser:download" || event == "browser:vid-download" {
                    if let Some(w) = app_for_browser.get_webview_window("main") {
                        w.show().ok();
                        w.set_focus().ok();
                    }
                }

                // Always forward to frontend for UI updates
                app_for_browser.emit(event, payload).ok();
            });

            let browser_monitor = Arc::new(BrowserMonitor::new(Arc::clone(&browser_emit)));
            let clipboard_monitor = Arc::new(ClipboardMonitor::new(Arc::clone(&emit_fn)));
            let queue_manager = Arc::new(QueueManager::new(&db_path, Arc::clone(&emit_fn)));

            browser_monitor.start();
            clipboard_monitor.start();
            queue_manager.start_scheduler();

            // Clone for WS broadcast listeners (Arc::clone is cheap; app.manage takes ownership below)
            let bm_ws_added    = Arc::clone(&browser_monitor);
            let bm_ws_complete = Arc::clone(&browser_monitor);
            let bm_ws_failed   = Arc::clone(&browser_monitor);

            // Push download lifecycle events to extension via WebSocket
            app_handle.listen("download:added", move |event| {
                let p: serde_json::Value = serde_json::from_str(event.payload()).unwrap_or_default();
                bm_ws_added.broadcast(serde_json::json!({
                    "type": "download_added",
                    "id": p["id"],
                    "fileName": p["fileName"],
                }).to_string());
            });

            app_handle.listen("download:completed", move |event| {
                let p: serde_json::Value = serde_json::from_str(event.payload()).unwrap_or_default();
                bm_ws_complete.broadcast(serde_json::json!({
                    "type": "download_complete",
                    "id": p["id"],
                    "fileName": p["fileName"],
                }).to_string());
            });

            app_handle.listen("download:failed", move |event| {
                let p: serde_json::Value = serde_json::from_str(event.payload()).unwrap_or_default();
                bm_ws_failed.broadcast(serde_json::json!({
                    "type": "download_failed",
                    "id": p["id"],
                    "error": p["error"],
                }).to_string());
            });

            // OS notifications for completed downloads
            let app_for_notif = app_handle.clone();
            app_handle.listen("download:notify", move |event| {
                let payload: serde_json::Value =
                    serde_json::from_str(event.payload()).unwrap_or_default();
                let title = payload["title"].as_str().unwrap_or("QDM").to_string();
                let body = payload["body"].as_str().unwrap_or("").to_string();
                use tauri_plugin_notification::NotificationExt;
                app_for_notif
                    .notification()
                    .builder()
                    .title(&title)
                    .body(&body)
                    .show()
                    .ok();
            });

            app.manage(AppState {
                engine,
                browser_monitor,
                clipboard_monitor,
                queue_manager,
            });

            // Auto-install yt-dlp and ffmpeg if missing
            {
                let app_for_tools = app_handle.clone();
                tauri::async_runtime::spawn(async move {
                    // Wait for the window to be ready before showing progress
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let status = tools::get_status(&app_for_tools).await;
                    if !status.ytdlp.installed {
                        log::info!("[startup] yt-dlp not found — auto-installing");
                        tools::install_ytdlp(&app_for_tools).await.ok();
                    }
                    if !status.ffmpeg.installed {
                        log::info!("[startup] ffmpeg not found — auto-installing");
                        tools::install_ffmpeg(&app_for_tools).await.ok();
                    }
                    if !status.ytdlp.installed || !status.ffmpeg.installed {
                        app_for_tools.emit("tools:setup_done", ()).ok();
                    }
                });
            }

            #[cfg(not(target_os = "linux"))]
            setup_tray(app)?;

            // Close → minimize to tray
            if let Some(window) = app.get_webview_window("main") {
                let app_for_close = app_handle.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let state = app_for_close.state::<AppState>();
                        let minimize = tauri::async_runtime::block_on(async {
                            state.engine.config.lock().await.minimize_to_tray
                        });
                        if minimize {
                            api.prevent_close();
                            if let Some(w) = app_for_close.get_webview_window("main") {
                                w.hide().ok();
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Resized(_) = event {
                let maximized = window.is_maximized().unwrap_or(false);
                window.emit("window:maximized", maximized).ok();
            }
        })
        .invoke_handler(tauri::generate_handler![
            window_minimize,
            window_maximize,
            window_close,
            window_is_maximized,
            download_add,
            download_start,
            download_pause,
            download_resume,
            download_cancel,
            download_remove,
            download_retry,
            download_get_all,
            download_open_file,
            download_open_folder,
            download_pause_all,
            download_resume_all,
            download_probe,
            browser_get_media_list,
            browser_clear_media,
            browser_download_media,
            browser_get_status,
            browser_set_config,
            clipboard_get_enabled,
            clipboard_set_enabled,
            queue_get_all,
            queue_create,
            queue_update,
            queue_delete,
            queue_add_downloads,
            queue_set_schedule,
            config_get,
            config_set,
            dialog_select_folder,
            shell_open_external,
            update_check,
            update_get_version,
            update_open_release,
            update_download_install,
            download_provide_auth,
            download_reopen_source,
            ytdlp_list_formats,
            ytdlp_get_version,
            ytdlp_check_update,
            ytdlp_download_update,
            tools_get_status,
            tools_install_ytdlp,
            tools_install_ffmpeg,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
