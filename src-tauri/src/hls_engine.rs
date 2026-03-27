/// HLS (HTTP Live Streaming) downloader
///
/// Flow:
///  1. Fetch manifest URL
///  2. If master playlist → select highest-bandwidth variant, fetch its media playlist
///  3. Parse media playlist → list of `MediaSegment`s
///  4. Download segments concurrently (up to HLS_WORKERS at a time)
///  5. AES-128-CBC decrypt if `#EXT-X-KEY` present
///  6. Concatenate .ts segment files in order → single output file
///  7. Report byte-level progress via EngineState so the existing progress reporter picks it up
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use futures::StreamExt;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::types::EngineState;

type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

// ── Constants ────────────────────────────────────────────────────────────────

/// Maximum concurrent HLS segment downloads.
const HLS_WORKERS: usize = 4;

/// Max retry attempts per segment (uses exponential back-off: 500ms → 1s → 2s → 4s).
const HLS_MAX_RETRIES: u32 = 3;

/// Max wait between retries.
const HLS_MAX_BACKOFF_MS: u64 = 8_000;

// ── Public entry point ───────────────────────────────────────────────────────

/// Download an HLS stream.
///
/// * `client`       – shared reqwest client
/// * `item_id`      – DownloadItem.id in the engine state (for progress updates)
/// * `manifest_url` – URL of the `.m3u8` master or media playlist
/// * `extra_headers`– forwarded headers (Cookie, Referer, …)
/// * `output_path`  – final assembled `.ts` file destination
/// * `temp_dir`     – scratch directory; created if absent, cleaned up on success
/// * `cancel_token` – cancel from pause/stop
/// * `state`        – shared engine state (updated as bytes are written)
///
/// Returns `Ok(total_bytes)` on success.
pub async fn run_hls(
    client: reqwest::Client,
    item_id: String,
    manifest_url: String,
    extra_headers: Option<HashMap<String, String>>,
    output_path: PathBuf,
    temp_dir: PathBuf,
    cancel_token: CancellationToken,
    state: Arc<Mutex<EngineState>>,
) -> Result<u64, String> {
    // ── 1. Fetch + parse manifest ─────────────────────────────────────────────

    let manifest_bytes =
        fetch_bytes(&client, &manifest_url, extra_headers.as_ref(), &cancel_token).await?;

    let media_pl = resolve_media_playlist(
        &client,
        &manifest_url,
        &manifest_bytes,
        extra_headers.as_ref(),
        &cancel_token,
    )
    .await?;

    let segments = media_pl.segments;
    let total = segments.len();
    if total == 0 {
        return Err("HLS playlist contains no segments".to_string());
    }

    // Update item.file_size to "segments × 1 MiB" so the progress reporter
    // can show a rough percentage while the real byte count accumulates.
    {
        let mut st = state.lock().await;
        if let Some(item) = st.downloads.get_mut(&item_id) {
            if item.file_size <= 0 {
                item.file_size = (total as i64) * 1_048_576;
            }
        }
    }

    // ── 2. Create temp directory ──────────────────────────────────────────────
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| e.to_string())?;

    // ── 3. Download segments in parallel ─────────────────────────────────────

    let semaphore = Arc::new(tokio::sync::Semaphore::new(HLS_WORKERS));
    // AES key cache: key_url → 16 raw bytes
    let key_cache: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut join_set = tokio::task::JoinSet::new();

    for (idx, seg) in segments.into_iter().enumerate() {
        let permit = semaphore.clone().acquire_owned().await.map_err(|e| e.to_string())?;
        let client2 = client.clone();
        let hdrs = extra_headers.clone();
        let base = manifest_url.clone();
        let td = temp_dir.clone();
        let ct = cancel_token.clone();
        let kc = Arc::clone(&key_cache);
        let st = Arc::clone(&state);
        let iid = item_id.clone();

        join_set.spawn(async move {
            let _permit = permit;

            if ct.is_cancelled() {
                return Err::<u64, String>("cancelled".to_string());
            }

            let seg_url = resolve_url(&base, &seg.uri)?;
            let part_path = td.join(format!("{:06}.ts", idx));

            let raw = download_segment_hls(&client2, &seg_url, hdrs.as_ref(), &ct).await?;

            // AES-128 decryption
            let data = match &seg.key {
                Some(key) if matches!(key.method, m3u8_rs::KeyMethod::AES128) => {
                    let key_uri = key
                        .uri
                        .as_deref()
                        .ok_or_else(|| "AES-128 key has no URI".to_string())?;
                    let key_url = resolve_url(&base, key_uri)?;
                    let key_bytes = {
                        let mut cache = kc.lock().await;
                        if let Some(k) = cache.get(&key_url) {
                            k.clone()
                        } else {
                            let k = fetch_bytes(&client2, &key_url, hdrs.as_ref(), &ct).await?;
                            cache.insert(key_url.clone(), k.clone());
                            k
                        }
                    };
                    if key_bytes.len() != 16 {
                        return Err(format!(
                            "HLS AES key is {} bytes, expected 16",
                            key_bytes.len()
                        ));
                    }
                    let iv: [u8; 16] = if let Some(iv_str) = &key.iv {
                        parse_hls_iv(iv_str).unwrap_or_else(|| seq_to_iv(idx as u64))
                    } else {
                        seq_to_iv(idx as u64)
                    };
                    decrypt_aes128_cbc(&key_bytes, &iv, &raw)?
                }
                _ => raw,
            };

            let bytes_written = data.len() as u64;
            tokio::fs::write(&part_path, &data)
                .await
                .map_err(|e| e.to_string())?;

            // Accumulate progress in item.downloaded (best-effort, under lock)
            {
                let mut st = st.lock().await;
                if let Some(item) = st.downloads.get_mut(&iid) {
                    item.downloaded = item.downloaded.saturating_add(bytes_written);
                }
            }

            Ok(bytes_written)
        });
    }

    // Drain join_set; collect errors but don't abort on the first one —
    // missing segments produce a gap in the assembled file, which the user
    // can re-download later.
    let mut segment_errors = 0usize;
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Err(ref e)) if e == "cancelled" => {
                join_set.shutdown().await;
                return Err("cancelled".to_string());
            }
            Ok(Err(e)) => {
                log::warn!("[HLS] segment error: {}", e);
                segment_errors += 1;
            }
            Err(e) => {
                log::warn!("[HLS] task panic: {}", e);
                segment_errors += 1;
            }
            Ok(Ok(_)) => {}
        }
    }

    if cancel_token.is_cancelled() {
        return Err("cancelled".to_string());
    }

    if segment_errors == total {
        return Err("All HLS segments failed to download".to_string());
    }

    // ── 4. Assemble segments ──────────────────────────────────────────────────

    let mut out_file = tokio::fs::File::create(&output_path)
        .await
        .map_err(|e| format!("Cannot create output file {:?}: {}", output_path, e))?;

    let mut total_bytes = 0u64;
    for idx in 0..total {
        let part_path = temp_dir.join(format!("{:06}.ts", idx));
        match tokio::fs::read(&part_path).await {
            Ok(data) => {
                out_file
                    .write_all(&data)
                    .await
                    .map_err(|e| e.to_string())?;
                total_bytes += data.len() as u64;
            }
            Err(e) => {
                log::warn!("[HLS] missing segment {:06}.ts during assembly: {}", idx, e);
            }
        }
    }
    out_file.flush().await.map_err(|e| e.to_string())?;
    drop(out_file);

    // Cleanup temp dir (non-fatal)
    tokio::fs::remove_dir_all(&temp_dir).await.ok();

    Ok(total_bytes)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Resolve a potentially-relative URI against a base URL.
