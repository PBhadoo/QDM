import React, { useState, useRef, useEffect } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { X, Lock, User, AlertCircle, Loader2 } from 'lucide-react'

interface AuthDialogProps {
  downloadId: string
  fileName?: string
  scheme: string
  onClose: () => void
}

export function AuthDialog({ downloadId, fileName, scheme, onClose }: AuthDialogProps) {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState('')
  const usernameRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    usernameRef.current?.focus()
  }, [])

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault()
    if (!username.trim()) { setError('Username is required'); return }
    setSubmitting(true)
    setError('')
    try {
      await invoke('download_provide_auth', { id: downloadId, username: username.trim(), password })
      onClose()
    } catch (err) {
      setError(String(err))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onClick={(e) => { if (e.target === e.currentTarget) onClose() }}
    >
      <div className="bg-qdm-surface border border-qdm-border rounded-xl shadow-2xl w-[420px] p-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <div className="flex items-center gap-2 text-qdm-text">
            <Lock className="w-4 h-4 text-purple-400" />
            <span className="font-semibold text-sm">Authentication Required</span>
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
          The server requires {scheme} authentication to download
          {fileName ? <> <span className="text-qdm-text font-medium">{fileName}</span></> : ' this file'}.
        </p>

        <form onSubmit={handleSubmit} className="space-y-3">
          {/* Username */}
          <div className="space-y-1">
            <label className="text-xs font-medium text-qdm-text-muted">Username</label>
            <div className="relative">
              <User className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-qdm-text-muted" />
              <input
                ref={usernameRef}
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                placeholder="Enter username"
                autoComplete="username"
                className="w-full bg-qdm-bg border border-qdm-border rounded-lg pl-9 pr-3 py-2
                           text-sm text-qdm-text placeholder:text-qdm-text-muted/50
                           focus:outline-none focus:border-purple-500 focus:ring-1 focus:ring-purple-500/30"
              />
            </div>
          </div>

          {/* Password */}
          <div className="space-y-1">
            <label className="text-xs font-medium text-qdm-text-muted">Password</label>
            <div className="relative">
              <Lock className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-qdm-text-muted" />
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                placeholder="Enter password"
                autoComplete="current-password"
                className="w-full bg-qdm-bg border border-qdm-border rounded-lg pl-9 pr-3 py-2
                           text-sm text-qdm-text placeholder:text-qdm-text-muted/50
                           focus:outline-none focus:border-purple-500 focus:ring-1 focus:ring-purple-500/30"
              />
            </div>
          </div>

          {/* Error */}
          {error && (
            <div className="flex items-center gap-2 text-red-400 text-xs py-2 px-3 bg-red-500/10 rounded-lg border border-red-500/20">
              <AlertCircle className="w-3.5 h-3.5 shrink-0" />
              <span>{error}</span>
            </div>
          )}

          {/* Actions */}
          <div className="flex items-center justify-end gap-2 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={submitting}
              className="px-4 py-2 text-sm text-qdm-text-muted hover:text-qdm-text
                         border border-qdm-border rounded-lg hover:border-qdm-text-muted
                         transition-colors disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="flex items-center gap-2 px-4 py-2 text-sm font-medium text-white
                         bg-purple-600 hover:bg-purple-500 rounded-lg transition-colors
                         disabled:opacity-60 disabled:cursor-not-allowed"
            >
              {submitting && <Loader2 className="w-3.5 h-3.5 animate-spin" />}
              {submitting ? 'Authenticating…' : 'Authenticate & Retry'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
