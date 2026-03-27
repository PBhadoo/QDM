/// Integrated tool management for QDM.
///
/// Manages yt-dlp and ffmpeg binaries stored inside the app's own data
/// directory, so users never need to install anything manually.
///
/// Tools are stored in  `{appDataDir}/tools/`
///   Windows : yt-dlp.exe, ffmpeg.exe
///   macOS   : yt-dlp, ffmpeg
///   Linux   : yt-dlp, ffmpeg
///
/// On first use (or when missing) the user presses "Install" in Settings and
/// the app downloads the latest release directly from GitHub / BtbN.
use std::io::Read;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

// ── Directory / path helpers ──────────────────────────────────────────────────

/// Returns the path to the managed tools directory, creating it if needed.
pub fn tools_dir(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("tools");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn ytdlp_bin(tools: &Path) -> PathBuf {
    if cfg!(windows) {
        tools.join("yt-dlp.exe")
    } else {
        tools.join("yt-dlp")
    }
}

pub fn ffmpeg_bin(tools: &Path) -> PathBuf {
    if cfg!(windows) {
        tools.join("ffmpeg.exe")
    } else {
        tools.join("ffmpeg")
    }
}

// ── Version helpers ───────────────────────────────────────────────────────────

pub async fn ytdlp_version(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let mut cmd = tokio::process::Command::new(path);
    cmd.arg("--version");
    #[cfg(windows)] { cmd.creation_flags(0x08000000); }
    let out = cmd.output().await.ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub async fn ffmpeg_version(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let mut cmd = tokio::process::Command::new(path);
    cmd.args(["-version"]);
    #[cfg(windows)] { cmd.creation_flags(0x08000000); }
    let out = cmd.output().await.ok()?;
    // First line: "ffmpeg version N-xxx Copyright …"
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("ffmpeg version "))
        .map(|v| v.split_whitespace().next().unwrap_or("unknown").to_string())
}

// ── Status ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub installed: bool,
    pub version: Option<String>,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolsStatus {
    pub ytdlp: ToolInfo,
    pub ffmpeg: ToolInfo,
    #[serde(rename = "toolsDir")]
    pub tools_dir: String,
}

pub async fn get_status(app: &AppHandle) -> ToolsStatus {
    let dir = tools_dir(app);
    let yp = ytdlp_bin(&dir);
    let fp = ffmpeg_bin(&dir);

    let yv = ytdlp_version(&yp).await;
    let fv = ffmpeg_version(&fp).await;

    ToolsStatus {
        ytdlp: ToolInfo {
            installed: yp.is_file(),
            version: yv,
            path: yp.to_string_lossy().to_string(),
        },
        ffmpeg: ToolInfo {
            installed: fp.is_file(),
            version: fv,
            path: fp.to_string_lossy().to_string(),
        },
        tools_dir: dir.to_string_lossy().to_string(),
    }
}

// ── Progress events ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct ToolProgress {
    tool: String,
    step: String,
    pct: u32,
    msg: String,
}

fn emit_progress(app: &AppHandle, tool: &str, step: &str, pct: u32, msg: &str) {
    log::info!("[tools] {} {} {}% — {}", tool, step, pct, msg);
    app.emit(
        "tools:progress",
        ToolProgress {
            tool: tool.to_string(),
            step: step.to_string(),
            pct,
            msg: msg.to_string(),
        },
    )
    .ok();
}

// ── Streaming download ────────────────────────────────────────────────────────

async fn download_to_file(
    app: &AppHandle,
    tool: &str,
    url: &str,
    dest: &Path,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("QDM/1.0.3")
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} downloading {}", resp.status(), tool));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut downloaded = 0u64;
    let mut stream = resp.bytes_stream();

    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| format!("Cannot create {}: {}", dest.display(), e))?;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;

        if total > 0 {
            let pct = (downloaded * 85 / total) as u32; // reserve 85% for download
            let mb = downloaded as f64 / 1_048_576.0;
            let total_mb = total as f64 / 1_048_576.0;
            emit_progress(
                app,
                tool,
                "downloading",
                pct,
                &format!("Downloading… {:.1} / {:.1} MB", mb, total_mb),
            );
        }
    }

    file.flush().await.ok();
    Ok(())
}