pub fn resolve_url(base: &str, uri: &str) -> Result<String, String> {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return Ok(uri.to_string());
    }
    url::Url::parse(base)
        .and_then(|b| b.join(uri))
        .map(|u| u.to_string())
        .map_err(|e| format!("Cannot resolve '{}' against '{}': {}", uri, base, e))
}

/// Return true if `url` looks like an HLS manifest.
pub fn is_hls_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    let path_part = lower.split('?').next().unwrap_or(&lower);
    path_part.ends_with(".m3u8") || path_part.ends_with(".m3u")
}

/// Rename `.m3u8` / `.m3u` extension to `.ts`.
pub fn hls_output_filename(name: &str) -> String {
    let stem = name
        .trim_end_matches(".m3u8")
        .trim_end_matches(".m3u");
    if stem == name {
        // No HLS extension — keep name, append .ts
        format!("{}.ts", name)
    } else {
        format!("{}.ts", stem)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Fetch a master or media playlist and return the `MediaPlaylist`.
/// If `bytes` is a master playlist, selects the highest-bandwidth variant,
/// fetches it, and parses the result as a `MediaPlaylist`.
async fn resolve_media_playlist(
    client: &reqwest::Client,
    base_url: &str,
    bytes: &[u8],
    headers: Option<&HashMap<String, String>>,
    cancel: &CancellationToken,
) -> Result<m3u8_rs::MediaPlaylist, String> {
    match m3u8_rs::parse_playlist_res(bytes)
        .map_err(|e| format!("Failed to parse HLS manifest: {:?}", e))?
    {
        m3u8_rs::Playlist::MediaPlaylist(pl) => Ok(pl),
        m3u8_rs::Playlist::MasterPlaylist(master) => {
            let best = master
                .variants
                .into_iter()
                .filter(|v| !v.is_i_frame)
                .max_by_key(|v| v.bandwidth)
                .ok_or_else(|| "HLS master playlist has no usable variants".to_string())?;

            let variant_url = resolve_url(base_url, &best.uri)?;
            let variant_bytes = fetch_bytes(client, &variant_url, headers, cancel).await?;

            match m3u8_rs::parse_playlist_res(&variant_bytes)
                .map_err(|e| format!("Failed to parse HLS variant playlist: {:?}", e))?
            {
                m3u8_rs::Playlist::MediaPlaylist(pl) => Ok(pl),
                m3u8_rs::Playlist::MasterPlaylist(_) => {
                    Err("Unexpected nested master playlist in HLS variant".to_string())
                }
            }
        }
    }
}

/// Fetch bytes from `url`, injecting extra headers, with cancellation support.
async fn fetch_bytes(
    client: &reqwest::Client,
    url: &str,
    headers: Option<&HashMap<String, String>>,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, String> {
    if cancel.is_cancelled() {
        return Err("cancelled".to_string());
    }
    let mut req = client.get(url);
    if let Some(hdrs) = headers {
        for (k, v) in hdrs {
            req = req.header(k.as_str(), v.as_str());
        }
    }
    let resp = tokio::select! {
        _ = cancel.cancelled() => return Err("cancelled".to_string()),
        r = req.send() => r.map_err(|e| e.to_string())?,
    };
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let bytes = tokio::select! {
        _ = cancel.cancelled() => return Err("cancelled".to_string()),
        b = resp.bytes() => b.map_err(|e| e.to_string())?,
    };
    Ok(bytes.to_vec())
}

/// Download a single HLS segment with exponential backoff retry.
async fn download_segment_hls(
    client: &reqwest::Client,
    url: &str,
    headers: Option<&HashMap<String, String>>,
    cancel: &CancellationToken,
) -> Result<Vec<u8>, String> {
    let mut delay_ms: u64 = 500;
    let mut last_err = String::new();

    for attempt in 0..=HLS_MAX_RETRIES {
        if attempt > 0 {
            tokio::select! {
                _ = cancel.cancelled() => return Err("cancelled".to_string()),
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)) => {}
            }
            delay_ms = (delay_ms * 2).min(HLS_MAX_BACKOFF_MS);
        }

        // Streaming download with stall timeout
        let mut req_builder = client.get(url);
        if let Some(hdrs) = headers {
            for (k, v) in hdrs {
                req_builder = req_builder.header(k.as_str(), v.as_str());
            }
        }

        let resp = tokio::select! {
            _ = cancel.cancelled() => return Err("cancelled".to_string()),
            r = req_builder.send() => match r {
                Ok(r) => r,
                Err(e) => { last_err = e.to_string(); continue; }
            },
        };

        if resp.status().is_client_error() && resp.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(format!("HTTP {}", resp.status())); // non-retryable 4xx
        }
        if !resp.status().is_success() {
            last_err = format!("HTTP {}", resp.status());
            continue;
        }

        let mut stream = resp.bytes_stream();
        let mut data = Vec::new();
        let stall_timeout = tokio::time::Duration::from_secs(30);

        loop {
            let chunk_fut = stream.next();
            tokio::select! {
                _ = cancel.cancelled() => return Err("cancelled".to_string()),
                _ = tokio::time::sleep(stall_timeout) => {
                    last_err = "segment stalled (no data for 30s)".to_string();
                    break; // retry
                }
                chunk = chunk_fut => match chunk {
                    None => {
                        // Stream ended normally
                        return Ok(data);
                    }
                    Some(Ok(bytes)) => {
                        data.extend_from_slice(&bytes);
                    }
                    Some(Err(e)) => {
                        last_err = e.to_string();
                        break; // retry
                    }
                }
            }
        }
    }

    Err(format!("Segment failed after {} retries: {}", HLS_MAX_RETRIES, last_err))
}

