import React, { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { X, Zap, Github, Heart, ExternalLink } from 'lucide-react'
import { useDownloadStore } from '../store/useDownloadStore'

export function AboutDialog() {
  const { setShowAbout } = useDownloadStore()
  const [version, setVersion] = useState('...')
  useEffect(() => { getVersion().then(setVersion).catch(() => setVersion('1.2.0')) }, [])

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
