import React from 'react'
import {
  Download, Archive, FileText, Music, Video, Monitor,
  MoreHorizontal, Settings, Info
} from 'lucide-react'
import { useDownloadStore } from '../store/useDownloadStore'
import type { DownloadCategory } from '../types/download'

interface CategoryItem {
  id: DownloadCategory
  label: string
  icon: React.ReactNode
  color: string
}

const categories: CategoryItem[] = [
  { id: 'all', label: 'All Downloads', icon: <Download size={16} />, color: 'text-qdm-accent' },
  { id: 'compressed', label: 'Compressed', icon: <Archive size={16} />, color: 'text-yellow-400' },
  { id: 'documents', label: 'Documents', icon: <FileText size={16} />, color: 'text-blue-400' },
  { id: 'music', label: 'Music', icon: <Music size={16} />, color: 'text-pink-400' },
  { id: 'videos', label: 'Videos', icon: <Video size={16} />, color: 'text-red-400' },
  { id: 'programs', label: 'Programs', icon: <Monitor size={16} />, color: 'text-green-400' },
  { id: 'other', label: 'Other', icon: <MoreHorizontal size={16} />, color: 'text-qdm-textSecondary' },
]

export function Sidebar() {
  const { activeCategory, setActiveCategory, downloads, setShowSettings, setShowAbout } = useDownloadStore()

  const getCategoryCount = (category: DownloadCategory): number => {
    if (category === 'all') return downloads.length
    return downloads.filter(d => d.category === category).length
  }

  const activeCount = downloads.filter(d => d.status === 'downloading').length
  const completedCount = downloads.filter(d => d.status === 'completed').length

  return (
    <div className="w-56 bg-qdm-surface/50 border-r border-qdm-border flex flex-col shrink-0">
      {/* Status Summary */}
      <div className="p-4 border-b border-qdm-border">
        <div className="grid grid-cols-2 gap-2">
          <div className="bg-qdm-accent/10 rounded-lg p-2.5 text-center">
            <div className="text-lg font-bold text-qdm-accent">{activeCount}</div>
            <div className="text-[10px] text-qdm-textSecondary uppercase tracking-wider">Active</div>
          </div>
          <div className="bg-qdm-success/10 rounded-lg p-2.5 text-center">
            <div className="text-lg font-bold text-qdm-success">{completedCount}</div>
            <div className="text-[10px] text-qdm-textSecondary uppercase tracking-wider">Done</div>
          </div>
        </div>
      </div>

      {/* Categories */}
      <div className="flex-1 overflow-y-auto py-2">
        <div className="px-3 mb-1">
          <span className="text-[10px] font-semibold text-qdm-textMuted uppercase tracking-widest">
            Categories
          </span>
        </div>
        {categories.map((cat) => {
          const count = getCategoryCount(cat.id)
          const isActive = activeCategory === cat.id
          return (
            <button
              key={cat.id}
              onClick={() => setActiveCategory(cat.id)}
              className={`w-full flex items-center gap-3 px-4 py-2 text-sm transition-all duration-150
                ${isActive 
                  ? 'bg-qdm-accent/10 text-qdm-text border-r-2 border-qdm-accent' 
                  : 'text-qdm-textSecondary hover:bg-qdm-surfaceHover hover:text-qdm-text'
                }`}
            >
              <span className={isActive ? cat.color : 'text-qdm-textMuted'}>
                {cat.icon}
              </span>
              <span className="flex-1 text-left">{cat.label}</span>
              {count > 0 && (
                <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded-full
                  ${isActive ? 'bg-qdm-accent/20 text-qdm-accent' : 'bg-qdm-bg/50 text-qdm-textMuted'}`}>
                  {count}
                </span>
              )}
            </button>
          )
        })}
      </div>

      {/* Bottom Actions */}
      <div className="border-t border-qdm-border p-2 space-y-0.5">
        <button
          onClick={() => setShowSettings(true)}
          className="w-full flex items-center gap-3 px-3 py-2 text-sm text-qdm-textSecondary 
                     hover:text-qdm-text hover:bg-qdm-surfaceHover rounded-lg transition-colors"
        >
          <Settings size={15} />
          <span>Settings</span>
        </button>
        <button
          onClick={() => setShowAbout(true)}
          className="w-full flex items-center gap-3 px-3 py-2 text-sm text-qdm-textSecondary 
                     hover:text-qdm-text hover:bg-qdm-surfaceHover rounded-lg transition-colors"
        >
          <Info size={15} />
          <span>About</span>
        </button>
      </div>
    </div>
  )
}