/// Decrypt a byte slice with AES-128-CBC (PKCS#7 padding).
fn decrypt_aes128_cbc(key: &[u8], iv: &[u8; 16], data: &[u8]) -> Result<Vec<u8>, String> {
    if key.len() != 16 {
        return Err(format!("AES-128 key must be 16 bytes, got {}", key.len()));
    }
    let key_arr: &[u8; 16] = key.try_into().map_err(|_| "Key length mismatch".to_string())?;
    let cipher = Aes128CbcDec::new(key_arr.into(), iv.into());
    let mut buf = data.to_vec();
    cipher
        .decrypt_padded_mut::<cbc::cipher::block_padding::Pkcs7>(&mut buf)
        .map(|plain| plain.to_vec())
        .map_err(|e| format!("AES-128-CBC decrypt failed: {:?}", e))
}

/// Parse an HLS IV string (e.g. `"0x000000000000000000000000000000FF"`) into 16 bytes.
fn parse_hls_iv(iv_str: &str) -> Option<[u8; 16]> {
    let hex = iv_str.strip_prefix("0x").or_else(|| iv_str.strip_prefix("0X"))?;
    let bytes = hex::decode(hex).ok()?;
    if bytes.len() != 16 {
        return None;
    }
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes);
    Some(arr)
}

