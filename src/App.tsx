/**
 * QDM - Quantum Download Manager
 * Main Application Component
 */

import React, { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { TitleBar } from './components/TitleBar'
import { Sidebar } from './components/Sidebar'
import { Toolbar } from './components/Toolbar'
import { DownloadList } from './components/DownloadList'
import { NewDownloadDialog } from './components/NewDownloadDialog'
import { SettingsDialog } from './components/SettingsDialog'
import { AboutDialog } from './components/AboutDialog'
import { AuthDialog } from './components/AuthDialog'
import { LinkExpiredDialog } from './components/LinkExpiredDialog'
import { YtdlpLogPanel } from './components/YtdlpLogPanel'
import { VideoQualityDialog } from './components/VideoQualityDialog'
import { useDownloadStore } from './store/useDownloadStore'
import type { YtdlpLog, QualityRequest } from './store/useDownloadStore'

export default function App() {
  const {
    loadDownloads, updateProgress, addDownload, updateDownload,
    setShowNewDownload, setShowSettings, setShowAbout,
    showNewDownload, showSettings, showAbout,
    authChallenge, setAuthChallenge,
    linkExpiredChallenge, setLinkExpiredChallenge,
    showYtdlpLogs, setShowYtdlpLogs, addYtdlpLog,
    qualityRequest, setQualityRequest,
  } = useDownloadStore()

  const [updateBanner, setUpdateBanner] = useState<{ version: string; url: string } | null>(null)
  const [toolsProgress, setToolsProgress] = useState<{ tool: string; step: string; pct: number; msg: string } | null>(null)

  useEffect(() => {
    loadDownloads()

    // Check for QDM app updates silently on startup
    invoke<any>('update_check').then((result) => {
      if (result?.updateAvailable) {
        setUpdateBanner({ version: result.latestVersion, url: result.releaseUrl })
      }
    }).catch(() => {})

    const unlistenPromises = [
      listen('download:progress', (e) => updateProgress(e.payload as any)),
      listen('download:added', (e) => {
        const item = e.payload as any
        addDownload(item)
        addYtdlpLog({ ts: new Date().toISOString(), downloadId: item.id, level: 'info', msg: `Added: ${item.fileName} — ${item.url}` })
      }),
      listen('download:completed', (e) => {
        const item = e.payload as any
        updateDownload(item.id, item)
        addYtdlpLog({ ts: new Date().toISOString(), downloadId: item.id, level: 'info', msg: `Completed: ${item.fileName}` })
      }),
      listen('download:failed', (e) => {
        const p = e.payload as any
        addYtdlpLog({ ts: new Date().toISOString(), downloadId: p.id, level: 'error', msg: `Failed: ${p.fileName || p.id} — ${p.error || ''}` })
      }),
      listen('show-new-download', () => setShowNewDownload(true)),
      listen('show-about', () => setShowAbout(true)),
      listen('show-settings', () => setShowSettings(true)),
      listen('download:auth_required', (e) => {
        const p = e.payload as { id: string; scheme: string }
        const dl = useDownloadStore.getState().downloads.find(d => d.id === p.id)
        setAuthChallenge({ id: p.id, scheme: p.scheme, fileName: dl?.fileName })
      }),
      listen('download:link_expired', (e) => {
        const p = e.payload as { id: string; sourcePageUrl?: string }
        const dl = useDownloadStore.getState().downloads.find(d => d.id === p.id)
        setLinkExpiredChallenge({ id: p.id, fileName: dl?.fileName, sourcePageUrl: p.sourcePageUrl })
      }),
      listen('download:quality_required', (e) => {
        setQualityRequest(e.payload as QualityRequest)
      }),
      listen('yt-dlp:log', (e) => {
        const p = e.payload as { download_id: string; level: string; msg: string }
        addYtdlpLog({
          ts: new Date().toISOString(),
          downloadId: p.download_id,
          level: p.level as YtdlpLog['level'],
          msg: p.msg,
        })
      }),
      listen('tools:progress', (e) => {
        const p = e.payload as { tool: string; step: string; pct: number; msg: string }
        if (p.step === 'done' && p.pct === 100) {
          // Hide overlay briefly after completion
          setToolsProgress({ ...p })
          setTimeout(() => setToolsProgress(prev => prev?.tool === p.tool ? null : prev), 1500)
        } else {
          setToolsProgress(p)
        }
      }),
      listen('tools:setup_done', () => setToolsProgress(null)),
    ]

    return () => {
      unlistenPromises.forEach(p => p.then(unlisten => unlisten()))
    }
  }, [])

  return (
    <div className="h-screen w-screen flex flex-col bg-qdm-bg overflow-hidden">
      {/* Custom Title Bar */}
      <TitleBar />

      {/* QDM update banner */}
      {updateBanner && (
        <div className="flex items-center justify-between px-4 py-1.5 bg-qdm-accent/15 border-b border-qdm-accent/30 text-xs">
          <span className="text-qdm-text">
            QDM <span className="font-semibold text-qdm-accent">v{updateBanner.version}</span> is available
          </span>
          <div className="flex items-center gap-3">
            <button
              onClick={() => invoke('update_open_release', { version: updateBanner.version }).catch(() => {})}
              className="text-qdm-accent font-semibold hover:underline"
            >
              Download update
            </button>
            <button
              onClick={() => setUpdateBanner(null)}
              className="text-qdm-textMuted hover:text-qdm-text"
            >
              ✕
            </button>
          </div>
        </div>
      )}

      {/* Tools auto-install progress banner */}
      {toolsProgress && (
        <div className="flex items-center gap-3 px-4 py-2 bg-qdm-surface/80 border-b border-qdm-accent/30 text-xs">
          <div className="w-3 h-3 rounded-full border-2 border-qdm-accent border-t-transparent animate-spin shrink-0" />
          <div className="flex-1 min-w-0">
            <span className="text-qdm-text font-medium">
              {toolsProgress.step === 'done'
                ? `${toolsProgress.tool === 'ytdlp' ? 'yt-dlp' : 'ffmpeg'} ready`
                : `Setting up ${toolsProgress.tool === 'ytdlp' ? 'yt-dlp' : 'ffmpeg'}…`}
            </span>
            <span className="text-qdm-textMuted ml-2">{toolsProgress.msg}</span>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <div className="w-24 h-1.5 bg-qdm-border rounded-full overflow-hidden">
              <div
                className="h-full bg-qdm-accent rounded-full transition-all duration-300"
                style={{ width: `${toolsProgress.pct}%` }}
              />
            </div>
            <span className="text-qdm-textMuted font-mono w-8 text-right">{toolsProgress.pct}%</span>
          </div>
        </div>
      )}

      {/* Main Content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar */}
        <Sidebar />

        {/* Main Area */}
        <div className="flex-1 flex flex-col overflow-hidden">
          <>
            <Toolbar />
            <DownloadList />
          </>
        </div>
      </div>

      {/* Dialogs */}
      {showNewDownload && <NewDownloadDialog />}
      {showSettings && <SettingsDialog />}
      {showAbout && <AboutDialog />}
      {authChallenge && (
        <AuthDialog
          downloadId={authChallenge.id}
          fileName={authChallenge.fileName}
          scheme={authChallenge.scheme}
          onClose={() => setAuthChallenge(null)}
        />
      )}
      {linkExpiredChallenge && (
        <LinkExpiredDialog
          downloadId={linkExpiredChallenge.id}
          fileName={linkExpiredChallenge.fileName}
          sourcePageUrl={linkExpiredChallenge.sourcePageUrl}
          onClose={() => setLinkExpiredChallenge(null)}
        />
      )}
      {showYtdlpLogs && <YtdlpLogPanel onClose={() => setShowYtdlpLogs(false)} />}
      {qualityRequest && <VideoQualityDialog request={qualityRequest} onClose={() => setQualityRequest(null)} />}
    </div>
  )
}
