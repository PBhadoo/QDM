<p align="center">
  <img src="https://img.shields.io/badge/⚡-QDM-6c5ce7?style=for-the-badge&logoColor=white" alt="QDM" />
</p>

<h1 align="center">Quantum Download Manager</h1>

<p align="center">
  <strong>A modern, open-source download manager for Windows, macOS, and Linux</strong><br>
  <em>Multi-segment acceleration • YouTube & media • Browser integration • System tray • Beautiful dark UI</em>
</p>

<p align="center">
  <a href="https://github.com/PBhadoo/QDM/releases">
    <img src="https://img.shields.io/github/v/release/PBhadoo/QDM?style=flat-square&color=6c5ce7" alt="Latest Release" />
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/badge/built%20with-Tauri%202-24c8db?style=flat-square" alt="Tauri 2" />
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License" />
</p>

<p align="center">
  <a href="#-features">Features</a> •
  <a href="#-installation">Installation</a> •
  <a href="#-browser-extension">Extension</a> •
  <a href="#-configuration">Configuration</a> •
  <a href="#-development">Development</a> •
  <a href="#-credits">Credits</a>
</p>

---

## ✨ Features

### Core Download Engine
- **⚡ Multi-Segment Acceleration** — Splits files into up to 32 parallel segments for maximum speed, each with its own HTTP connection and Range header
- **⏸️ Pause & Resume** — Full pause/resume with persistent segment state — survives app restarts and crashes
- **🔄 Auto-Retry** — Failed segments retry automatically; stalled connections recover gracefully
- **🔍 Smart Probing** — HEAD request before download to detect file size, filename, and server resumability support
- **📊 Real-Time Progress** — Per-segment progress visualization, speed (with exponential moving average), ETA, and downloaded bytes

### Media & Video
- **🎬 YouTube Downloads** — yt-dlp integration with in-app quality selection (Best, 1080p, 720p, 480p, 360p, Audio Only)
- **📺 HLS Streaming** — Full HLS (HTTP Live Streaming) engine with AES-128 decryption and multi-bitrate support
- **🎵 DASH Streaming** — MPEG-DASH parser for adaptive video/audio streams
- **🖥️ Media Grabber** — Detects videos, audio, and streams from any website you browse via the extension
- **🍪 Cookie Support** — Passes browser cookies to yt-dlp for age-restricted and member-only content

### Browser Integration
- **🌐 Chrome Extension (MV3)** — Auto-intercepts downloads and media streams, works just like IDM
- **🔗 Context Menu** — Right-click any link or media → *Download with QDM ⚡*
- **📋 Clipboard Monitor** — Automatically detects download URLs copied to clipboard
- **🎥 YouTube Banner** — Overlay banner on YouTube pages for one-click video download
- **🔔 Notifications** — Desktop notifications when downloads complete

### Download Management
- **📂 Smart Categories** — Auto-sorts by type: Videos, Music, Documents, Programs, Archives, Other
- **🔎 Search & Filter** — Instant search across all downloads by filename or URL
- **⚙️ Download Queues** — Create named queues with concurrency limits and scheduled execution times
- **🔐 Auth Support** — Handles HTTP 401 challenges with a credential dialog
- **🔗 Expired Link Detection** — Detects signed/expiring URLs and prompts to re-fetch from source
- **🗑️ Batch Actions** — Pause all, resume all, clear completed, remove selected

### App & System
- **🖥️ System Tray** — Minimize to tray, background operation, tray icon with download status
- **🔔 OS Notifications** — Native desktop notifications for completed downloads
- **🔄 Auto-Updater** — In-app update check on startup; one-click download and install of new versions
- **⬇️ Tool Management** — Auto-installs yt-dlp and ffmpeg on first launch; in-app updater for both
- **🎨 Frameless Dark UI** — Custom titlebar, segment progress bars, clean dark theme throughout
- **🌍 Cross-Platform** — Windows (x64), macOS (Apple Silicon), Linux (x64)

---

## 🏗️ Architecture

QDM is built with **Tauri 2** — a Rust backend powering a React/TypeScript frontend, with a native Axum HTTP server for browser extension communication.