/// When no IV is in the key tag, HLS spec says IV = segment media sequence number
/// as a 128-bit big-endian integer.
fn seq_to_iv(seq: u64) -> [u8; 16] {
    let mut iv = [0u8; 16];
    iv[8..].copy_from_slice(&seq.to_be_bytes());
    iv
}

// ── DASH MPD Downloader ───────────────────────────────────────────────────────
//
// Supports VoD DASH manifests using SegmentTemplate with $Number$ tokens.
// No DRM.  Selects the highest-bandwidth video representation in the first
// Period and concatenates all segments into a single .mp4 file.

/// Returns true if `url` looks like a DASH MPD manifest.
pub fn is_dash_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    let path = lower.split('?').next().unwrap_or(&lower);
    path.ends_with(".mpd")
}

/// Rename `.mpd` extension to `.mp4` for the output file.
pub fn dash_output_filename(name: &str) -> String {
    if let Some(stem) = name.strip_suffix(".mpd") {
        format!("{}.mp4", stem)
    } else {
        format!("{}.mp4", name)
    }
}

/// Minimal parsed representation from a DASH MPD.
struct DashRepresentation {
    /// Bandwidth in bps (for quality selection)
    bandwidth: u64,
    /// Base URL for resolving relative segment paths (from `<BaseURL>` or manifest URL)
    base_url: String,
    /// URL template for segment files, e.g. `"video_$Number%05d$.m4s"`
    media_template: Option<String>,
    /// URL template for the initialization segment, e.g. `"video_init.mp4"`
    init_template: Option<String>,
    /// Absolute `<SegmentList>` init URL (alternative to template)
    init_list_url: Option<String>,
    /// `<SegmentList>` media URLs in order
    segment_list_urls: Vec<String>,
    /// First segment number (SegmentTemplate)
    start_number: u64,
    /// Total segments to download (computed from duration/timescale)
    num_segments: u64,
}

