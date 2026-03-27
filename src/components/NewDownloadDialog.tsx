import React, { useState, useRef, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { X, Link, FolderOpen, Zap, ChevronDown, ChevronUp, Loader2, FileDown, HardDrive, Shield, Music, Video } from 'lucide-react'
import { useDownloadStore } from '../store/useDownloadStore'
import { formatBytes } from '../utils/format'

const YTDLP_QUALITY_OPTIONS = [
  { value: 'best',  label: 'Best Quality',  hint: 'Highest available (needs ffmpeg for HD)' },
  { value: '1080p', label: '1080p HD',       hint: 'Full HD — needs ffmpeg' },
  { value: '720p',  label: '720p HD',        hint: 'HD — needs ffmpeg' },
  { value: '480p',  label: '480p SD',        hint: 'Standard definition' },
  { value: '360p',  label: '360p',           hint: 'Low quality, no ffmpeg needed' },
  { value: 'audio', label: 'Audio Only',     hint: 'MP3 / M4A audio track' },
]

function isYtDlpUrl(url: string): boolean {
  const l = url.toLowerCase()
  return l.includes('youtube.com/watch') || l.includes('youtu.be/') ||
    l.includes('youtube.com/shorts/') || l.includes('youtube.com/live/') ||
    l.includes('music.youtube.com/watch') || l.includes('youtube.com/embed/')
}

function useDebounce(value: string, delay: number): string {
  const [debounced, setDebounced] = useState(value)
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(value), delay)
    return () => clearTimeout(timer)
  }, [value, delay])
  return debounced
}

interface FileInfo {
  fileName: string
  fileSize: number
  resumable: boolean
  finalUrl?: string
}

