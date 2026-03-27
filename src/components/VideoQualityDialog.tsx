import React, { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { X, Download, Loader2 } from 'lucide-react'
import type { QualityRequest } from '../store/useDownloadStore'
import { useDownloadStore } from '../store/useDownloadStore'

interface FormatOption {
  formatId: string
  label: string
  note: string
  height?: number
  isAudioOnly: boolean
  fileSize?: number
}

interface FormatResult {
  title: string
  formats: FormatOption[]
}

interface Props {
  request: QualityRequest
  onClose: () => void
}

export function VideoQualityDialog({ request, onClose }: Props) {
  const [result, setResult] = useState<FormatResult | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)
  const { updateDownload } = useDownloadStore()

  const formats = result?.formats ?? null
  const videoTitle = result?.title || request.fileName || null

  const displayName = videoTitle
    || request.url.replace(/.*youtube\.com\/watch\?v=/, '').replace(/&.*/, '')
    || request.url

  useEffect(() => {
    invoke<FormatResult>('ytdlp_list_formats', { url: request.url })
      .then(setResult)
      .catch(e => setError(String(e)))
  }, [request.url])

  async function pick(fmt: FormatOption) {
    setLoading(true)
    try {
      // Use the real video title as the initial filename
      const fileName = videoTitle ? `${videoTitle}.mp4` : (request.fileName || null)
      const item = await invoke<any>('download_add', {
        request: {
          url: request.url,
          fileName,
          savePath: null,
          headers: request.headers || null,
          maxSegments: null,
          autoStart: true,
          sourcePageUrl: request.sourcePageUrl || null,
          ytdlpQuality: fmt.formatId,
          ytdlpCookies: request.ytdlpCookies || null,
        },
      })
      // Apply known file size immediately so the list doesn't show 0 B
      if (item?.id && fmt.fileSize && fmt.fileSize > 0) {
        updateDownload(item.id, { fileSize: fmt.fileSize })
      }
      onClose()
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-sm flex items-center justify-center p-4">
      <div className="bg-qdm-surface border border-qdm-border rounded-xl w-full max-w-sm shadow-2xl">
        {/* Header */}
        <div className="flex items-center gap-2 px-4 py-3 border-b border-qdm-border">
          <Download size={14} className="text-qdm-accent shrink-0" />
          <span className="text-sm font-semibold text-qdm-text flex-1">Select Quality</span>
          <button onClick={onClose} className="p-1 rounded text-qdm-textMuted hover:text-qdm-text hover:bg-qdm-surfaceHover transition-colors">
            <X size={14} />
          </button>
        </div>

        {/* URL */}
        <div className="px-4 pt-3 pb-1">
          <p className="text-[10px] text-qdm-textMuted font-mono truncate" title={request.url}>
            {displayName}
          </p>
        </div>

        {/* Content */}
        <div className="p-3 flex flex-col gap-1.5">
          {!formats && !error && (
            <div className="flex items-center justify-center gap-2 py-6 text-qdm-textMuted text-xs">
              <Loader2 size={14} className="animate-spin" />
              Fetching available formats…
            </div>
          )}

          {error && (
            <div className="text-xs text-red-400 px-2 py-3 bg-red-500/10 rounded-lg">
              {error}
            </div>
          )}

          {formats && formats.length === 0 && (
            <div className="text-xs text-qdm-textMuted px-2 py-3">
              No formats found for this URL.
            </div>
          )}

          {formats && formats.map(f => (
            <button
              key={f.formatId}
              disabled={loading}
              onClick={() => pick(f)}
              className="flex items-center gap-3 px-3 py-2.5 rounded-lg bg-qdm-bg hover:bg-qdm-surfaceHover border border-qdm-border hover:border-qdm-accent/40 transition-all text-left disabled:opacity-50 group"
            >
              <span className={`text-sm font-bold w-16 shrink-0 ${f.isAudioOnly ? 'text-qdm-success' : f.height && f.height >= 1080 ? 'text-qdm-accent' : f.height && f.height >= 720 ? 'text-blue-400' : 'text-qdm-textSecondary'}`}>
                {f.label}
              </span>
              <span className="text-xs text-qdm-textMuted group-hover:text-qdm-textSecondary transition-colors truncate">
                {f.note}
              </span>
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