/// Download a DASH stream (no DRM, single-period, SegmentTemplate or SegmentList).
///
/// Returns `Ok(total_bytes)` on success.
pub async fn run_dash(
    client: reqwest::Client,
    item_id: String,
    manifest_url: String,
    extra_headers: Option<HashMap<String, String>>,
    output_path: PathBuf,
    temp_dir: PathBuf,
    cancel_token: CancellationToken,
    state: Arc<Mutex<EngineState>>,
) -> Result<u64, String> {
    // ── 1. Fetch + parse MPD ──────────────────────────────────────────────────

    let mpd_bytes =
        fetch_bytes(&client, &manifest_url, extra_headers.as_ref(), &cancel_token).await?;
    let mpd_text = String::from_utf8_lossy(&mpd_bytes);

    let repr = parse_mpd(&mpd_text, &manifest_url)?;

    let total = if !repr.segment_list_urls.is_empty() {
        repr.segment_list_urls.len() as u64
    } else {
        repr.num_segments
    };

    if total == 0 {
        return Err("DASH manifest has no segments".to_string());
    }

    // Update estimated file size (rough: segments × 1 MiB)
    {
        let mut st = state.lock().await;
        if let Some(item) = st.downloads.get_mut(&item_id) {
            if item.file_size <= 0 {
                item.file_size = (total as i64) * 1_048_576;
            }
        }
    }

    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| e.to_string())?;

    // ── 2. Download initialization segment (if present) ───────────────────────

    let mut seg_files: Vec<PathBuf> = Vec::new();

    if let Some(init_url_raw) = repr.init_list_url.as_deref()
        .or(repr.init_template.as_deref().map(|t| t).filter(|_| true))
    {
        // Resolve template ($RepresentationID$ not needed here — just resolve path)
        let init_url = if init_url_raw.starts_with("http") {
            init_url_raw.to_string()
        } else {
            resolve_url(&repr.base_url, init_url_raw)?
        };
        let init_path = temp_dir.join("init.mp4");
        let init_data =
            download_segment_hls(&client, &init_url, extra_headers.as_ref(), &cancel_token).await?;
        tokio::fs::write(&init_path, &init_data)
            .await
            .map_err(|e| e.to_string())?;
        seg_files.push(init_path);
    }

    // ── 3. Build segment URL list ─────────────────────────────────────────────

    let media_urls: Vec<String> = if !repr.segment_list_urls.is_empty() {
        repr.segment_list_urls
            .iter()
            .map(|u| {
                if u.starts_with("http") {
                    u.clone()
                } else {
                    resolve_url(&repr.base_url, u).unwrap_or_else(|_| u.clone())
                }
            })
            .collect()
    } else if let Some(tmpl) = &repr.media_template {
        (repr.start_number..(repr.start_number + total))
            .map(|n| expand_number_template(tmpl, n))
            .map(|rel| {
                if rel.starts_with("http") {
                    rel
                } else {
                    resolve_url(&repr.base_url, &rel).unwrap_or(rel)
                }
            })
            .collect()
    } else {
        return Err("DASH: no media template or segment list found".to_string());
    };

    // ── 4. Download media segments in parallel ────────────────────────────────

    let semaphore = Arc::new(tokio::sync::Semaphore::new(HLS_WORKERS));
    let mut join_set: tokio::task::JoinSet<Result<(usize, u64), String>> =
        tokio::task::JoinSet::new();

    for (idx, seg_url) in media_urls.into_iter().enumerate() {
        let permit = semaphore.clone().acquire_owned().await.map_err(|e| e.to_string())?;
        let client2 = client.clone();
        let hdrs = extra_headers.clone();
        let td = temp_dir.clone();
        let ct = cancel_token.clone();
        let st = Arc::clone(&state);
        let iid = item_id.clone();

        join_set.spawn(async move {
            let _permit = permit;
            if ct.is_cancelled() {
                return Err("cancelled".to_string());
            }
            let part_path = td.join(format!("{:06}.mp4", idx));
            let data = download_segment_hls(&client2, &seg_url, hdrs.as_ref(), &ct).await?;
            let bytes_written = data.len() as u64;
            tokio::fs::write(&part_path, &data)
                .await
                .map_err(|e| e.to_string())?;
            {
                let mut st = st.lock().await;
                if let Some(item) = st.downloads.get_mut(&iid) {
                    item.downloaded = item.downloaded.saturating_add(bytes_written);
                }
            }
            Ok((idx, bytes_written))
        });
    }

    let mut segment_errors = 0usize;
    let num_media = total as usize;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Err(ref e)) if e == "cancelled" => {
                join_set.shutdown().await;
                return Err("cancelled".to_string());
            }
            Ok(Err(e)) => {
                log::warn!("[DASH] segment error: {}", e);
                segment_errors += 1;
            }
            Err(e) => {
                log::warn!("[DASH] task panic: {}", e);
                segment_errors += 1;
            }
            Ok(Ok((idx, _))) => {
                seg_files.push(temp_dir.join(format!("{:06}.mp4", idx)));
            }
        }
    }

    if cancel_token.is_cancelled() {
        return Err("cancelled".to_string());
    }
    if segment_errors == num_media {
        return Err("All DASH segments failed to download".to_string());
    }

    // ── 5. Assemble: init + segments in order ─────────────────────────────────

    let mut out_file = tokio::fs::File::create(&output_path)
        .await
        .map_err(|e| format!("Cannot create output {:?}: {}", output_path, e))?;

    let mut total_bytes = 0u64;

    // Write init segment first
    if let Some(init_path) = temp_dir.join("init.mp4").to_str().map(|_| temp_dir.join("init.mp4")) {
        if tokio::fs::metadata(&init_path).await.is_ok() {
            if let Ok(data) = tokio::fs::read(&init_path).await {
                out_file.write_all(&data).await.map_err(|e| e.to_string())?;
                total_bytes += data.len() as u64;
            }
        }
    }

    // Write numbered media segments in order
    for idx in 0..num_media {
        let part = temp_dir.join(format!("{:06}.mp4", idx));
        match tokio::fs::read(&part).await {
            Ok(data) => {
                out_file.write_all(&data).await.map_err(|e| e.to_string())?;
                total_bytes += data.len() as u64;
            }
            Err(e) => {
                log::warn!("[DASH] missing segment {:06}.mp4 during assembly: {}", idx, e);
            }
        }
    }

    out_file.flush().await.map_err(|e| e.to_string())?;
    drop(out_file);

    tokio::fs::remove_dir_all(&temp_dir).await.ok();

    Ok(total_bytes)
}

