import { useCallback, useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { claudeUsage, type ClaudeLimit } from '../lib/tauri'

const CACHE_KEY = 'commandeer:claude-usage'
const MIN_REFRESH_MS = 60_000

const KIND_ORDER = ['session', 'weekly_all', 'weekly_scoped']

function limitLabel(limit: ClaudeLimit): string {
  if (limit.kind === 'session') return 'Current session'
  if (limit.kind === 'weekly_all') return 'Current week (all models)'
  const model = limit.scope?.model?.display_name
  return model ? `Current week (${model})` : 'Current week'
}

function barColor(limit: ClaudeLimit): string {
  if (limit.severity === 'error' || limit.severity === 'exceeded' || limit.percent >= 90) return '#f7768e'
  if (limit.severity === 'warning' || limit.percent >= 75) return '#e0af68'
  return 'var(--accent)'
}

function formatReset(iso: string): string {
  const d = new Date(iso)
  if (isNaN(d.getTime())) return ''
  const withinDay = d.getTime() - Date.now() < 24 * 60 * 60 * 1000
  const text = withinDay
    ? d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
    : d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' })
  return `resets ${text.toLowerCase()}`
}

function loadCached(): ClaudeLimit[] | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    return raw ? (JSON.parse(raw) as ClaudeLimit[]) : null
  } catch {
    return null
  }
}

export default function ClaudeUsage() {
  const [limits, setLimits] = useState<ClaudeLimit[] | null>(loadCached)
  const [loading, setLoading] = useState(limits === null)
  const [error, setError] = useState<string | null>(null)
  const lastFetch = useRef(0)

  const refresh = useCallback(async (force = false) => {
    if (!force && Date.now() - lastFetch.current < MIN_REFRESH_MS) return
    setLoading(true)
    setError(null)
    lastFetch.current = Date.now()
    try {
      const data = await claudeUsage()
      const sorted = (data.limits ?? [])
        .filter(l => KIND_ORDER.includes(l.kind))
        .sort((a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind))
      setLimits(sorted)
      localStorage.setItem(CACHE_KEY, JSON.stringify(sorted))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setError(message)
      console.error('claude usage:', err)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh()
    const unlisten = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) refresh()
    })
    return () => { unlisten.then(fn => fn()) }
  }, [refresh])

  const spinner = (
    <svg style={spinStyle} width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M21 12a9 9 0 1 1-6.219-8.56" />
    </svg>
  )

  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      gap: 8,
      padding: '8px 12px 10px',
      borderTop: '1px solid var(--border)',
      fontFamily: 'var(--font-ui)',
      userSelect: 'none',
    }}>
      <div style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
      }}>
        <span style={{ fontSize: 11, color: 'var(--text-dim)', fontWeight: 600 }}>Claude usage</span>
        <button
          onClick={() => refresh(true)}
          disabled={loading}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 4,
            background: 'transparent',
            border: 'none',
            color: 'var(--text-dim)',
            fontSize: 10,
            fontFamily: 'var(--font-ui)',
            cursor: loading ? 'wait' : 'pointer',
            padding: '2px 4px',
            borderRadius: 3,
            opacity: loading ? 0.7 : 1,
          }}
          title="Refresh now"
        >
          {loading ? spinner : (
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21.5 2v6h-6M2.5 22v-6h6M2 11.5a10 10 0 0 1 18.8-4.3M22 12.5a10 10 0 0 1-18.8 4.3" />
            </svg>
          )}
          <span>{loading ? 'loading…' : 'refresh'}</span>
        </button>
      </div>

      {error && (
        <div style={{
          padding: '4px 0',
          color: '#f7768e',
          fontSize: 11,
          lineHeight: 1.4,
        }}>
          {error}
        </div>
      )}

      {!error && (!limits || limits.length === 0) && !loading && (
        <div style={{
          padding: '4px 0',
          color: 'var(--text-dim)',
          fontSize: 11,
        }}>
          No usage data available.
        </div>
      )}

      {!error && limits && limits.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {limits.map(limit => {
            const pct = Math.min(100, Math.max(0, Math.round(limit.percent)))
            const color = barColor(limit)
            return (
              <div key={limit.kind + (limit.scope?.model?.display_name ?? '')}>
                <div style={{
                  display: 'flex',
                  alignItems: 'baseline',
                  justifyContent: 'space-between',
                  marginBottom: 4,
                }}>
                  <span style={{ fontSize: 11, color: 'var(--text)' }}>
                    {limitLabel(limit)}
                  </span>
                  <span style={{ fontSize: 10, color: 'var(--text-dim)' }}>
                    <span style={{ color: pct >= 75 ? color : 'var(--text)' }}>{pct}% used</span>
                    <span style={{ opacity: 0.5, margin: '0 4px' }}>·</span>
                    {formatReset(limit.resets_at)}
                  </span>
                </div>
                <div style={{
                  height: 4,
                  borderRadius: 2,
                  background: 'rgba(255,255,255,0.06)',
                  overflow: 'hidden',
                }}>
                  <div style={{
                    height: '100%',
                    width: `${pct}%`,
                    borderRadius: 2,
                    background: color,
                    transition: 'width 0.4s ease',
                  }} />
                </div>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}

const spinStyle: React.CSSProperties = {
  animation: 'spin 1s linear infinite',
}
