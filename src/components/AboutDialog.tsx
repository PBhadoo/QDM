import React, { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getVersion } from '@tauri-apps/api/app'
import { X, Zap, Github, Heart, ExternalLink, RefreshCw, Download, CheckCircle } from 'lucide-react'
import { useDownloadStore } from '../store/useDownloadStore'

type UpdateState = 'idle' | 'checking' | 'upToDate' | 'available' | 'downloading' | 'installing' | 'done'

export function AboutDialog() {
  const { setShowAbout } = useDownloadStore()
  const [version, setVersion] = useState('...')
  const [updateState, setUpdateState] = useState<UpdateState>('idle')
  const [updateInfo, setUpdateInfo] = useState<{ version: string; notes: string } | null>(null)
  const [downloadPct, setDownloadPct] = useState(0)
  const [downloadMsg, setDownloadMsg] = useState('')
  const unlistenRef = useRef<(() => void) | null>(null)

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion('1.0.3'))
    return () => { unlistenRef.current?.() }
  }, [])

  const checkForUpdates = async () => {
    setUpdateState('checking')
    try {
      const result = await invoke<any>('update_check')
      if (result?.updateAvailable) {
        setUpdateInfo({ version: result.latestVersion, notes: result.releaseNotes || '' })
        setUpdateState('available')
      } else {
        setUpdateState('upToDate')
      }
    } catch {
      setUpdateState('idle')
    }
  }

  const installUpdate = async () => {
    if (!updateInfo) return
    setUpdateState('downloading')
    setDownloadPct(0)
    setDownloadMsg('Starting download…')

    unlistenRef.current?.()
    unlistenRef.current = await listen<any>('update:progress', (e) => {
      setDownloadPct(e.payload.pct)
      setDownloadMsg(e.payload.msg)
      if (e.payload.done) setUpdateState('done')
    })
    await listen('update:installing', () => setUpdateState('installing'))

    try {
      await invoke('update_download_install', { version: updateInfo.version })
    } catch (err: any) {
      setDownloadMsg(err?.toString() || 'Download failed')
      setUpdateState('available')
    }
  }

  const openLink = (url: string) => {
    invoke('shell_open_external', { url })
  }

  return (
    <div
      className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 animate-fade-in"
      onClick={() => setShowAbout(false)}
    >
      <div
        className="bg-qdm-surface border border-qdm-border rounded-xl w-[460px] shadow-2xl animate-slide-up"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-qdm-border">
          <div className="flex items-center gap-2">
            <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-qdm-accent to-purple-400 flex items-center justify-center">
              <Zap size={14} className="text-white" />
            </div>
            <h2 className="text-sm font-semibold text-qdm-text">About QDM</h2>
          </div>
          <button
            onClick={() => setShowAbout(false)}
            className="p-1 rounded-lg hover:bg-white/10 transition-colors"
          >
            <X size={16} className="text-qdm-textSecondary" />
          </button>
        </div>

        <div className="p-6 text-center">
          {/* Logo */}
          <div className="w-16 h-16 rounded-2xl bg-gradient-to-br from-qdm-accent to-purple-400 flex items-center justify-center mx-auto mb-4 shadow-lg shadow-qdm-accent/30">
            <Zap size={28} className="text-white" />
          </div>

          <h1 className="text-xl font-bold text-qdm-text mb-1">
            Quantum Download Manager
          </h1>
          <p className="text-xs font-mono text-qdm-textMuted mb-4">v{version}</p>

          <p className="text-xs text-qdm-textMuted mb-1">Made by <span className="text-qdm-accent font-semibold">Parveen Bhadoo</span></p>
          <p className="text-sm text-qdm-textSecondary leading-relaxed mb-6 max-w-sm mx-auto">
            A modern, open-source download manager for Windows with multi-segment
            downloading, pause/resume support, and a beautiful dark interface.
          </p>

          {/* Credits */}
          <div className="bg-qdm-bg/50 rounded-xl p-4 mb-5 text-left">
            <div className="flex items-center gap-2 mb-3">
              <Heart size={14} className="text-red-400" />
              <span className="text-xs font-semibold text-qdm-text uppercase tracking-wider">Credits & Acknowledgments</span>
            </div>

            <div className="space-y-3">
              <div className="flex items-start gap-3">
                <div className="w-8 h-8 rounded-lg bg-blue-500/20 flex items-center justify-center shrink-0 mt-0.5">
                  <span className="text-sm">🌟</span>
                </div>
                <div>
                  <button
                    onClick={() => openLink('https://github.com/subhra74/xdm')}
                    className="text-sm font-medium text-qdm-text hover:text-qdm-accent transition-colors flex items-center gap-1"
                  >
                    XDM (Xtreme Download Manager)
                    <ExternalLink size={10} />
                  </button>
                  <p className="text-[11px] text-qdm-textMuted leading-relaxed">
                    By <span className="text-qdm-textSecondary">subhra74</span> — Our download engine architecture
                    is inspired by XDM's brilliant multi-segment approach. QDM is a spiritual
                    successor to this amazing open-source project.
                  </p>
                </div>
              </div>

              <div className="flex items-start gap-3">
                <div className="w-8 h-8 rounded-lg bg-green-500/20 flex items-center justify-center shrink-0 mt-0.5">
                  <span className="text-sm">🏆</span>
                </div>
                <div>
                  <div className="text-sm font-medium text-qdm-text">
                    IDM (Internet Download Manager)
                  </div>
                  <p className="text-[11px] text-qdm-textMuted leading-relaxed">
                    By <span className="text-qdm-textSecondary">Tonec Inc.</span> — Pioneers of segmented download
                    technology. Their innovation in download acceleration has been the
                    gold standard for decades. We honor their hard work.
                  </p>
                </div>
              </div>
            </div>
          </div>

          {/* Update checker */}
          <div className="bg-qdm-bg/50 rounded-xl px-4 py-3 mb-5">
            {updateState === 'idle' && (
              <button
                onClick={checkForUpdates}
                className="flex items-center gap-2 text-xs text-qdm-textSecondary hover:text-qdm-text transition-colors mx-auto"
              >
                <RefreshCw size={13} />
                Check for Updates
              </button>
            )}
            {updateState === 'checking' && (
              <div className="flex items-center gap-2 justify-center text-xs text-qdm-textSecondary">
                <RefreshCw size={13} className="animate-spin" />
                Checking for updates…
              </div>
            )}
            {updateState === 'upToDate' && (
              <div className="flex items-center gap-2 justify-center text-xs text-green-400">
                <CheckCircle size={13} />
                You're up to date (v{version})
              </div>
            )}
            {updateState === 'available' && updateInfo && (
              <div className="text-center">
                <p className="text-xs text-qdm-accent font-semibold mb-2">
                  v{updateInfo.version} is available
                </p>
                {downloadMsg && (
                  <p className="text-[11px] text-red-400 mb-2">{downloadMsg}</p>
                )}
                <button
                  onClick={installUpdate}
                  className="btn-primary flex items-center gap-2 mx-auto text-xs"
                >
                  <Download size={13} />
                  Install v{updateInfo.version}
                </button>
              </div>
            )}
            {updateState === 'downloading' && (
              <div>
                <div className="flex items-center justify-between text-xs text-qdm-textSecondary mb-1.5">
                  <span>Downloading update…</span>
                  <span className="font-mono">{downloadPct}%</span>
                </div>
                <div className="w-full h-1.5 bg-qdm-border rounded-full overflow-hidden">
                  <div
                    className="h-full bg-qdm-accent rounded-full transition-all duration-200"
                    style={{ width: `${downloadPct}%` }}
                  />
                </div>
                <p className="text-[11px] text-qdm-textMuted mt-1">{downloadMsg}</p>
              </div>
            )}
            {updateState === 'installing' && (
              <div className="flex items-center gap-2 justify-center text-xs text-qdm-accent">
                <RefreshCw size={13} className="animate-spin" />
                Installing… app will restart
              </div>
            )}
            {updateState === 'done' && (
              <div className="text-center">
                <p className="text-xs text-green-400 font-semibold mb-0.5 flex items-center gap-1 justify-center">
                  <CheckCircle size={13} /> Download complete
                </p>
                <p className="text-[11px] text-qdm-textMuted">{downloadMsg}</p>
              </div>
            )}
          </div>

          {/* Links */}
          <div className="flex items-center justify-center gap-3">
            <button
              onClick={() => openLink('https://github.com/PBhadoo/QDM')}
              className="btn-secondary flex items-center gap-2"
            >
              <Github size={14} />
              GitHub
            </button>
            <button
              onClick={() => openLink('https://github.com/PBhadoo/QDM/issues')}
              className="btn-ghost"
            >
              Report Issue
            </button>
          </div>

          {/* License */}
          <p className="text-[10px] text-qdm-textMuted mt-4">
            &copy; 2026 Parveen Bhadoo — Open Source — MIT License — Made with <Heart size={8} className="inline text-red-400" />
          </p>
        </div>
      </div>
    </div>
  )
}