// ── DASH MPD parser ───────────────────────────────────────────────────────────

/// Parse a DASH MPD document and return the best (highest bandwidth) video representation.
fn parse_mpd(xml: &str, manifest_url: &str) -> Result<DashRepresentation, String> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| format!("Failed to parse MPD: {}", e))?;
    let root = doc.root_element();

    // Base URL directory from the manifest URL itself
    let manifest_base = if let Some(idx) = manifest_url.rfind('/') {
        manifest_url[..=idx].to_string()
    } else {
        manifest_url.to_string()
    };

    // Total duration: MPD@mediaPresentationDuration ("PT1H30M0.0S")
    let mpd_duration_secs = root
        .attribute("mediaPresentationDuration")
        .and_then(parse_iso8601_duration)
        .unwrap_or(0.0);

    // Global BaseURL
    let global_base = node_child_text(&root, "BaseURL")
        .map(|s| resolve_inherit(&s, &manifest_base))
        .unwrap_or_else(|| manifest_base.clone());

    // Find the first Period
    let period = root
        .children()
        .find(|n: &roxmltree::Node| n.tag_name().name() == "Period")
        .ok_or_else(|| "MPD has no Period element".to_string())?;

    let period_duration_secs = period
        .attribute("duration")
        .and_then(parse_iso8601_duration)
        .unwrap_or(mpd_duration_secs);

    let period_base = node_child_text(&period, "BaseURL")
        .map(|s| resolve_inherit(&s, &global_base))
        .unwrap_or_else(|| global_base.clone());

    // Find best video AdaptationSet (prefer video contentType/mimeType, else first)
    let adapt_set = period
        .children()
        .filter(|n: &roxmltree::Node| n.tag_name().name() == "AdaptationSet")
        .fold(None::<roxmltree::Node>, |best, n| {
            let is_video = n.attribute("contentType")
                .map(|t| t.to_lowercase().contains("video"))
                .unwrap_or(false)
                || n.attribute("mimeType")
                    .map(|t| t.to_lowercase().starts_with("video/"))
                    .unwrap_or(false);
            match best {
                None => Some(n),
                Some(b) => if is_video { Some(n) } else { Some(b) },
            }
        })
        .ok_or_else(|| "DASH Period has no AdaptationSets".to_string())?;

    let adapt_base = node_child_text(&adapt_set, "BaseURL")
        .map(|s| resolve_inherit(&s, &period_base))
        .unwrap_or_else(|| period_base.clone());

    // Collect SegmentTemplate attributes from AdaptationSet level (may be inherited)
    let parent_tmpl = node_child_attrs(&adapt_set, "SegmentTemplate");

    // Best Representation by bandwidth
    let best_repr = adapt_set
        .children()
        .filter(|n: &roxmltree::Node| n.tag_name().name() == "Representation")
        .max_by_key(|n: &roxmltree::Node| {
            n.attribute("bandwidth")
                .and_then(|s: &str| s.parse::<u64>().ok())
                .unwrap_or(0)
        })
        .ok_or_else(|| "DASH AdaptationSet has no Representation".to_string())?;

    let bandwidth = best_repr.attribute("bandwidth")
        .and_then(|s: &str| s.parse::<u64>().ok())
        .unwrap_or(0);
    let repr_id = best_repr.attribute("id").unwrap_or("0").to_string();

    let repr_base = node_child_text(&best_repr, "BaseURL")
        .map(|s| resolve_inherit(&s, &adapt_base))
        .unwrap_or_else(|| adapt_base.clone());

    // Merge SegmentTemplate: Representation attrs override AdaptationSet attrs
    let repr_tmpl = node_child_attrs(&best_repr, "SegmentTemplate");
    let tmpl_attrs: HashMap<String, String> = {
        let mut m = parent_tmpl.unwrap_or_default();
        if let Some(r) = repr_tmpl { m.extend(r); }
        m
    };

    if !tmpl_attrs.is_empty() {
        let media = tmpl_attrs.get("media")
            .map(|s| s.replace("$RepresentationID$", &repr_id));
        let init = tmpl_attrs.get("initialization")
            .map(|s| s.replace("$RepresentationID$", &repr_id));
        let start_number: u64 = tmpl_attrs.get("startNumber")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let duration: f64 = tmpl_attrs.get("duration")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let timescale: f64 = tmpl_attrs.get("timescale")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);

        // Count segments from SegmentTimeline or duration/timescale
        let timeline_count = find_segment_template_timeline_count(&best_repr, &adapt_set);

        let num_segments = if timeline_count > 0 {
            timeline_count
        } else if duration > 0.0 && period_duration_secs > 0.0 {
            ((period_duration_secs * timescale) / duration).ceil() as u64
        } else {
            return Err("DASH: cannot determine segment count".to_string());
        };

        return Ok(DashRepresentation {
            bandwidth,
            base_url: repr_base,
            media_template: media,
            init_template: init,
            init_list_url: None,
            segment_list_urls: Vec::new(),
            start_number,
            num_segments,
        });
    }

    // SegmentList approach
    let seg_list_opt = best_repr
        .children()
        .find(|n: &roxmltree::Node| n.tag_name().name() == "SegmentList");

    if let Some(seg_list) = seg_list_opt {
        let init_url = seg_list
            .children()
            .find(|n: &roxmltree::Node| n.tag_name().name() == "Initialization")
            .and_then(|n: roxmltree::Node| n.attribute("sourceURL").map(String::from));

        let segment_urls: Vec<String> = seg_list
            .children()
            .filter(|n: &roxmltree::Node| n.tag_name().name() == "SegmentURL")
            .filter_map(|n: roxmltree::Node| n.attribute("media").map(String::from))
            .collect();

        return Ok(DashRepresentation {
            bandwidth,
            base_url: repr_base,
            media_template: None,
            init_template: None,
            init_list_url: init_url,
            segment_list_urls: segment_urls,
            start_number: 1,
            num_segments: 0,
        });
    }

    Err("DASH: Representation has neither SegmentTemplate nor SegmentList".to_string())
}

