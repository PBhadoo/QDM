<p align="center">
  <img src="https://img.shields.io/badge/⚡-QDM-6c5ce7?style=for-the-badge&logoColor=white" alt="QDM" />
</p>

<h1 align="center">Quantum Download Manager</h1>

<p align="center">
  <strong>A modern, open-source download manager for Windows, macOS, and Linux</strong><br>
  <em>Multi-segment downloading • YouTube/media support • Browser integration • Beautiful UI</em>
</p>

<p align="center">
  <a href="https://github.com/PBhadoo/QDM/releases">
    <img src="https://img.shields.io/github/v/release/PBhadoo/QDM?style=flat-square&color=6c5ce7" alt="Latest Release" />
  </a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=flat-square" alt="Platform" />
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License" />
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#browser-extension">Extension</a> •
  <a href="#development">Development</a> •
  <a href="#credits">Credits</a>
</p>

---

## ✨ Features

- **⚡ Multi-Segment Downloads** — Splits files into parallel segments for maximum speed
- **🎬 YouTube & Media** — Download YouTube videos via yt-dlp with quality selection
- **🌐 Browser Integration** — Chrome extension that automatically intercepts downloads and streams
- **⏸️ Pause & Resume** — Full pause/resume support, even after restarting the app
- **📂 Smart Categories** — Auto-sorts downloads by type (video, audio, documents, programs, archives)
- **🎨 Modern Dark UI** — Clean, frameless interface with real-time segment visualization
- **🔄 Auto-Retry** — Failed segments retry automatically; stalled segments recover gracefully
- **🛡️ Auth Support** — Handles 401 prompts with credential dialog
- **🔗 Expired Link Detection** — Detects signed/expiring URLs and alerts you
- **⚙️ Configurable** — Max segments, concurrent downloads, speed limit, custom yt-dlp path
- **🖥️ System Tray** — Runs in the tray, shows download notifications

## 🏗️ Architecture

QDM is built with **Tauri 2** (Rust backend + React frontend):

| Layer | Technology |
|-------|-----------|
| Desktop framework | Tauri 2 (Rust) |
| UI | React 18 + TypeScript + Vite |
| Styling | Tailwind CSS |
| State | Zustand |
| HTTP server | Axum (for browser extension) |
| Download engine | Custom Rust multi-segment engine |
| Media | yt-dlp + ffmpeg (auto-installed) |
| HLS/DASH | Custom Rust HLS/DASH engine |

### Download Engine

```
┌──────────────────────────────────────────────┐
│                  File (100 MB)                │
├───────────┬───────────┬──────────┬───────────┤
│ Segment 1 │ Segment 2 │ Segment 3│ Segment 4 │
│  25 MB    │  25 MB    │  25 MB   │  25 MB    │
│ ████████░ │ ██████░░░ │ ████░░░░ │ ██░░░░░░░ │
│ Conn #1   │ Conn #2   │ Conn #3  │ Conn #4   │
└───────────┴───────────┴──────────┴───────────┘
```

1. **Probe** — HEAD request to get file size, resumability, filename
2. **Split** — File divided into N segments based on configuration
3. **Parallel** — Each segment via separate HTTP connection with Range headers
4. **Progress** — Real-time speed with exponential moving average
5. **Assemble** — Segments merged into final file in order
6. **Persist** — Segment state saved for crash recovery

## 📦 Installation

### Pre-built Releases

