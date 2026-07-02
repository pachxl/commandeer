import { useCallback, useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { claudeUsage, type ClaudeLimit } from '../lib/tauri'

const CACHE_KEY = 'commandeer:claude-usage'
const CACHE_TTL_MS = 60_000
const RATE_LIMIT_KEY = 'commandeer:claude-rate-limit'

const KIND_ORDER = ['session', 'weekly_all', 'weekly_scoped']
const CLAUDE_ORANGE = '#D97757'

interface CachedUsage {
  limits: ClaudeLimit[]
  fetchedAt: number
}

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

function pad(n: number): string {
  return String(n).padStart(2, '0')
}

function formatReset(iso: string, now: number): string {
  const d = new Date(iso)
  if (isNaN(d.getTime())) return ''
  const diff = d.getTime() - now
  if (diff <= 0) return 'resets now'

  const withinDay = diff < 24 * 60 * 60 * 1000
  if (withinDay) {
    const totalSeconds = Math.ceil(diff / 1000)
    const h = Math.floor(totalSeconds / 3600)
    const m = Math.floor((totalSeconds % 3600) / 60)
    const s = totalSeconds % 60
    return `resets in ${h}:${pad(m)} (${h}:${pad(m)}:${pad(s)})`
  }

  const text = d.toLocaleDateString(undefined, { weekday: 'short', month: 'short', day: 'numeric' })
  return `resets ${text.toLowerCase()}`
}

function loadCached(): CachedUsage | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<CachedUsage> | ClaudeLimit[]
    // Legacy cache stored the array directly; migrate it on first read.
    if (Array.isArray(parsed)) {
      return parsed.length > 0 ? { limits: parsed, fetchedAt: 0 } : null
    }
    if (parsed.limits && parsed.fetchedAt) {
      return { limits: parsed.limits, fetchedAt: parsed.fetchedAt }
    }
    return null
  } catch {
    return null
  }
}

function isFresh(cache: CachedUsage | null): boolean {
  return !!cache && Date.now() - cache.fetchedAt < CACHE_TTL_MS
}

function loadRateLimitUntil(): number {
  try {
    const raw = localStorage.getItem(RATE_LIMIT_KEY)
    const n = raw ? Number(raw) : 0
    return Number.isFinite(n) ? n : 0
  } catch {
    return 0
  }
}

function parseRateLimitSeconds(message: string): number | null {
  const match = message.match(/rate limited; retry after (\d+)s/)
  return match ? Number(match[1]) : null
}

function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.ceil(ms / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  if (minutes > 0) return `${minutes}m ${seconds}s`
  return `${seconds}s`
}

export default function ClaudeUsage() {
  const cacheRef = useRef<CachedUsage | null>(loadCached())
  const rateLimitRef = useRef<number>(loadRateLimitUntil())
  const [cache, setCache] = useState<CachedUsage | null>(cacheRef.current)
  const [loading, setLoading] = useState(!isFresh(cacheRef.current) && Date.now() >= rateLimitRef.current)
  const [error, setError] = useState<string | null>(null)
  const [rateLimitedUntil, setRateLimitedUntil] = useState<number>(rateLimitRef.current)
  const [now, setNow] = useState<number>(Date.now())
  const limits = cache?.limits ?? null

  // Tick every second so the "try again in..." countdown updates.
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [])

  const refresh = useCallback(async (force = false) => {
    if (!force && isFresh(cacheRef.current)) {
      setLoading(false)
      return
    }
    if (Date.now() < rateLimitRef.current) {
      setLoading(false)
      return
    }
    setLoading(true)
    setError(null)
    try {
      const data = await claudeUsage()
      const sorted = (data.limits ?? [])
        .filter(l => KIND_ORDER.includes(l.kind))
        .sort((a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind))
      const next = { limits: sorted, fetchedAt: Date.now() }
      cacheRef.current = next
      setCache(next)
      localStorage.setItem(CACHE_KEY, JSON.stringify(next))
      rateLimitRef.current = 0
      setRateLimitedUntil(0)
      localStorage.removeItem(RATE_LIMIT_KEY)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      const retrySeconds = parseRateLimitSeconds(message)
      if (retrySeconds !== null) {
        const until = Date.now() + retrySeconds * 1000
        rateLimitRef.current = until
        setRateLimitedUntil(until)
        localStorage.setItem(RATE_LIMIT_KEY, String(until))
      }
      setError(message)
      console.error('claude usage:', err)
      // Keep showing stale cached data instead of wiping it on error.
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
        <span style={{ fontSize: 11, color: 'var(--text-dim)', fontWeight: 600 }}>
          <span style={{ color: CLAUDE_ORANGE }}>Claude</span>
        </span>
        {loading && (
          <svg style={{ animation: 'spin 1s linear infinite' }} width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 12a9 9 0 1 1-6.219-8.56" />
          </svg>
        )}
      </div>

      {error && (
        <div style={{
          padding: '4px 0',
          color: '#f7768e',
          fontSize: 11,
          lineHeight: 1.4,
        }}>
          {rateLimitedUntil > now
            ? `Rate limited — try again in ${formatDuration(rateLimitedUntil - now)}`
            : error}
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
                    <span style={{ color: 'var(--text)', opacity: 0.9, fontVariantNumeric: 'tabular-nums' }}>
                      {formatReset(limit.resets_at, now)}
                    </span>
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