/// Get the text content of the first named child element.
fn node_child_text(node: &roxmltree::Node, tag: &str) -> Option<String> {
    node.children()
        .find(|n: &roxmltree::Node| n.tag_name().name() == tag)
        .and_then(|n: roxmltree::Node| n.text().map(String::from))
}

/// Extract all attributes of the first named child element into a HashMap.
fn node_child_attrs(node: &roxmltree::Node, tag: &str) -> Option<HashMap<String, String>> {
    node.children()
        .find(|n: &roxmltree::Node| n.tag_name().name() == tag)
        .map(|n: roxmltree::Node| {
            n.attributes()
                .map(|a| (a.name().to_string(), a.value().to_string()))
                .collect()
        })
}

/// If `s` is an absolute URL, return it unchanged; otherwise resolve against `base`.
fn resolve_inherit(s: &str, base: &str) -> String {
    if s.starts_with("http://") || s.starts_with("https://") {
        s.to_string()
    } else {
        resolve_url(base, s).unwrap_or_else(|_| s.to_string())
    }
}

/// Count total segments from SegmentTimeline elements in SegmentTemplate nodes
/// of either the Representation or the AdaptationSet.
fn find_segment_template_timeline_count(
    repr: &roxmltree::Node,
    adapt_set: &roxmltree::Node,
) -> u64 {
    let count_in = |node: &roxmltree::Node| -> u64 {
        node.children()
            .find(|n: &roxmltree::Node| n.tag_name().name() == "SegmentTemplate")
            .and_then(|tmpl: roxmltree::Node| {
                tmpl.children()
                    .find(|n: &roxmltree::Node| n.tag_name().name() == "SegmentTimeline")
                    .map(|tl: roxmltree::Node| {
                        tl.children()
                            .filter(|n: &roxmltree::Node| n.tag_name().name() == "S")
                            .map(|s: roxmltree::Node| {
                                let r: u64 = s.attribute("r")
                                    .and_then(|v: &str| v.parse().ok())
                                    .unwrap_or(0);
                                r + 1
                            })
                            .sum::<u64>()
                    })
            })
            .unwrap_or(0)
    };
    let c = count_in(repr);
    if c > 0 { c } else { count_in(adapt_set) }
}