Download from the [Releases page](https://github.com/PBhadoo/QDM/releases):

| Platform | File | Description |
|----------|------|-------------|
| 🪟 Windows | `*_x64_en-US.msi` | MSI installer |
| 🪟 Windows | `*_x64-setup.exe` | NSIS installer |
| 🍎 macOS | `*.dmg` | Disk image (Apple Silicon) |
| 🐧 Linux | `*.AppImage` | Universal (x64) |
| 🐧 Linux | `*.deb` | Debian/Ubuntu (x64) |

> **macOS:** If the app is blocked, run `xattr -cr /Applications/Quantum\ Download\ Manager.app`

### yt-dlp & ffmpeg

QDM auto-installs yt-dlp and ffmpeg on first launch if they are not found. You can also install manually via **Settings → Tools**.

## 🌐 Browser Extension

The Chrome extension integrates QDM with your browser — automatically intercepting downloads and media streams just like IDM.

### Install

1. Open `chrome://extensions`
2. Enable **Developer mode**
3. Click **Load unpacked** and select the `extension/chrome/` folder

### Features

- **Auto-intercept** — `.exe`, `.zip`, `.pdf`, `.mp4`, `.mkv` etc. go directly to QDM
- **Toggle** — Enable/disable auto-intercept from the popup
- **YouTube** — Click the QDM banner on YouTube to download with quality selection
- **Context menu** — Right-click any link → *Download with QDM ⚡*
- **Manual URL** — Paste any URL in the popup to queue it
- **Notifications** — Desktop notification when a download completes

## 🛠️ Development

### Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) (stable)
- [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your platform

### Setup

```bash
git clone https://github.com/PBhadoo/QDM.git
cd QDM
npm install
```

### Run

```bash
# Dev mode (hot-reload UI + Tauri backend)
npm run tauri:dev

# Build production app
npm run tauri:build
```

### Build Scripts

| Command | Description |
|---------|-------------|
| `npm run dev` | Vite dev server (UI only) |
| `npm run tauri:dev` | Full Tauri dev mode with hot-reload |
| `npm run tauri:build` | Production build (current platform) |
| `npm run tauri:build:win` | Windows MSI + NSIS installer |
| `npm run tauri:build:linux` | Linux AppImage + .deb |
| `npm run tauri:build:mac` | macOS .dmg (Apple Silicon) |
| `npm run fetch-yt-dlp` | Download yt-dlp binary for bundling |

### Project Structure

```
QDM/
├── src/                     # React frontend
│   ├── App.tsx              # Root component + event listeners
│   ├── components/          # UI components
│   │   ├── TitleBar.tsx
│   │   ├── Sidebar.tsx
│   │   ├── Toolbar.tsx
│   │   ├── DownloadList.tsx
│   │   ├── VideoQualityDialog.tsx
│   │   ├── NewDownloadDialog.tsx
│   │   ├── SettingsDialog.tsx
│   │   └── AboutDialog.tsx
│   └── store/
│       └── useDownloadStore.ts
├── src-tauri/               # Rust/Tauri backend
│   ├── src/
│   │   ├── lib.rs           # Tauri commands + HTTP server (Axum)
│   │   ├── download_engine.rs  # Multi-segment download engine
│   │   ├── yt_dlp.rs        # yt-dlp integration
│   │   ├── hls_engine.rs    # HLS/DASH streaming engine
│   │   ├── browser_monitor.rs  # Browser extension HTTP API
│   │   ├── tools.rs         # yt-dlp/ffmpeg auto-install
│   │   └── types.rs         # Shared types
│   ├── tauri.conf.json
│   └── Cargo.toml
├── extension/
│   └── chrome/              # Chrome MV3 extension
│       ├── background.js    # Service worker (download interception)
│       ├── content.js       # Content script (hover banner)
│       ├── inject.js        # Page-world media detector
│       ├── popup.html       # Extension popup UI
│       ├── popup.js         # Popup logic
│       └── manifest.json
└── scripts/
    └── fetch-yt-dlp.js      # Downloads yt-dlp binary for bundling
```

### Release

To create a release, push a version tag:

```bash
git tag v1.0.3
git push origin v1.0.3
```

GitHub Actions will build for all platforms and create a release automatically.

## 🙏 Credits

### XDM (Xtreme Download Manager)
**By [subhra74](https://github.com/subhra74)** — [github.com/subhra74/xdm](https://github.com/subhra74/xdm)

QDM's multi-segment download architecture is inspired by XDM. XDM pioneered open-source download acceleration with segment splitting, Range-header resumption, and crash recovery — patterns QDM builds on with a modern Rust + Tauri stack.

### IDM (Internet Download Manager)
**By [Tonec Inc.](https://www.internetdownloadmanager.com/)**

IDM established the gold standard for download acceleration and browser integration. QDM's browser extension design follows the patterns IDM pioneered.

### Open Source

- [Tauri](https://tauri.app/) — Rust desktop framework
- [React](https://react.dev/) — UI library
- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — Video downloader
- [Axum](https://github.com/tokio-rs/axum) — Async web framework
- [Tailwind CSS](https://tailwindcss.com/) — Styling
- [Zustand](https://github.com/pmndrs/zustand) — State management
- [Lucide Icons](https://lucide.dev/) — Icons

## 📄 License

MIT License — see [LICENSE](LICENSE) for details.

---

<p align="center">
  Made with ❤️ by <a href="https://github.com/PBhadoo">Parveen Bhadoo</a>
</p>
