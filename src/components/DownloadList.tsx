import React, { useState, useEffect, useRef, useCallback } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  Play, Pause, Trash2, FolderOpen, RotateCcw, FileDown,
  CheckCircle2, AlertCircle, Clock, Loader2, ExternalLink,
  Copy, Plus, Link2
} from 'lucide-react'
import { useDownloadStore } from '../store/useDownloadStore'
import { formatBytes, formatSpeed, formatEta, formatDate, getFileIcon } from '../utils/format'
import type { DownloadItem, DownloadSegment } from '../types/download'

// ── Status Badge ──────────────────────────────────────────────────────────────
function StatusBadge({ status }: { status: string }) {
  const configs: Record<string, { icon: React.ReactNode; label: string; className: string }> = {
    downloading: {
      icon: <Loader2 size={11} className="animate-spin" />,
      label: 'Downloading',
      className: 'bg-qdm-accent/15 text-qdm-accent border-qdm-accent/30'
    },
    completed: {
      icon: <CheckCircle2 size={11} />,
      label: 'Completed',
      className: 'bg-qdm-success/15 text-qdm-success border-qdm-success/30'
    },
    paused: {
      icon: <Pause size={11} />,
      label: 'Paused',
      className: 'bg-qdm-warning/15 text-qdm-warning border-qdm-warning/30'
    },
    failed: {
      icon: <AlertCircle size={11} />,
      label: 'Failed',
      className: 'bg-qdm-danger/15 text-qdm-danger border-qdm-danger/30'
    },
    queued: {
      icon: <Clock size={11} />,
      label: 'Queued',
      className: 'bg-qdm-textMuted/15 text-qdm-textSecondary border-qdm-textMuted/30'
    },
    assembling: {
      icon: <Loader2 size={11} className="animate-spin" />,
      label: 'Assembling',
      className: 'bg-qdm-accent/15 text-qdm-accent border-qdm-accent/30'
    },
    stopped: {
      icon: <AlertCircle size={11} />,
      label: 'Stopped',
      className: 'bg-qdm-textMuted/15 text-qdm-textSecondary border-qdm-textMuted/30'
    },
  }

  const config = configs[status] || configs.stopped

  return (
    <span className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[10px] font-medium border ${config.className}`}>
      {config.icon}
      {config.label}
    </span>
  )
}

// ── Segment Visualization Bar (XDM/IDM style) ────────────────────────────────
function SegmentBar({ segments, fileSize }: { segments: DownloadSegment[]; fileSize: number }) {
  if (!segments.length || fileSize <= 0) return null

  return (
    <div className="segment-bar mt-1">
      {segments.map((seg) => {
        const left = (seg.offset / fileSize) * 100
        const width = (seg.length / fileSize) * 100
        const progress = seg.length > 0 ? Math.min(1, seg.downloaded / seg.length) : 0

        const colors: Record<number, string> = {
          0: '',
          1: 'bg-qdm-accent',
          2: 'bg-qdm-success',
          3: 'bg-qdm-danger',
        }

        if (seg.state === 0) return null

        return (
          <div
            key={seg.id}
            className={`segment-fill ${colors[seg.state] || ''}`}
            style={{
              left: `${left}%`,
              width: `${width * progress}%`,
              opacity: seg.state === 2 ? 0.7 : 1,
            }}
          />
        )
      })}
    </div>
  )
}

// ── Progress Bar ──────────────────────────────────────────────────────────────
function ProgressBar({ progress, status }: { progress: number; status: string }) {
  const colorMap: Record<string, string> = {
    downloading: 'bg-qdm-accent',
    completed: 'bg-qdm-success',
    paused: 'bg-qdm-warning',
    failed: 'bg-qdm-danger',
    assembling: 'bg-purple-400',
  }
  const color = colorMap[status] || 'bg-qdm-textMuted'
  const pct = Math.min(100, Math.max(0, progress))

  return (
    <div className="relative w-full h-1.5 bg-white/5 rounded-full overflow-hidden">
      <div
        className={`h-full rounded-full transition-all duration-300 ${color} ${status === 'downloading' ? 'progress-bar-animated' : ''}`}
        style={{ width: `${pct}%` }}
      />
    </div>
  )
}

// ── Context Menu ──────────────────────────────────────────────────────────────
interface ContextMenuProps {
  x: number
  y: number
  item: DownloadItem
  onClose: () => void
}

function ContextMenu({ x, y, item, onClose }: ContextMenuProps) {
  const menuRef = useRef<HTMLDivElement>(null)
  const isCompleted = item.status === 'completed'
  const isDownloading = item.status === 'downloading'
  const isPaused = item.status === 'paused' || item.status === 'stopped'
  const isFailed = item.status === 'failed'
  const { removeDownloadFromList } = useDownloadStore()

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose()
      }
    }
    const keyHandler = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose() }
    document.addEventListener('mousedown', handler)
    document.addEventListener('keydown', keyHandler)
    return () => {
      document.removeEventListener('mousedown', handler)
      document.removeEventListener('keydown', keyHandler)
    }
  }, [onClose])

  // Adjust position to stay within viewport
  const adjustedX = Math.min(x, window.innerWidth - 200)
  const adjustedY = Math.min(y, window.innerHeight - 320)

  const handleAction = async (action: string) => {
    onClose()

    switch (action) {
      case 'open':
        await invoke('download_open_file', { id: item.id })
        break
      case 'folder':
        await invoke('download_open_folder', { id: item.id })
        break
      case 'pause':
        await invoke('download_pause', { id: item.id })
        break
      case 'resume':
        await invoke('download_resume', { id: item.id })
        break
      case 'retry':
        await invoke('download_retry', { id: item.id })
        break
      case 'copy-url':
        await navigator.clipboard.writeText(item.url)
        break
      case 'delete':
        await invoke('download_remove', { id: item.id, deleteFile: false })
        removeDownloadFromList(item.id)
        break
      case 'delete-file':
        await invoke('download_remove', { id: item.id, deleteFile: true })
        removeDownloadFromList(item.id)
        break
    }
  }

  return (
    <div
      ref={menuRef}
      className="fixed z-[100] bg-qdm-surface border border-qdm-border rounded-lg shadow-2xl py-1 min-w-[180px] text-sm"
      style={{ left: adjustedX, top: adjustedY }}
    >
      {isCompleted && (
        <>
          <MenuItem icon={<ExternalLink size={13} />} label="Open File" onClick={() => handleAction('open')} />
          <MenuItem icon={<FolderOpen size={13} />} label="Open Folder" onClick={() => handleAction('folder')} />
          <Divider />
        </>
      )}
      {!isCompleted && (
        <MenuItem icon={<FolderOpen size={13} />} label="Open Folder" onClick={() => handleAction('folder')} />
      )}
      {isDownloading && (
        <MenuItem icon={<Pause size={13} />} label="Pause" onClick={() => handleAction('pause')} />
      )}
      {isPaused && (
        <MenuItem icon={<Play size={13} />} label="Resume" onClick={() => handleAction('resume')} accent />
      )}
      {isFailed && (
        <MenuItem icon={<RotateCcw size={13} />} label="Retry Download" onClick={() => handleAction('retry')} accent />
      )}
      <Divider />
      <MenuItem icon={<Copy size={13} />} label="Copy URL" onClick={() => handleAction('copy-url')} />
      <Divider />
      <MenuItem icon={<Trash2 size={13} />} label="Remove from List" onClick={() => handleAction('delete')} danger />
      {isCompleted && (
        <MenuItem icon={<Trash2 size={13} />} label="Delete File" onClick={() => handleAction('delete-file')} danger />
      )}
    </div>
  )
}

function MenuItem({ icon, label, onClick, accent, danger }: {
  icon: React.ReactNode
  label: string
  onClick: () => void
  accent?: boolean
  danger?: boolean
}) {
  const color = danger
    ? 'text-qdm-danger hover:bg-qdm-danger/10'
    : accent
    ? 'text-qdm-accent hover:bg-qdm-accent/10'
    : 'text-qdm-text hover:bg-qdm-surfaceHover'

  return (
    <button
      onClick={onClick}
      className={`w-full flex items-center gap-2.5 px-3 py-1.5 text-left text-xs transition-colors ${color}`}
    >
      <span className="opacity-70">{icon}</span>
      {label}
    </button>
  )
}

function Divider() {
  return <div className="my-1 border-t border-qdm-border/50" />
}

// ── Download Row ──────────────────────────────────────────────────────────────
function DownloadRow({ item }: { item: DownloadItem }) {
  const { selectedIds, toggleSelect, removeDownloadFromList } = useDownloadStore()
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null)
  const isSelected = selectedIds.has(item.id)
  const isActive = item.status === 'downloading'
  const isCompleted = item.status === 'completed'
  const isPaused = item.status === 'paused'
  const isFailed = item.status === 'failed'

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault()
    setContextMenu({ x: e.clientX, y: e.clientY })
  }

  const handleAction = async (e: React.MouseEvent, action: string) => {
    e.stopPropagation()

    switch (action) {
      case 'pause':
        await invoke('download_pause', { id: item.id })
        break
      case 'resume':
        await invoke('download_resume', { id: item.id })
        break
      case 'retry':
        await invoke('download_retry', { id: item.id })
        break
      case 'openFile':
        await invoke('download_open_file', { id: item.id })
        break
      case 'openFolder':
        await invoke('download_open_folder', { id: item.id })
        break
      case 'delete':
        await invoke('download_remove', { id: item.id, deleteFile: false })
        removeDownloadFromList(item.id)
        break
    }
  }

  return (
    <>
      <div
        onClick={() => toggleSelect(item.id)}
        onContextMenu={handleContextMenu}
        className={`group px-4 py-3 border-b border-qdm-border/40 cursor-pointer transition-all duration-100 select-none
          ${isSelected
            ? 'bg-qdm-accent/8 border-l-2 border-l-qdm-accent'
            : 'hover:bg-white/[0.02] border-l-2 border-l-transparent'
          }
          ${isActive ? 'bg-qdm-accent/[0.03]' : ''}
        `}
      >
        <div className="flex items-center gap-3">
          {/* File icon */}
          <div className="text-xl select-none shrink-0 w-8 text-center">
            {getFileIcon(item.fileName)}
          </div>

          {/* Main content */}
          <div className="flex-1 min-w-0">
            {/* Row 1: filename + status badge */}
            <div className="flex items-center gap-2 mb-1">
              <span className="text-sm font-medium text-qdm-text truncate" title={item.fileName}>
                {item.fileName}
              </span>
              <StatusBadge status={item.status} />
            </div>

            {/* Row 2: progress */}
            {!isCompleted && (
              <ProgressBar progress={item.progress} status={item.status} />
            )}

            {/* Segment visualization */}
            {isActive && item.segments.length > 1 && (
              <SegmentBar segments={item.segments} fileSize={item.fileSize} />
            )}

            {/* Row 3: stats */}
            <div className="flex items-center gap-3 mt-1 text-[11px] text-qdm-textMuted flex-wrap">
              {/* Size */}
              <span className="font-mono">
                {item.fileSize > 0
                  ? `${formatBytes(item.downloaded)} / ${formatBytes(item.fileSize)}`
                  : formatBytes(item.downloaded)
                }
              </span>

              {/* Speed */}
              {isActive && item.speed > 0 && (
                <>
                  <span className="text-qdm-border">•</span>
                  <span className="text-qdm-accent font-mono font-semibold">
                    {formatSpeed(item.speed)}
                  </span>
                </>
              )}

              {/* ETA */}
              {isActive && item.eta > 0 && (
                <>
                  <span className="text-qdm-border">•</span>
                  <span>{formatEta(item.eta)}</span>
                </>
              )}

              {/* Progress % */}
              {!isCompleted && item.progress > 0 && (
                <>
                  <span className="text-qdm-border">•</span>
                  <span className="font-mono text-qdm-textSecondary">{item.progress}%</span>
                </>
              )}

              {/* Segment count */}
              {isActive && item.segments.length > 1 && (
                <>
                  <span className="text-qdm-border">•</span>
                  <span className="text-qdm-textMuted">
                    {item.segments.filter(s => s.state === 1).length}/{item.segments.length} threads
                  </span>
                </>
              )}

              {/* Date */}
              <span className="text-qdm-border">•</span>
              <span>
                {formatDate(isCompleted ? (item.dateCompleted || item.dateAdded) : item.dateAdded)}
              </span>

              {/* Error */}
              {isFailed && item.error && (
                <>
                  <span className="text-qdm-border">•</span>
                  <span className="text-qdm-danger truncate max-w-xs">{item.error}</span>
                </>
              )}
            </div>
          </div>

          {/* Action buttons (hover) */}
          <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
            {isActive && (
              <ActionBtn
                onClick={(e) => handleAction(e, 'pause')}
                title="Pause"
                className="hover:bg-qdm-warning/20 text-qdm-warning"
              >
                <Pause size={13} />
              </ActionBtn>
            )}
            {isPaused && (
              <ActionBtn
                onClick={(e) => handleAction(e, 'resume')}
                title="Resume"
                className="hover:bg-qdm-accent/20 text-qdm-accent"
              >
                <Play size={13} />
              </ActionBtn>
            )}
            {isFailed && (
              <ActionBtn
                onClick={(e) => handleAction(e, 'retry')}
                title="Retry"
                className="hover:bg-qdm-accent/20 text-qdm-accent"
              >
                <RotateCcw size={13} />
              </ActionBtn>
            )}
            {isCompleted && (
              <ActionBtn
                onClick={(e) => handleAction(e, 'openFile')}
                title="Open File"
                className="hover:bg-qdm-accent/20 text-qdm-accent"
              >
                <ExternalLink size={13} />
              </ActionBtn>
            )}
            <ActionBtn
              onClick={(e) => handleAction(e, 'openFolder')}
              title="Open Folder"
              className="hover:bg-white/10 text-qdm-textSecondary"
            >
              <FolderOpen size={13} />
            </ActionBtn>
            <ActionBtn
              onClick={(e) => handleAction(e, 'delete')}
              title="Remove"
              className="hover:bg-qdm-danger/20 text-qdm-danger/70 hover:text-qdm-danger"
            >
              <Trash2 size={13} />
            </ActionBtn>
          </div>
        </div>
      </div>

      {contextMenu && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          item={item}
          onClose={() => setContextMenu(null)}
        />
      )}
    </>
  )
}

function ActionBtn({ children, onClick, title, className }: {
  children: React.ReactNode
  onClick: (e: React.MouseEvent) => void
  title: string
  className: string
}) {
  return (
    <button
      onClick={onClick}
      title={title}
      className={`p-1.5 rounded-md transition-colors ${className}`}
    >
      {children}
    </button>
  )
}

// ── Empty State ───────────────────────────────────────────────────────────────
function EmptyState() {
  const { setShowNewDownload, activeCategory } = useDownloadStore()

  return (
    <div className="flex-1 flex flex-col items-center justify-center text-center p-8 animate-fade-in">
      <div className="w-20 h-20 rounded-2xl bg-qdm-surface flex items-center justify-center mb-4 border border-qdm-border">
        <FileDown size={32} className="text-qdm-textMuted" />
      </div>
      <h3 className="text-base font-semibold text-qdm-text mb-1">
        {activeCategory === 'all' ? 'No downloads yet' : `No ${activeCategory} downloads`}
      </h3>
      <p className="text-xs text-qdm-textMuted mb-5 max-w-xs leading-relaxed">
        Add a URL to start downloading. QDM uses up to 32 parallel connections for maximum speed.
      </p>
      <button
        onClick={() => setShowNewDownload(true)}
        className="btn-primary flex items-center gap-2"
      >
        <Plus size={15} />
        New Download
      </button>
    </div>
  )
}

// ── Main List ─────────────────────────────────────────────────────────────────
export function DownloadList() {
  const { filteredDownloads } = useDownloadStore()
  const downloads = filteredDownloads()

  if (downloads.length === 0) {
    return <EmptyState />
  }

  return (
    <div className="flex-1 overflow-y-auto">
      {downloads.map((item) => (
        <DownloadRow key={item.id} item={item} />
      ))}
    </div>
  )
}