export function NewDownloadDialog() {
  const { setShowNewDownload, config } = useDownloadStore()
  const [url, setUrl] = useState('')
  const [fileName, setFileName] = useState('')
  const [fileNameManual, setFileNameManual] = useState(false)
  const [savePath, setSavePath] = useState(config?.downloadDir || '')
  const [maxSegments, setMaxSegments] = useState(config?.maxSegmentsPerDownload || 8)
  const [showAdvanced, setShowAdvanced] = useState(false)
  const [ytQuality, setYtQuality] = useState('best')
  const [isFetching, setIsFetching] = useState(false)
  const [fileInfo, setFileInfo] = useState<FileInfo | null>(null)
  const [fetchError, setFetchError] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  const probeTimeout = useRef<ReturnType<typeof setTimeout> | null>(null)

  const debouncedUrl = useDebounce(url, 700)

  useEffect(() => {
    inputRef.current?.focus()
    // Pre-fill from clipboard
    navigator.clipboard?.readText().then(text => {
      if (text && (text.startsWith('http://') || text.startsWith('https://') || text.startsWith('ftp://'))) {
        setUrl(text)
      }
    }).catch(() => {})
  }, [])

  useEffect(() => {
    if (!debouncedUrl || !isValidUrl(debouncedUrl)) {
      setFileInfo(null)
      setFetchError('')
      return
    }
    probeFileInfo(debouncedUrl)
  }, [debouncedUrl])

  function isValidUrl(str: string): boolean {
    try {
      const u = new URL(str)
      return u.protocol === 'http:' || u.protocol === 'https:' || u.protocol === 'ftp:'
    } catch {
      return false
    }
  }

  async function probeFileInfo(targetUrl: string) {
    setIsFetching(true)
    setFetchError('')

    // Immediately show filename from URL as quick feedback
    const urlName = extractFileNameFromUrl(targetUrl)
    if (!fileNameManual && urlName) setFileName(urlName)

    try {
      const result = await invoke<any>('download_probe', { url: targetUrl })
      if (result.error) {
        // Probe failed — still show URL-extracted filename
        setFileInfo({ fileName: urlName || 'download', fileSize: -1, resumable: false })
        setFetchError('Could not fetch file info — download will still work')
      } else {
        const info: FileInfo = {
          fileName: result.fileName || urlName || 'download',
          fileSize: result.fileSize,
          resumable: result.resumable,
          finalUrl: result.finalUrl,
        }
        if (!fileNameManual && info.fileName) setFileName(info.fileName)
        setFileInfo(info)
      }
    } catch (err: any) {
      const urlName2 = extractFileNameFromUrl(targetUrl)
      if (!fileNameManual && urlName2) setFileName(urlName2)
      setFileInfo({ fileName: urlName2 || 'download', fileSize: -1, resumable: false })
    } finally {
      setIsFetching(false)
    }
  }

  function extractFileNameFromUrl(urlStr: string): string {
    try {
      const urlObj = new URL(urlStr)
      let pathname = urlObj.pathname
      if (pathname.endsWith('/')) pathname = pathname.slice(0, -1)
      const name = pathname.split('/').pop() || ''
      const decoded = decodeURIComponent(name)
      if (decoded && decoded.includes('.')) return decoded
      const fileParam = urlObj.searchParams.get('filename') ||
                         urlObj.searchParams.get('file') ||
                         urlObj.searchParams.get('name')
      if (fileParam) return decodeURIComponent(fileParam)
      return decoded || ''
    } catch {
      return ''
    }
  }

  const handleSelectFolder = async () => {
    const folder = await invoke<string | null>('dialog_select_folder')
    if (folder) setSavePath(folder)
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!url.trim()) return
    setShowNewDownload(false)

    const isYt = isYtDlpUrl(url.trim())
    try {
      await invoke('download_add', {
        request: {
          url: fileInfo?.finalUrl || url.trim(),
          fileName: fileName.trim() || undefined,
          savePath: savePath || undefined,
          maxSegments,
          autoStart: true,
          ytdlpQuality: isYt ? ytQuality : undefined,
        }
      })
    } catch (err) {
      console.error('Failed to add download:', err)
    }
  }

  return (
    <div
      className="fixed inset-0 bg-black/60 backdrop-blur-sm flex items-center justify-center z-50 animate-fade-in"
      onClick={() => setShowNewDownload(false)}
      onKeyDown={(e) => e.key === 'Escape' && setShowNewDownload(false)}
    >
      <div
        className="bg-qdm-surface border border-qdm-border rounded-xl w-[560px] shadow-2xl animate-slide-up"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 border-b border-qdm-border">
          <div className="flex items-center gap-2">
            <div className="w-7 h-7 rounded-lg bg-qdm-accent/20 flex items-center justify-center">
              <Zap size={14} className="text-qdm-accent" />
            </div>
            <h2 className="text-sm font-semibold text-qdm-text">New Download</h2>
          </div>
          <button
            onClick={() => setShowNewDownload(false)}
            className="p-1 rounded-lg hover:bg-white/10 transition-colors"
          >
            <X size={16} className="text-qdm-textSecondary" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="p-5 space-y-4">
          {/* URL input */}
          <div>
            <label className="block text-xs font-medium text-qdm-textSecondary mb-1.5">
              Download URL
            </label>
            <div className="relative">
              <Link size={14} className="absolute left-3 top-1/2 -translate-y-1/2 text-qdm-textMuted" />
              <input
                ref={inputRef}
                type="url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://example.com/file.zip"
                className="input-qdm pl-9 pr-10"
                required
              />
              {isFetching && (
                <Loader2 size={14} className="absolute right-3 top-1/2 -translate-y-1/2 text-qdm-accent animate-spin" />
              )}
            </div>
          </div>

          {/* File info bar */}
          {fileInfo && url && (
            <div className="flex items-center gap-3 p-3 bg-qdm-bg/80 rounded-lg border border-qdm-border/50 animate-fade-in">
              <FileDown size={16} className="text-qdm-accent shrink-0" />
              <div className="flex-1 min-w-0">
                <div className="text-xs text-qdm-text truncate font-medium">
                  {fileInfo.fileName || 'Unknown file'}
                </div>
                {fetchError && (
                  <div className="text-[10px] text-qdm-warning mt-0.5">{fetchError}</div>
                )}
              </div>
              <div className="flex items-center gap-2 shrink-0">
                {fileInfo.fileSize > 0 && (
                  <span className="flex items-center gap-1 text-[10px] text-qdm-textMuted">
                    <HardDrive size={10} />
                    {formatBytes(fileInfo.fileSize)}
                  </span>
                )}
                {fileInfo.resumable && (
                  <span className="flex items-center gap-1 px-1.5 py-0.5 bg-qdm-success/15 text-qdm-success rounded text-[9px] font-semibold">
                    <Shield size={8} />
                    Resumable
                  </span>
                )}
              </div>
            </div>
          )}

          {/* Quality selector — only for yt-dlp URLs */}
          {isYtDlpUrl(url) && (
            <div className="animate-fade-in">
              <label className="flex items-center gap-1.5 text-xs font-medium text-qdm-textSecondary mb-1.5">
                <Video size={12} className="text-qdm-accent" />
                Quality
              </label>
              <div className="grid grid-cols-3 gap-1.5">
                {YTDLP_QUALITY_OPTIONS.map(opt => (
                  <button
                    key={opt.value}
                    type="button"
                    onClick={() => setYtQuality(opt.value)}
                    className={`px-2 py-2 rounded-lg border text-[11px] font-medium text-left transition-all ${
                      ytQuality === opt.value
                        ? 'bg-qdm-accent/20 border-qdm-accent text-qdm-accent'
                        : 'bg-qdm-bg/60 border-qdm-border text-qdm-textSecondary hover:border-qdm-accent/50'
                    }`}
                  >
                    <div className="flex items-center gap-1">
                      {opt.value === 'audio' ? <Music size={10} /> : <Video size={10} />}
                      {opt.label}
                    </div>
                    <div className="text-[9px] text-qdm-textMuted mt-0.5 font-normal">{opt.hint}</div>
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* File name */}
          <div>
            <label className="block text-xs font-medium text-qdm-textSecondary mb-1.5">
              File Name
              <span className="text-qdm-textMuted font-normal ml-1">(auto-detected)</span>
            </label>
            <input
              type="text"
              value={fileName}
              onChange={(e) => { setFileName(e.target.value); setFileNameManual(true) }}
              placeholder="Auto-detect from URL"
              className="input-qdm"
            />
          </div>

          {/* Save path */}
          <div>
            <label className="block text-xs font-medium text-qdm-textSecondary mb-1.5">
              Save To
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={savePath}
                onChange={(e) => setSavePath(e.target.value)}
                placeholder="Default download directory"
                className="input-qdm flex-1"
              />
              <button
                type="button"
                onClick={handleSelectFolder}
                className="btn-secondary flex items-center gap-1.5 shrink-0"
              >
                <FolderOpen size={14} />
                Browse
              </button>
            </div>
          </div>

          {/* Advanced */}
          <div>
            <button
              type="button"
              onClick={() => setShowAdvanced(!showAdvanced)}
              className="flex items-center gap-1 text-xs text-qdm-textSecondary hover:text-qdm-text transition-colors"
            >
              {showAdvanced ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
              Advanced Options
            </button>

            {showAdvanced && (
              <div className="mt-3 p-3 bg-qdm-bg/60 rounded-lg border border-qdm-border/50 space-y-3 animate-slide-down">
                <div>
                  <label className="block text-xs font-medium text-qdm-textSecondary mb-2">
                    Connections (Segments): <span className="text-qdm-accent font-mono">{maxSegments}</span>
                  </label>
                  <input
                    type="range"
                    min="1"
                    max="32"
                    value={maxSegments}
                    onChange={(e) => setMaxSegments(parseInt(e.target.value))}
                    className="w-full accent-qdm-accent"
                  />
                  <div className="flex justify-between text-[9px] text-qdm-textMuted mt-1">
                    <span>1 (safe)</span>
                    <span>8 (recommended)</span>
                    <span>32 (max)</span>
                  </div>
                </div>
              </div>
            )}
          </div>

          {/* Actions */}
          <div className="flex justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={() => setShowNewDownload(false)}
              className="btn-secondary"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={!url.trim() || !isValidUrl(url.trim())}
              className="btn-primary flex items-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              <Zap size={14} />
              Download Now
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
