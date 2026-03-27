import React from 'react'
import { invoke } from '@tauri-apps/api/core'
import { X, LinkIcon, RefreshCw } from 'lucide-react'

interface LinkExpiredDialogProps {
  downloadId: string
  fileName?: string
  sourcePageUrl?: string
  onClose: () => void
}

export function LinkExpiredDialog({ downloadId, fileName, sourcePageUrl, onClose }: LinkExpiredDialogProps) {
  async function handleReopen() {
    await invoke('download_reopen_source', { id: downloadId })
    onClose()
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={(e) => { if (e.target === e.currentTarget) onClose() }}
    >
      <div className="bg-qdm-surface border border-qdm-border rounded-xl shadow-2xl w-[440px] p-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <div className="flex items-center gap-2 text-qdm-text">
            <LinkIcon className="w-4 h-4 text-orange-400" />
            <span className="font-semibold text-sm">Download Link Expired</span>
          </div>
          <button
            onClick={onClose}
            className="text-qdm-text-muted hover:text-qdm-text transition-colors rounded p-0.5"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Description */}
        <p className="text-qdm-text-muted text-xs mb-4 leading-relaxed">
          The download link for{' '}
          {fileName
            ? <span className="text-qdm-text font-medium">{fileName}</span>
            : 'this file'
          }{' '}
          has expired. This usually happens with time-limited CDN links (AWS S3, CloudFront, Azure).
        </p>

        {sourcePageUrl && (
          <p className="text-qdm-text-muted text-xs mb-5 leading-relaxed">
            Re-open the source page to get a fresh link, then the download will resume automatically.
          </p>
        )}

        {/* Actions */}
        <div className="flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 text-sm text-qdm-text-muted hover:text-qdm-text
                       border border-qdm-border rounded-lg hover:border-qdm-text-muted
                       transition-colors"
          >
            Dismiss
          </button>
          {sourcePageUrl && (
            <button
              type="button"
              onClick={handleReopen}
              className="flex items-center gap-2 px-4 py-2 text-sm font-medium text-white
                         bg-orange-600 hover:bg-orange-500 rounded-lg transition-colors"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              Re-open Source Page
            </button>
          )}
        </div>
      </div>
    </div>
  )
}
