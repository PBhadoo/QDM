import React, { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import {
  Plus, Play, Pause, Trash2, Search, X,
  ChevronsRight, StopCircle, Terminal
} from 'lucide-react'
import { useDownloadStore } from '../store/useDownloadStore'

export function Toolbar() {
  const {
    selectedIds, downloads, searchQuery, setSearchQuery,
    setShowNewDownload, clearSelection, removeDownloadFromList,
    ytdlpLogs, setShowYtdlpLogs,
  } = useDownloadStore()

  const [showSearch, setShowSearch] = useState(false)

  const selectedDownloads = downloads.filter(d => selectedIds.has(d.id))
  const hasSelection = selectedIds.size > 0
  const hasActiveSelected = selectedDownloads.some(d => d.status === 'downloading')
  const hasPausedSelected = selectedDownloads.some(d => d.status === 'paused' || d.status === 'failed')

  const allActive = downloads.filter(d => d.status === 'downloading')
  const allPaused = downloads.filter(d => d.status === 'paused' || d.status === 'queued')
  const allCompleted = downloads.filter(d => d.status === 'completed')

  // Global pause/resume all
  const handlePauseAll = async () => {
    await invoke('download_pause_all')
  }

  const handleResumeAll = async () => {
    await invoke('download_resume_all')
  }

  // Selection-based actions
  const handleResumeSelected = async () => {
    for (const d of selectedDownloads) {
      if (d.status === 'paused' || d.status === 'failed') {
        await invoke('download_resume', { id: d.id })
      }
    }
  }

  const handlePauseSelected = async () => {
    for (const d of selectedDownloads) {
      if (d.status === 'downloading') {
        await invoke('download_pause', { id: d.id })
      }
    }
  }

  const handleDeleteSelected = async () => {
    for (const d of selectedDownloads) {
      await invoke('download_remove', { id: d.id, deleteFile: false })
      removeDownloadFromList(d.id)
    }
    clearSelection()
  }

  const handleClearCompleted = async () => {
    for (const d of allCompleted) {
      await invoke('download_remove', { id: d.id, deleteFile: false })
      removeDownloadFromList(d.id)
    }
  }

  const handleDeleteAll = async () => {
    for (const d of downloads) {
      await invoke('download_remove', { id: d.id, deleteFile: false })
      removeDownloadFromList(d.id)
    }
    clearSelection()
  }

  const handleClearSearch = () => {
    setSearchQuery('')
    setShowSearch(false)
  }

  return (
    <div className="h-12 bg-qdm-surface/30 border-b border-qdm-border flex items-center px-3 gap-1.5 shrink-0">
      {/* New Download */}
      <button
        onClick={() => setShowNewDownload(true)}
        className="btn-primary flex items-center gap-1.5 h-8 px-3 text-xs shadow-lg shadow-qdm-accent/20"
        title="New Download (Ctrl+N)"
      >
        <Plus size={14} />
        <span>New Download</span>
      </button>

      <div className="w-px h-5 bg-qdm-border mx-0.5" />

      {/* Global Pause All */}
      <button
        onClick={handlePauseAll}
        disabled={allActive.length === 0}
        className="flex items-center gap-1.5 px-2.5 h-8 text-xs text-qdm-textSecondary hover:text-qdm-warning hover:bg-qdm-warning/10 rounded-lg transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
        title={allActive.length > 0 ? `Pause All (${allActive.length} active)` : 'No active downloads'}
      >
        <Pause size={13} />
        <span className="hidden md:inline">Pause All</span>
        {allActive.length > 0 && (
          <span className="bg-qdm-warning/20 text-qdm-warning text-[9px] font-mono px-1 rounded">
            {allActive.length}
          </span>
        )}
      </button>

      {/* Global Resume All */}
      <button
        onClick={handleResumeAll}
        disabled={allPaused.length === 0}
        className="flex items-center gap-1.5 px-2.5 h-8 text-xs text-qdm-textSecondary hover:text-qdm-accent hover:bg-qdm-accent/10 rounded-lg transition-colors disabled:opacity-30 disabled:cursor-not-allowed"
        title={allPaused.length > 0 ? `Resume All (${allPaused.length} paused)` : 'Nothing paused'}
      >
        <Play size={13} />
        <span className="hidden md:inline">Resume All</span>
        {allPaused.length > 0 && (
          <span className="bg-qdm-accent/20 text-qdm-accent text-[9px] font-mono px-1 rounded">
            {allPaused.length}
          </span>
        )}
      </button>

      {/* Clear Completed */}
      {allCompleted.length > 0 && !hasSelection && (
        <button
          onClick={handleClearCompleted}
          className="flex items-center gap-1.5 px-2.5 h-8 text-xs text-qdm-textSecondary hover:text-qdm-success hover:bg-qdm-success/10 rounded-lg transition-colors"
          title={`Clear ${allCompleted.length} completed`}
        >
          <Trash2 size={13} />
          <span className="hidden md:inline">Clear Done</span>
          <span className="bg-qdm-success/20 text-qdm-success text-[9px] font-mono px-1 rounded">
            {allCompleted.length}
          </span>
        </button>
      )}

      {/* Delete All */}
      {downloads.length > 0 && !hasSelection && (
        <button
          onClick={handleDeleteAll}
          className="flex items-center gap-1.5 px-2.5 h-8 text-xs text-qdm-textSecondary hover:text-qdm-danger hover:bg-qdm-danger/10 rounded-lg transition-colors"
          title="Remove all downloads from list"
        >
          <X size={13} />
          <span className="hidden md:inline">Delete All</span>
        </button>
      )}

      {/* Selection-specific actions */}
      {hasSelection && (
        <>
          <div className="w-px h-5 bg-qdm-border mx-0.5" />

          {hasPausedSelected && (
            <button
              onClick={handleResumeSelected}
              className="flex items-center gap-1.5 px-2 h-8 text-xs text-qdm-accent hover:bg-qdm-accent/10 rounded-lg transition-colors"
              title="Resume Selected"
            >
              <ChevronsRight size={13} />
              <span className="hidden lg:inline">Resume</span>
            </button>
          )}

          {hasActiveSelected && (
            <button
              onClick={handlePauseSelected}
              className="flex items-center gap-1.5 px-2 h-8 text-xs text-qdm-warning hover:bg-qdm-warning/10 rounded-lg transition-colors"
              title="Pause Selected"
            >
              <StopCircle size={13} />
              <span className="hidden lg:inline">Pause</span>
            </button>
          )}

          <button
            onClick={handleDeleteSelected}
            className="flex items-center gap-1.5 px-2 h-8 text-xs text-qdm-danger/70 hover:text-qdm-danger hover:bg-qdm-danger/10 rounded-lg transition-colors"
            title="Remove Selected"
          >
            <Trash2 size={13} />
            <span className="hidden lg:inline">Remove</span>
          </button>

          <span className="text-[10px] text-qdm-textMuted font-mono ml-1">
            {selectedIds.size} selected
          </span>
        </>
      )}

      <div className="flex-1" />

      {/* Logs */}
      <button
        onClick={() => setShowYtdlpLogs(true)}
        className="flex items-center gap-1.5 px-2.5 h-8 text-xs text-qdm-textMuted hover:text-qdm-accent hover:bg-qdm-accent/10 rounded-lg transition-colors relative"
        title="Application logs"
      >
        <Terminal size={13} />
        <span className="hidden md:inline">Logs</span>
        {ytdlpLogs.length > 0 && (
          <span className="bg-qdm-accent/20 text-qdm-accent text-[9px] font-mono px-1 rounded">
            {ytdlpLogs.length}
          </span>
        )}
      </button>

      {/* Search */}
      {showSearch || searchQuery ? (
        <div className="relative animate-fade-in">
          <Search size={12} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-qdm-textMuted" />
          <input
            autoFocus
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search downloads..."
            className="input-qdm pl-7 pr-7 w-48 text-xs h-8"
            onKeyDown={(e) => e.key === 'Escape' && handleClearSearch()}
          />
          {searchQuery && (
            <button
              onClick={handleClearSearch}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-qdm-textMuted hover:text-qdm-text"
            >
              <X size={12} />
            </button>
          )}
        </div>
      ) : (
        <button
          onClick={() => setShowSearch(true)}
          className="p-2 rounded-lg text-qdm-textMuted hover:text-qdm-text hover:bg-qdm-surfaceHover transition-colors"
          title="Search (Ctrl+F)"
        >
          <Search size={14} />
        </button>
      )}
    </div>
  )
}
