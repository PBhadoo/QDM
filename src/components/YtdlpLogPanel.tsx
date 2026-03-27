import React, { useEffect, useRef, useState } from 'react'
import { X, Trash2, Terminal } from 'lucide-react'
import { useDownloadStore } from '../store/useDownloadStore'
import type { YtdlpLog } from '../store/useDownloadStore'

interface Props {
  onClose: () => void
}

const LEVEL_STYLES: Record<YtdlpLog['level'], { badge: string; text: string }> = {
  cmd:    { badge: 'bg-blue-500/20 text-blue-400 border border-blue-500/30',   text: 'text-blue-300' },
  stdout: { badge: 'bg-green-500/20 text-green-400 border border-green-500/30', text: 'text-qdm-text' },
  stderr: { badge: 'bg-yellow-500/20 text-yellow-400 border border-yellow-500/30', text: 'text-yellow-200' },
  error:  { badge: 'bg-red-500/20 text-red-400 border border-red-500/30',       text: 'text-red-300' },
  info:   { badge: 'bg-qdm-accent/20 text-qdm-accent border border-qdm-accent/30', text: 'text-qdm-accent' },
}

export function YtdlpLogPanel({ onClose }: Props) {
  const { ytdlpLogs, clearYtdlpLogs } = useDownloadStore()
  const bottomRef = useRef<HTMLDivElement>(null)
  const [filter, setFilter] = useState<YtdlpLog['level'] | 'all'>('all')
  const [autoScroll, setAutoScroll] = useState(true)

  useEffect(() => {
    if (autoScroll) bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [ytdlpLogs, autoScroll])

  const shown = filter === 'all' ? ytdlpLogs : ytdlpLogs.filter(l => l.level === filter)

  // Group consecutive stdout progress lines — skip ones with % to reduce noise
  const displayLogs = shown.filter(l =>
    !(l.level === 'stdout' && l.msg.startsWith('[download]') && l.msg.includes('%'))
    || l.msg.includes('100%')
  )

  return (
    <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-stretch justify-end">
      <div className="w-full max-w-3xl bg-qdm-bg border-l border-qdm-border flex flex-col">
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-3 border-b border-qdm-border shrink-0">
          <Terminal size={15} className="text-qdm-accent" />
          <span className="text-sm font-semibold text-qdm-text">Logs</span>
          <span className="text-[10px] font-mono text-qdm-textMuted ml-1">{ytdlpLogs.length} entries</span>
          <div className="flex-1" />

          {/* Level filter */}
          <div className="flex items-center gap-1">
            {(['all', 'cmd', 'stdout', 'stderr', 'error', 'info'] as const).map(lvl => (
              <button
                key={lvl}
                onClick={() => setFilter(lvl)}
                className={`px-2 py-0.5 rounded text-[10px] font-mono transition-colors ${
                  filter === lvl
                    ? 'bg-qdm-accent/20 text-qdm-accent border border-qdm-accent/40'
                    : 'text-qdm-textMuted hover:text-qdm-text hover:bg-qdm-surface'
                }`}
              >
                {lvl}
              </button>
            ))}
          </div>

          <button
            onClick={() => setAutoScroll(v => !v)}
            className={`px-2 py-0.5 rounded text-[10px] font-mono transition-colors ml-2 ${
              autoScroll
                ? 'bg-qdm-success/20 text-qdm-success border border-qdm-success/30'
                : 'text-qdm-textMuted hover:text-qdm-text border border-qdm-border'
            }`}
            title="Toggle auto-scroll"
          >
            auto-scroll
          </button>

          <button
            onClick={clearYtdlpLogs}
            className="p-1.5 rounded-lg text-qdm-textMuted hover:text-qdm-danger hover:bg-qdm-danger/10 transition-colors"
            title="Clear logs"
          >
            <Trash2 size={13} />
          </button>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-qdm-textMuted hover:text-qdm-text hover:bg-qdm-surfaceHover transition-colors"
          >
            <X size={14} />
          </button>
        </div>

        {/* Log entries */}
        <div className="flex-1 overflow-y-auto font-mono text-[11px] leading-relaxed p-3 space-y-0.5">
          {displayLogs.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-full text-qdm-textMuted gap-2">
              <Terminal size={28} className="opacity-30" />
              <p>No logs yet. Activity will appear here.</p>
            </div>
          ) : (
            displayLogs.map((log, i) => {
              const style = LEVEL_STYLES[log.level]
              const time = log.ts.slice(11, 19) // HH:MM:SS
              return (
                <div key={i} className="flex gap-2 items-start group hover:bg-qdm-surface/40 rounded px-1 py-0.5">
                  <span className="text-qdm-textMuted shrink-0 select-none">{time}</span>
                  <span className={`shrink-0 px-1.5 py-0 rounded text-[9px] font-bold uppercase ${style.badge}`}>
                    {log.level}
                  </span>
                  <span className={`break-all ${style.text}`}>{log.msg}</span>
                </div>
              )
            })
          )}
          <div ref={bottomRef} />
        </div>
      </div>
    </div>
  )
}
