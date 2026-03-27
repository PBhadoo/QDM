/**
 * QDM Download Store - Zustand state management
 */

import { create } from 'zustand'
import { invoke } from '@tauri-apps/api/core'
import type { DownloadItem, DownloadCategory, DownloadProgress, AppConfig } from '../types/download'

export interface QualityRequest {
  url: string
  fileName?: string
  sourcePageUrl?: string
  ytdlpCookies?: string
  headers?: Record<string, string>
}

export interface YtdlpLog {
  ts: string
  downloadId: string
  level: 'cmd' | 'stdout' | 'stderr' | 'error' | 'info'
  msg: string
}

interface AuthChallenge {
  id: string
  scheme: string
  fileName?: string
}

interface LinkExpiredChallenge {
  id: string
  fileName?: string
  sourcePageUrl?: string
}

interface DownloadStore {
  // State
  downloads: DownloadItem[]
  selectedIds: Set<string>
  activeCategory: DownloadCategory
  searchQuery: string
  showNewDownload: boolean
  showSettings: boolean
  showAbout: boolean
  config: AppConfig | null
  authChallenge: AuthChallenge | null
  linkExpiredChallenge: LinkExpiredChallenge | null
  ytdlpLogs: YtdlpLog[]
  showYtdlpLogs: boolean
  qualityRequest: QualityRequest | null

  // Actions
  setAuthChallenge: (challenge: AuthChallenge | null) => void
  setLinkExpiredChallenge: (challenge: LinkExpiredChallenge | null) => void
  setDownloads: (downloads: DownloadItem[]) => void
  updateDownload: (id: string, updates: Partial<DownloadItem>) => void
  updateProgress: (progress: DownloadProgress) => void
  addDownload: (item: DownloadItem) => void
  removeDownloadFromList: (id: string) => void
  setSelectedIds: (ids: Set<string>) => void
  toggleSelect: (id: string) => void
  selectAll: () => void
  clearSelection: () => void
  setActiveCategory: (category: DownloadCategory) => void
  setSearchQuery: (query: string) => void
  setShowNewDownload: (show: boolean) => void
  setShowSettings: (show: boolean) => void
  setShowAbout: (show: boolean) => void
  setConfig: (config: AppConfig) => void
  loadDownloads: () => Promise<void>
  filteredDownloads: () => DownloadItem[]
  addYtdlpLog: (log: YtdlpLog) => void
  clearYtdlpLogs: () => void
  setShowYtdlpLogs: (show: boolean) => void
  setQualityRequest: (req: QualityRequest | null) => void
}

export const useDownloadStore = create<DownloadStore>((set, get) => ({
  downloads: [],
  selectedIds: new Set<string>(),
  activeCategory: 'all',
  searchQuery: '',
  showNewDownload: false,
  showSettings: false,
  showAbout: false,
  config: null,
  authChallenge: null,
  linkExpiredChallenge: null,
  ytdlpLogs: [],
  showYtdlpLogs: false,
  qualityRequest: null,

  setAuthChallenge: (challenge) => set({ authChallenge: challenge }),
  setLinkExpiredChallenge: (challenge) => set({ linkExpiredChallenge: challenge }),
  setDownloads: (downloads) => set({ downloads }),

  updateDownload: (id, updates) => set((state) => ({
    downloads: state.downloads.map(d =>
      d.id === id ? { ...d, ...updates } : d
    )
  })),

  updateProgress: (progress) => set((state) => ({
    downloads: state.downloads.map(d =>
      d.id === progress.id ? {
        ...d,
        downloaded: progress.downloaded,
        progress: progress.progress,
        speed: progress.speed,
        eta: progress.eta,
        segments: progress.segments,
        status: progress.status,
      } : d
    )
  })),

  addDownload: (item) => set((state) => ({
    downloads: [item, ...state.downloads]
  })),

  removeDownloadFromList: (id) => set((state) => ({
    downloads: state.downloads.filter(d => d.id !== id),
    selectedIds: new Set([...state.selectedIds].filter(sid => sid !== id))
  })),

  setSelectedIds: (ids) => set({ selectedIds: ids }),

  toggleSelect: (id) => set((state) => {
    const newSet = new Set(state.selectedIds)
    if (newSet.has(id)) {
      newSet.delete(id)
    } else {
      newSet.add(id)
    }
    return { selectedIds: newSet }
  }),

  selectAll: () => set((state) => ({
    selectedIds: new Set(get().filteredDownloads().map(d => d.id))
  })),

  clearSelection: () => set({ selectedIds: new Set() }),

  setActiveCategory: (category) => set({ activeCategory: category, selectedIds: new Set() }),

  setSearchQuery: (query) => set({ searchQuery: query }),

  setShowNewDownload: (show) => set({ showNewDownload: show }),

  setShowSettings: (show) => set({ showSettings: show }),

  setShowAbout: (show) => set({ showAbout: show }),

  setConfig: (config) => set({ config }),

  loadDownloads: async () => {
    try {
      const downloads = await invoke<DownloadItem[]>('download_get_all')
      set({ downloads })
    } catch (err) {
      console.error('Failed to load downloads:', err)
    }
  },

  filteredDownloads: () => {
    const { downloads, activeCategory, searchQuery } = get()
    let filtered = downloads

    if (activeCategory !== 'all') {
      if (activeCategory === 'compressed' || activeCategory === 'documents' ||
          activeCategory === 'music' || activeCategory === 'videos' ||
          activeCategory === 'programs' || activeCategory === 'other') {
        filtered = filtered.filter(d => d.category === activeCategory)
      }
    }

    if (searchQuery) {
      const q = searchQuery.toLowerCase()
      filtered = filtered.filter(d =>
        d.fileName.toLowerCase().includes(q) ||
        d.url.toLowerCase().includes(q)
      )
    }

    return filtered
  },

  addYtdlpLog: (log) => set((state) => ({
    ytdlpLogs: [...state.ytdlpLogs, log].slice(-2000), // keep last 2000 lines
  })),

  clearYtdlpLogs: () => set({ ytdlpLogs: [] }),

  setShowYtdlpLogs: (show) => set({ showYtdlpLogs: show }),
  setQualityRequest: (req) => set({ qualityRequest: req }),
}))