| Layer | Technology |
|-------|-----------|
| Desktop framework | Tauri 2 (Rust) |
| UI | React 18 + TypeScript + Vite 5 |
| Styling | Tailwind CSS 3 |
| State management | Zustand |
| HTTP server (extension API) | Axum 0.7 + WebSocket |
| Download engine | Custom Rust multi-segment engine |
| Media downloader | yt-dlp (auto-managed) |
| Video processing | ffmpeg (auto-managed) |
| HLS/DASH engine | Custom Rust streaming parser |
| Clipboard | arboard |
| Icons | Lucide React |

### Download Engine Flow

```
┌──────────────────────────────────────────────────┐
│                   File (e.g. 100 MB)             │
├──────────┬──────────┬──────────┬─────────────────┤
│ Segment 1│ Segment 2│ Segment 3│    Segment 4    │
│  25 MB   │  25 MB   │  25 MB   │     25 MB       │
│ ████████░│ ██████░░░│ ████░░░░ │  ██░░░░░░░      │
│  Conn #1 │  Conn #2 │  Conn #3 │   Conn #4       │
└──────────┴──────────┴──────────┴─────────────────┘
```

1. **Probe** — HEAD request: file size, resumability, Content-Disposition filename
2. **Split** — Divided into N segments (configurable 1–32) with byte-range boundaries
3. **Parallel** — Each segment downloads via a separate HTTP connection with `Range` header
4. **Progress** — Speed computed with exponential moving average; per-segment state persisted to disk
5. **Assemble** — Segments merged sequentially into the final file
6. **Recovery** — On restart, already-downloaded byte ranges are skipped; only missing parts re-fetched

### Browser Extension Communication

```
Browser Extension
      │
      │  HTTP POST /download  (port 8597)
      │  HTTP POST /media
      │  WebSocket /ws  (real-time sync)
      ▼
Rust Axum Server (browser_monitor.rs)
      │
      │  Tauri event: browser:download
      │  Tauri event: browser:vid-download
      ▼
lib.rs → DownloadEngine → emits download:added → React UI
```

The extension authenticates every request with a session token (`X-QDM-Token`) obtained from `/sync` on connection. The token rotates between sessions.

---

## 📦 Installation

### Pre-built Releases