// ── Zip extraction ────────────────────────────────────────────────────────────

/// Extract the first entry whose name ends with `suffix` from a zip archive.
fn extract_from_zip(zip_path: &Path, suffix: &str, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| format!("Open zip: {}", e))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| format!("Parse zip: {}", e))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().replace('\\', "/");
        if name.ends_with(suffix) || name == suffix {
            let mut data = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
            std::fs::write(dest, &data)
                .map_err(|e| format!("Write {}: {}", dest.display(), e))?;
            log::info!("[tools] Extracted '{}' → {}", name, dest.display());
            return Ok(());
        }
    }

    Err(format!("Entry ending with '{}' not found in zip", suffix))
}

// ── yt-dlp install / update ───────────────────────────────────────────────────

/// Download the latest yt-dlp binary into the managed tools directory.
/// Emits `tools:progress` events on `app`.
/// Returns the installed version string.
pub async fn install_ytdlp(app: &AppHandle) -> Result<String, String> {
    let dir = tools_dir(app);
    let dest = ytdlp_bin(&dir);

    emit_progress(app, "ytdlp", "fetching", 0, "Fetching latest yt-dlp release…");
    let (tag, url) = crate::yt_dlp::get_latest_release().await?;

    emit_progress(
        app,
        "ytdlp",
        "downloading",
        5,
        &format!("Downloading yt-dlp {}…", tag),
    );
    download_to_file(app, "ytdlp", &url, &dest).await?;

    #[cfg(unix)]
    set_executable(&dest)?;

    emit_progress(app, "ytdlp", "done", 100, &format!("yt-dlp {} installed", tag));
    Ok(tag)
}

// ── ffmpeg install ────────────────────────────────────────────────────────────

/// `(zip_url, entry_suffix_inside_zip)`
fn ffmpeg_download_source() -> Option<(&'static str, &'static str)> {
    if cfg!(windows) {
        Some((
            "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip",
            "bin/ffmpeg.exe",
        ))
    } else if cfg!(target_os = "macos") {
        // evermeet.cx — zip contains a single `ffmpeg` binary
        Some(("https://evermeet.cx/ffmpeg/getrelease/ffmpeg/zip", "ffmpeg"))
    } else {
        // Linux: tell user to use package manager
        None
    }
}

/// Download and install ffmpeg into the managed tools directory.
/// Emits `tools:progress` events on `app`.
pub async fn install_ffmpeg(app: &AppHandle) -> Result<(), String> {
    let (zip_url, entry_suffix) = ffmpeg_download_source()
        .ok_or_else(|| {
            "Automatic ffmpeg install is not supported on Linux. \
             Please run: sudo apt install ffmpeg   (or equivalent)".to_string()
        })?;

    let dir = tools_dir(app);
    let dest = ffmpeg_bin(&dir);

    emit_progress(app, "ffmpeg", "downloading", 0, "Downloading ffmpeg…");

    // Download zip to a temp file
    let tmp = std::env::temp_dir().join("qdm-ffmpeg-dl.zip");
    download_to_file(app, "ffmpeg", zip_url, &tmp).await?;

    emit_progress(app, "ffmpeg", "extracting", 88, "Extracting ffmpeg…");
    extract_from_zip(&tmp, entry_suffix, &dest)?;
    std::fs::remove_file(&tmp).ok();

    #[cfg(unix)]
    set_executable(&dest)?;

    emit_progress(app, "ffmpeg", "done", 100, "ffmpeg installed");
    Ok(())
}

// ── Unix helper ───────────────────────────────────────────────────────────────

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)
        .map_err(|e| e.to_string())?
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).map_err(|e| e.to_string())
}
