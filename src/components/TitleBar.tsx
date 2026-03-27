import React, { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { getVersion } from '@tauri-apps/api/app'
import { Minus, Square, X, Zap } from 'lucide-react'

const isMac = navigator.platform?.toLowerCase().includes('mac') ||
              navigator.userAgent?.toLowerCase().includes('mac')

export function TitleBar() {
  const [version, setVersion] = useState('')
  useEffect(() => { getVersion().then(setVersion).catch(() => {}) }, [])
  return (
    <div className="titlebar-drag h-10 bg-qdm-surface/80 border-b border-qdm-border flex items-center shrink-0 relative">
      {/* macOS: leave space for native traffic lights on the left */}
      {isMac && <div className="w-[78px] shrink-0" />}

      {/* Windows: brand on the left */}
      {!isMac && (
        <div className="flex items-center gap-2 px-4">
          <div className="w-5 h-5 rounded bg-gradient-to-br from-qdm-accent to-purple-400 flex items-center justify-center">
            <Zap size={12} className="text-white" />
          </div>
          <span className="text-xs font-semibold text-qdm-textSecondary tracking-wide uppercase">
            QDM
          </span>
          {version && <span className="text-[10px] text-qdm-textMuted font-mono">v{version}</span>}
        </div>
      )}

      {/* macOS: centered title */}
      {isMac && (
        <div className="flex-1 flex items-center justify-center gap-2">
          <div className="w-4 h-4 rounded bg-gradient-to-br from-qdm-accent to-purple-400 flex items-center justify-center">
            <Zap size={9} className="text-white" />
          </div>
          <span className="text-xs font-semibold text-qdm-textSecondary tracking-wide">
            Quantum Download Manager
          </span>
        </div>
      )}

      {/* Spacer for Windows */}
      {!isMac && <div className="flex-1" />}

      {/* Window Controls — only show on Windows/Linux */}
      {!isMac && (
        <div className="titlebar-no-drag flex items-center gap-0.5 px-2">
          <button
            onClick={() => invoke('window_minimize')}
            className="w-8 h-7 flex items-center justify-center rounded hover:bg-white/10 transition-colors"
            title="Minimize"
          >
            <Minus size={14} className="text-qdm-textSecondary" />
          </button>
          <button
            onClick={() => invoke('window_maximize')}
            className="w-8 h-7 flex items-center justify-center rounded hover:bg-white/10 transition-colors"
            title="Maximize"
          >
            <Square size={11} className="text-qdm-textSecondary" />
          </button>
          <button
            onClick={() => invoke('window_close')}
            className="w-8 h-7 flex items-center justify-center rounded hover:bg-red-500/80 hover:text-white transition-colors"
            title="Close"
          >
            <X size={14} className="text-qdm-textSecondary hover:text-white" />
          </button>
        </div>
      )}

      {/* macOS: small spacer on right for symmetry */}
      {isMac && <div className="w-[12px] shrink-0" />}
    </div>
  )
}
