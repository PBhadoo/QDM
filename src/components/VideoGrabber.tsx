import React, { useState, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  Video, Download, Trash2, RefreshCw, Globe,
  Wifi, WifiOff, Play, Music, Radio, Tv
} from 'lucide-react'

interface MediaItem {
  id: string
  name: string
  description: string
  tabUrl: string
  type: 'video' | 'audio' | 'hls' | 'dash' | 'youtube'
  size: number
  dateAdded: string
}

function getTypeIcon(type: string) {
  switch (type) {
    case 'youtube': return <Tv size={14} className="text-red-400" />
    case 'hls': return <Radio size={14} className="text-orange-400" />
    case 'dash': return <Radio size={14} className="text-blue-400" />
    case 'audio': return <Music size={14} className="text-pink-400" />
    default: return <Video size={14} className="text-purple-400" />
  }
}

function getTypeBadge(type: string) {
  const styles: Record<string, string> = {
    youtube: 'bg-red-500/15 text-red-400 border-red-500/30',
    hls: 'bg-orange-500/15 text-orange-400 border-orange-500/30',
    dash: 'bg-blue-500/15 text-blue-400 border-blue-500/30',
    audio: 'bg-pink-500/15 text-pink-400 border-pink-500/30',
    video: 'bg-purple-500/15 text-purple-400 border-purple-500/30',
  }
  return styles[type] || styles.video
}

export function VideoGrabber() {
  const [mediaList, setMediaList] = useState<MediaItem[]>([])
  const [browserStatus, setBrowserStatus] = useState({ running: false, port: 8597, mediaCount: 0 })
  const [isLoading, setIsLoading] = useState(false)

  const loadMedia = async () => {
    setIsLoading(true)
    try {
      const [list, status] = await Promise.all([
        invoke<MediaItem[]>('browser_get_media_list'),
        invoke<{ running: boolean; port: number; mediaCount: number }>('browser_get_status'),
      ])
      setMediaList(list)
      setBrowserStatus(status)
    } catch (err) {
      console.error('Failed to load media:', err)
    }
    setIsLoading(false)
  }

  useEffect(() => {
    loadMedia()
    const unlistenPromise = listen('media:added', () => loadMedia())
    const interval = setInterval(loadMedia, 5000)
    return () => {
      unlistenPromise.then(unlisten => unlisten())
      clearInterval(interval)
    }
  }, [])

  const handleDownload = async (mediaId: string) => {
    await invoke('browser_download_media', { mediaId })
    // Remove from list visually
    setMediaList(prev => prev.filter(m => m.id !== mediaId))
  }

  const handleClear = async () => {
    await invoke('browser_clear_media')
    setMediaList([])
  }

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      {/* Header */}
      <div className="px-4 py-3 border-b border-qdm-border/50 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h2 className="text-sm font-semibold text-qdm-text">Video & Media Grabber</h2>
          <span className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[10px] font-medium border
            ${browserStatus.running
              ? 'bg-qdm-success/15 text-qdm-success border-qdm-success/30'
              : 'bg-qdm-danger/15 text-qdm-danger border-qdm-danger/30'}`}>
            {browserStatus.running ? <Wifi size={10} /> : <WifiOff size={10} />}
            {browserStatus.running ? `Listening on :${browserStatus.port}` : 'Not connected'}
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button onClick={loadMedia} className="btn-ghost flex items-center gap-1.5" disabled={isLoading}>
            <RefreshCw size={13} className={isLoading ? 'animate-spin' : ''} />
            Refresh
          </button>
          {mediaList.length > 0 && (
            <button onClick={handleClear} className="btn-ghost flex items-center gap-1.5 text-qdm-danger/70 hover:text-qdm-danger">
              <Trash2 size={13} />
              Clear All
            </button>
          )}
        </div>
      </div>

      {/* Info Banner */}
      <div className="mx-4 mt-3 p-3 bg-qdm-accent/5 border border-qdm-accent/20 rounded-lg">
        <div className="flex items-start gap-2">
          <Globe size={14} className="text-qdm-accent mt-0.5 shrink-0" />
          <div>
            <p className="text-xs text-qdm-textSecondary">
              Install the <span className="text-qdm-accent font-medium">QDM Browser Extension</span> to automatically detect
              videos, audio, and streams from any website including YouTube, Twitch, and more.
            </p>
            <p className="text-[10px] text-qdm-textMuted mt-1">
              Extension communicates with QDM via local server on port {browserStatus.port}
            </p>
          </div>
        </div>
      </div>

      {/* Media List */}
      {mediaList.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center text-center p-8">
          <div className="w-16 h-16 rounded-2xl bg-qdm-surface flex items-center justify-center mb-3">
            <Video size={28} className="text-qdm-textMuted" />
          </div>
          <h3 className="text-sm font-semibold text-qdm-text mb-1">No media detected</h3>
          <p className="text-xs text-qdm-textMuted max-w-sm">
            Browse web pages with video or audio content. QDM will automatically
            detect and list downloadable media here.
          </p>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto p-4 space-y-2">
          {mediaList.map((media) => (
            <div
              key={media.id}
              className="group bg-qdm-surface/50 hover:bg-qdm-surfaceHover border border-qdm-border/50
                         rounded-lg p-3 transition-all duration-150"
            >
              <div className="flex items-center gap-3">
                {/* Type icon */}
                <div className="w-9 h-9 rounded-lg bg-qdm-bg flex items-center justify-center shrink-0">
                  {getTypeIcon(media.type)}
                </div>

                {/* Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2 mb-0.5">
                    <span className="text-sm text-qdm-text truncate" title={media.name}>
                      {media.name}
                    </span>
                    <span className={`inline-flex items-center px-1.5 py-0 rounded text-[9px] font-medium border uppercase ${getTypeBadge(media.type)}`}>
                      {media.type}
                    </span>
                  </div>
                  <div className="flex items-center gap-2 text-[10px] text-qdm-textMuted">
                    <span>{media.description}</span>
                    {media.tabUrl && (
                      <>
                        <span className="text-qdm-border">•</span>
                        <span className="truncate max-w-[200px]">{media.tabUrl}</span>
                      </>
                    )}
                  </div>
                </div>

                {/* Download button */}
                <button
                  onClick={() => handleDownload(media.id)}
                  className="shrink-0 flex items-center gap-1.5 px-3 py-1.5 bg-qdm-accent/10 hover:bg-qdm-accent/20
                             text-qdm-accent border border-qdm-accent/30 rounded-lg text-xs font-medium
                             transition-all duration-150 opacity-0 group-hover:opacity-100"
                >
                  <Download size={12} />
                  Download
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