/// Expand `$Number$` and `$Number%05d$` tokens in a DASH template string.
fn expand_number_template(template: &str, number: u64) -> String {
    // Match $Number%Nd$ or $Number$
    let mut result = template.to_string();
    // Handle format specifier: $Number%05d$
    while let Some(start) = result.find("$Number%") {
        if let Some(end) = result[start..].find("d$") {
            let fmt_part = &result[start + 8..start + end]; // e.g. "05"
            let width: usize = fmt_part.parse().unwrap_or(0);
            let formatted = if width > 0 {
                format!("{:0>width$}", number, width = width)
            } else {
                number.to_string()
            };
            result = format!("{}{}{}", &result[..start], formatted, &result[start + end + 2..]);
            break;
        } else {
            break;
        }
    }
    result = result.replace("$Number$", &number.to_string());
    result
}

/// Parse ISO 8601 duration string ("PT1H30M5.5S") into total seconds.
fn parse_iso8601_duration(s: &str) -> Option<f64> {
    let s = s.strip_prefix("PT")?; // only handle time portion for VoD
    let mut total = 0.0f64;
    let mut buf = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            buf.push(ch);
        } else {
            let v: f64 = buf.parse().ok()?;
            buf.clear();
            total += match ch {
                'H' => v * 3600.0,
                'M' => v * 60.0,
                'S' => v,
                _ => 0.0,
            };
        }
    }
    Some(total)
}