Download the latest version from the [Releases page](https://github.com/PBhadoo/QDM/releases):

| Platform | File | Notes |
|----------|------|-------|
| 🪟 Windows | `*_x64-setup.exe` | NSIS installer — recommended |
| 🪟 Windows | `*_x64_en-US.msi` | MSI installer |
| 🍎 macOS | `*.dmg` | Apple Silicon (M1/M2/M3) |
| 🐧 Linux | `*.AppImage` | Universal, no install needed |
| 🐧 Linux | `*.deb` | Debian / Ubuntu |

> **macOS:** The app is not code-signed. If macOS blocks it, run:
> ```bash
> xattr -cr /Applications/Quantum\ Download\ Manager.app
> ```

> **Linux AppImage:** Make executable before running:
> ```bash
> chmod +x Quantum_Download_Manager_*.AppImage
> ./Quantum_Download_Manager_*.AppImage
> ```

### yt-dlp & ffmpeg

QDM **auto-installs** yt-dlp and ffmpeg on first launch if they are not found. A progress banner is shown while they download. You can also manage them manually via **Settings → Tools**.

---

## 🌐 Browser Extension

The Chrome extension integrates QDM with your browser — intercepting file downloads and detecting media streams automatically, just like IDM.

### Install (Chrome / Brave / Edge)

1. Open `chrome://extensions`
2. Enable **Developer mode** (top-right toggle)
3. Click **Load unpacked**
4. Select the `extension/chrome/` folder from the QDM repository

> Firefox users: `extension/firefox/` is a Manifest V2 version (load via `about:debugging`).

### How It Works

When QDM is running, the extension connects to it over WebSocket on port 8597. A session token is exchanged at startup for all subsequent requests.

### Extension Features

| Feature | Description |
|---------|-------------|
| **Auto-intercept** | Captures `.zip`, `.exe`, `.pdf`, `.mp4`, `.mkv`, `.dmg`, `.iso` and more |
| **Toggle** | Enable/disable auto-intercept from the popup without reloading |
| **YouTube** | Hover over any YouTube video for a QDM download banner |
| **Context menu** | Right-click any link or media → *Download with QDM ⚡* |
| **Manual URL** | Paste any URL directly in the popup to queue it |
| **HLS/DASH detection** | `inject.js` runs in the page context to detect media streams before they load |
| **Cookie passing** | Passes cookies to QDM for authenticated/age-restricted downloads |
| **Notifications** | Chrome notification when a queued download completes |

### Permissions Used

| Permission | Purpose |
|-----------|---------|
| `webRequest` | Intercept and inspect network requests |
| `downloads` | Monitor browser-initiated downloads |
| `cookies` | Pass site cookies to yt-dlp for auth |
| `contextMenus` | Right-click "Download with QDM" |
| `notifications` | Download completion alerts |
| `storage` | Persist settings and session token |
| `tabs`, `scripting` | Inject YouTube banner and media detector |

---

## ⚙️ Configuration

Open **Settings** (sidebar or `Ctrl+,`) to configure QDM.

### Storage

| Option | Default | Description |
|--------|---------|-------------|
| Download Directory | `~/Downloads` | Default save location for all downloads |

### Performance

| Option | Default | Range | Description |
|--------|---------|-------|-------------|
| Max Concurrent Downloads | 3 | 1–10 | How many files download simultaneously |
| Segments Per Download | 8 | 1–32 | Parallel connections per file (1 = safe/no splitting) |
| Speed Limit | 0 (unlimited) | KB/s | Global bandwidth cap across all downloads |

### General

| Option | Default | Description |
|--------|---------|-------------|
| Show Notifications | On | OS notification when a download completes |
| Minimize to Tray | On | Close button minimizes to system tray instead of quitting |

### Tools

| Tool | Description |
|------|-------------|
| **yt-dlp** | Video downloader binary — install, update, or check version |
| **ffmpeg** | Video processing — required for 1080p+ quality merging |
| **Browser for cookies** | Choose which browser's cookie jar yt-dlp uses (Chrome, Firefox, Edge, Brave, Opera, Chromium) |

---

## 🎬 Adding a Download

### Regular Files

1. Click **+ New Download** or press `Ctrl+N`
2. Paste the URL — QDM probes it instantly for filename, size, and resumability
3. Edit filename or save path if needed
4. Adjust segment count (default 8; use 1 for servers that block parallel requests)
5. Click **Download**

### YouTube / Video URLs

QDM auto-detects YouTube, Shorts, Reels, and other yt-dlp-supported URLs and shows quality presets:

| Preset | Notes |
|--------|-------|
| Best Quality | Highest available resolution + audio, ffmpeg required |
| 1080p HD | Full HD, ffmpeg required for muxing |
| 720p HD | HD, ffmpeg required |
| 480p SD | Standard definition, no ffmpeg needed |
| 360p | Low quality, no ffmpeg needed |
| Audio Only | MP3 / M4A audio extraction |

### From the Browser Extension

- Click the QDM icon in your toolbar → paste a URL or it auto-fills from the active tab
- Right-click any download link → **Download with QDM ⚡**
- Visit YouTube → hover the video → click the QDM banner

---

## 🔄 Auto-Updater

QDM checks for updates automatically on startup. If a new version is available:

- A banner appears at the top: **"QDM v1.x.x is available — Install now"**
- Click **Install now** to download and run the installer in-place
- On **Windows**: the NSIS installer launches and QDM exits to allow replacement
- On **macOS**: the `.dmg` opens — drag to Applications to complete
- On **Linux**: the `.AppImage` is downloaded and the folder opens

You can also check manually: **About → Check for Updates**.

---

## 🛠️ Development

### Prerequisites

- [Node.js](https://nodejs.org/) 24+
- [Rust](https://rustup.rs/) (stable, 1.77.2+)
- [Tauri v2 prerequisites](https://tauri.app/start/prerequisites/) for your platform

**Linux extra:**
```bash
sudo apt-get install -y \
  libwebkit2gtk-4.1-dev librsvg2-dev patchelf \
  libgtk-3-dev libayatana-appindicator3-dev
```

### Setup

```bash
git clone https://github.com/PBhadoo/QDM.git
cd QDM
npm install
```

### Run in Dev Mode

```bash
npm run tauri:dev
```

This starts the Vite dev server with hot-reload and the Tauri Rust backend simultaneously.

### Build

```bash
# Current platform
npm run tauri:build

# Platform-specific
npm run tauri:build:win    # Windows — MSI + NSIS (.exe)
npm run tauri:build:mac    # macOS  — .dmg (Apple Silicon)
npm run tauri:build:linux  # Linux  — .AppImage + .deb
```

### Build Scripts Reference

| Command | Description |
|---------|-------------|
| `npm run dev` | Vite dev server only (UI preview, no Tauri) |
| `npm run tauri:dev` | Full Tauri dev mode with Rust backend + hot-reload |
| `npm run tauri:build` | Production build for current platform |
| `npm run tauri:build:win` | Windows cross-target (x86_64-pc-windows-msvc) |
| `npm run tauri:build:linux` | Linux cross-target (x86_64-unknown-linux-gnu) |
| `npm run tauri:build:mac` | macOS cross-target (aarch64-apple-darwin) |
| `npm run fetch-yt-dlp` | Download platform yt-dlp binary into `src-tauri/resources/` |

### Project Structure

```
QDM/
├── src/                          # React frontend (TypeScript)
│   ├── App.tsx                   # Root — event listeners, update banner, layout
│   ├── components/
│   │   ├── TitleBar.tsx          # Custom frameless title bar (minimize/maximize/close)
│   │   ├── Sidebar.tsx           # Category navigation + status counts
│   │   ├── Toolbar.tsx           # Actions: new, pause-all, resume-all, search
│   │   ├── DownloadList.tsx      # Download rows with segment progress bars
│   │   ├── NewDownloadDialog.tsx # Add download — probe + quality + segments
│   │   ├── VideoQualityDialog.tsx# yt-dlp quality picker
│   │   ├── VideoGrabber.tsx      # Media grabber panel (intercepted streams)
│   │   ├── SettingsDialog.tsx    # All app settings + tools management
│   │   ├── AboutDialog.tsx       # Version info + update checker
│   │   ├── AuthDialog.tsx        # HTTP 401 credential prompt
│   │   ├── LinkExpiredDialog.tsx # Expired/signed URL recovery
│   │   └── YtdlpLogPanel.tsx     # yt-dlp stdout/stderr log viewer
│   ├── store/
│   │   └── useDownloadStore.ts   # Zustand global state
│   └── types/
│       └── download.ts           # Shared TypeScript types
│
├── src-tauri/                    # Rust/Tauri backend
│   ├── src/
│   │   ├── lib.rs                # All Tauri commands + app setup + event wiring
│   │   ├── download_engine.rs    # Multi-segment download engine with retry/recover
│   │   ├── hls_engine.rs         # HLS/DASH parser with AES-128 decryption
│   │   ├── yt_dlp.rs             # yt-dlp binary integration + progress parsing
│   │   ├── browser_monitor.rs    # Axum HTTP server + WebSocket hub (port 8597)
│   │   ├── clipboard_monitor.rs  # Clipboard URL watcher
│   │   ├── queue_manager.rs      # Download queue scheduler
│   │   ├── tools.rs              # yt-dlp + ffmpeg auto-install with progress
│   │   ├── types.rs              # Shared Rust types (serde)
│   │   └── main.rs               # Tauri entry point
│   ├── capabilities/
│   │   └── default.json          # Tauri permission capabilities
│   ├── icons/                    # App icons (all platforms)
│   ├── tauri.conf.json           # Tauri app config
│   └── Cargo.toml                # Rust dependencies
│
├── extension/
│   ├── chrome/                   # Chrome MV3 extension
│   │   ├── background.js         # Service worker: intercept, WebSocket, context menu
│   │   ├── content.js            # Content script: YouTube banner, link hover
│   │   ├── inject.js             # Page-world script: early media stream detection
│   │   ├── popup.html            # Extension popup UI
│   │   ├── popup.js              # Popup logic: URL input, status, settings
│   │   ├── manifest.json         # MV3 manifest
│   │   └── icons/                # Extension icons
│   └── firefox/
│       └── manifest.json         # Firefox MV2 manifest
│
└── scripts/
    └── fetch-yt-dlp.js           # Downloads correct yt-dlp binary into resources/
```

### Tauri Commands Reference

<details>
<summary>View all backend commands</summary>

| Command | Description |
|---------|-------------|
| `download_add` | Queue a new download (URL, filename, path, segments) |
| `download_start` | Start a queued download |
| `download_pause` | Pause an active download |
| `download_resume` | Resume a paused download |
| `download_cancel` | Cancel and clean up a download |
| `download_remove` | Remove from list (optionally delete file) |
| `download_retry` | Retry a failed download |
| `download_get_all` | Fetch all download records |
| `download_open_file` | Open completed file with default app |
| `download_open_folder` | Reveal file in Finder/Explorer |
| `download_pause_all` | Pause all active downloads |
| `download_resume_all` | Resume all paused downloads |
| `download_probe` | Probe URL: size, resumability, filename |
| `download_provide_auth` | Submit credentials for a 401 challenge |
| `download_reopen_source` | Open original source page to refresh expired link |
| `browser_get_media_list` | Get list of media intercepted by extension |
| `browser_clear_media` | Clear the intercepted media list |
| `browser_download_media` | Send intercepted media to download engine |
| `browser_get_status` | Check browser monitor connectivity |
| `browser_set_config` | Enable/disable browser interception |
| `clipboard_get_enabled` | Is clipboard monitoring on? |
| `clipboard_set_enabled` | Toggle clipboard monitoring |
| `queue_get_all` | Fetch all download queues |
| `queue_create` | Create a new named queue |
| `queue_update` | Update queue settings |
| `queue_delete` | Delete a queue |
| `queue_add_downloads` | Assign downloads to a queue |
| `queue_set_schedule` | Set scheduled start time for a queue |
| `config_get` | Read app configuration |
| `config_set` | Write app configuration |
| `update_check` | Check GitHub for a newer QDM release |
| `update_download_install` | Download + run platform installer for a release |
| `ytdlp_list_formats` | List available quality formats for a URL |
| `ytdlp_get_version` | Get installed yt-dlp version |
| `ytdlp_check_update` | Check if yt-dlp has a newer release |
| `ytdlp_download_update` | Download and install latest yt-dlp |
| `tools_get_status` | Check yt-dlp + ffmpeg install status |
| `tools_install_ytdlp` | Auto-install yt-dlp |
| `tools_install_ffmpeg` | Auto-install ffmpeg |

</details>

### Release

Bump the version in these three files to the new version number:
- `package.json` → `"version"`
- `src-tauri/Cargo.toml` → `version`
- `src-tauri/tauri.conf.json` → `"version"`

Then tag and push:

```bash
git tag v1.0.4
git push origin v1.0.4
```

GitHub Actions will build all three platforms and publish the release automatically.

---

## 🙏 Credits

### XDM (Xtreme Download Manager)
**By [subhra74](https://github.com/subhra74)** — [github.com/subhra74/xdm](https://github.com/subhra74/xdm)

QDM's multi-segment download architecture, segment splitting, Range-header resumption, and crash-recovery patterns are directly inspired by XDM's brilliant open-source engine. QDM is a spiritual successor built on a modern Rust + Tauri stack.

### IDM (Internet Download Manager)
**By [Tonec Inc.](https://www.internetdownloadmanager.com/)**

IDM pioneered the browser-integration model and established the gold standard for segmented download acceleration. QDM's extension design, context menu integration, and IDM-style popup follow the patterns they established.

### Open Source Dependencies

| Project | Use |
|---------|-----|
| [Tauri](https://tauri.app/) | Cross-platform desktop framework |
| [React](https://react.dev/) | UI library |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | Video & media downloader |
| [ffmpeg](https://ffmpeg.org/) | Video processing and muxing |
| [Axum](https://github.com/tokio-rs/axum) | Async Rust web framework |
| [Tokio](https://tokio.rs/) | Async runtime |
| [reqwest](https://github.com/seanmonstar/reqwest) | HTTP client |
| [Tailwind CSS](https://tailwindcss.com/) | Utility-first CSS |
| [Zustand](https://github.com/pmndrs/zustand) | State management |
| [Lucide Icons](https://lucide.dev/) | Icon library |
| [m3u8-rs](https://github.com/rutgersc/m3u8-rs) | HLS playlist parsing |

---

## 📄 License

MIT License — see [LICENSE](LICENSE) for details.

---

<p align="center">
  Made with ❤️ by <a href="https://github.com/PBhadoo">Parveen Bhadoo</a>
</p>
