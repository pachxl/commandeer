import { useCallback, useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { codexUsage, type CodexRateLimit, type CodexRateLimitWindow, type CodexUsageData } from '../lib/tauri'

const CACHE_KEY = 'commandeer:codex-usage'
const STATE_KEY = 'commandeer:codex-poll-state'
const BASE_INTERVAL_MS = 5 * 60_000
const MAX_INTERVAL_MS = 30 * 60_000
const JITTER_MS = 15_000
const CODEX_GREEN = '#10A37F'

interface CachedUsage {
  data: CodexUsageData
  fetchedAt: number
}

interface PollState {
  interval: number
  nextAllowedAt: number
}

interface DisplayLimit {
  key: string
  label: string
  percent: number
  resetAt?: number | null
  exceeded: boolean
}

function clampInterval(ms: number): number {
  return Math.min(MAX_INTERVAL_MS, Math.max(BASE_INTERVAL_MS, ms))
}

function loadCached(): CachedUsage | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    if (!raw) return null
    const parsed = JSON.parse(raw) as Partial<CachedUsage>
    return parsed.data && parsed.fetchedAt ? { data: parsed.data, fetchedAt: parsed.fetchedAt } : null
  } catch {
    return null
  }
}

function loadState(): PollState {
  try {
    const raw = localStorage.getItem(STATE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<PollState>
      const interval = Number(parsed.interval)
      const nextAllowedAt = Number(parsed.nextAllowedAt)
      return {
        interval: Number.isFinite(interval) ? clampInterval(interval) : BASE_INTERVAL_MS,
        nextAllowedAt: Number.isFinite(nextAllowedAt) ? nextAllowedAt : 0,
      }
    }
  } catch {
    /* fall through to defaults */
  }
  return { interval: BASE_INTERVAL_MS, nextAllowedAt: 0 }
}

function saveState(state: PollState): void {
  try {
    localStorage.setItem(STATE_KEY, JSON.stringify(state))
  } catch {
    /* ignore quota / private-mode failures */
  }
}

function parseRateLimitSeconds(message: string): number | null {
  const match = message.match(/rate limited; retry after (\d+)s/)
  return match ? Number(match[1]) : null
}

function pad(n: number): string {
  return String(n).padStart(2, '0')
}

function formatReset(timestampSeconds: number | null | undefined, now: number): string {
  if (!timestampSeconds) return ''
  const resetAt = timestampSeconds * 1000
  const diff = resetAt - now
  if (diff <= 0) return 'resets now'

  if (diff < 24 * 60 * 60 * 1000) {
    const totalSeconds = Math.ceil(diff / 1000)
    const hours = Math.floor(totalSeconds / 3600)
    const minutes = Math.floor((totalSeconds % 3600) / 60)
    const seconds = totalSeconds % 60
    return `resets in ${hours}:${pad(minutes)} (${hours}:${pad(minutes)}:${pad(seconds)})`
  }

  const date = new Date(resetAt)
  const text = date.toLocaleDateString(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
  })
  return `resets ${text.toLowerCase()}`
}

function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.ceil(ms / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  return minutes > 0 ? `${minutes}m ${seconds}s` : `${seconds}s`
}

function periodLabel(window: CodexRateLimitWindow): string {
  const seconds = window.limit_window_seconds
  if (!seconds) return 'usage'
  if (seconds <= 6 * 60 * 60) return 'session'
  if (seconds >= 6 * 24 * 60 * 60 && seconds <= 8 * 24 * 60 * 60) return 'week'
  if (seconds % (24 * 60 * 60) === 0) return `${seconds / (24 * 60 * 60)} days`
  if (seconds % (60 * 60) === 0) return `${seconds / (60 * 60)} hours`
  return 'usage'
}

function addWindows(target: DisplayLimit[], rateLimit: CodexRateLimit, keyPrefix: string, name?: string): void {
  const windows = [rateLimit.primary_window, rateLimit.secondary_window]
  for (const [index, window] of windows.entries()) {
    if (!window) continue
    const period = periodLabel(window)
    const label = name
      ? `${name} (${period})`
      : period === 'session'
        ? 'Current session'
        : period === 'week'
          ? 'Current week'
          : 'Current usage'
    target.push({
      key: `${keyPrefix}:${index}`,
      label,
      percent: window.used_percent,
      resetAt: window.reset_at,
      exceeded: rateLimit.limit_reached || !rateLimit.allowed,
    })
  }
}

function displayLimits(data: CodexUsageData): DisplayLimit[] {
  const limits: DisplayLimit[] = []
  // Only the primary rate limit (session + week windows) is shown. The metered
  // "spark" feature in `additional_rate_limits` is deliberately omitted.
  if (data.rate_limit) addWindows(limits, data.rate_limit, 'codex')
  return limits
}

function barColor(limit: DisplayLimit): string {
  if (limit.exceeded || limit.percent >= 90) return '#f7484f'
  if (limit.percent >= 75) return '#f5c542'
  return '#4a9eff'
}

function planLabel(plan: string | null | undefined): string {
  if (!plan) return ''
  return plan
    .split('_')
    .map(word => word.charAt(0).toUpperCase() + word.slice(1))
    .join(' ')
}

export default function CodexUsage() {
  const cacheRef = useRef<CachedUsage | null>(loadCached())
  const stateRef = useRef<PollState>(loadState())
  const inFlightRef = useRef<Promise<void> | null>(null)
  const [cache, setCache] = useState<CachedUsage | null>(cacheRef.current)
  const [loading, setLoading] = useState(!cacheRef.current && Date.now() >= stateRef.current.nextAllowedAt)
  const [error, setError] = useState<string | null>(null)
  const [rateLimitedUntil, setRateLimitedUntil] = useState(0)
  const [now, setNow] = useState(Date.now())

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [])

  const refresh = useCallback((): Promise<void> => {
    if (inFlightRef.current) return inFlightRef.current
    if (Date.now() < stateRef.current.nextAllowedAt) {
      setLoading(false)
      return Promise.resolve()
    }

    setLoading(true)
    setError(null)
    const run = (async () => {
      try {
        const data = await codexUsage()
        const next = { data, fetchedAt: Date.now() }
        cacheRef.current = next
        setCache(next)
        try {
          localStorage.setItem(CACHE_KEY, JSON.stringify(next))
        } catch {
          /* ignore storage failures */
        }

        const interval = clampInterval(stateRef.current.interval / 2)
        stateRef.current = {
          interval,
          nextAllowedAt: Date.now() + interval + Math.floor(Math.random() * JITTER_MS),
        }
        saveState(stateRef.current)
        setRateLimitedUntil(0)
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        const retrySeconds = parseRateLimitSeconds(message)
        if (retrySeconds !== null) {
          const interval = clampInterval(Math.max(retrySeconds * 1000, stateRef.current.interval * 2))
          const nextAllowedAt = Date.now() + interval
          stateRef.current = { interval, nextAllowedAt }
          setRateLimitedUntil(nextAllowedAt)
        } else {
          stateRef.current = {
            ...stateRef.current,
            nextAllowedAt: Date.now() + BASE_INTERVAL_MS,
          }
        }
        saveState(stateRef.current)
        setError(message)
        console.error('codex usage:', err)
      } finally {
        setLoading(false)
        inFlightRef.current = null
      }
    })()

    inFlightRef.current = run
    return run
  }, [])

  useEffect(() => {
    void refresh()
    const unlisten = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) void refresh()
    })
    return () => {
      void unlisten.then(fn => fn())
    }
  }, [refresh])

  const limits = cache ? displayLimits(cache.data) : []
  const showRateLimit = rateLimitedUntil > now
  const credits = cache?.data.credits
  const creditsText = credits?.unlimited
    ? 'Credits: unlimited'
    : credits?.has_credits && credits.balance
      ? `Credits: ${credits.balance}`
      : ''

  return (
    <div
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        padding: '8px 12px 10px',
        borderTop: '1px solid var(--border)',
        fontFamily: 'var(--font-ui)',
        userSelect: 'none',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <span style={{ fontSize: 11, fontWeight: 600, color: CODEX_GREEN }}>Codex Usage</span>
        <div style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
          {cache?.data.plan_type && (
            <span style={{ fontSize: 9, color: 'var(--text-dim)' }}>{planLabel(cache.data.plan_type)}</span>
          )}
          {loading && (
            <svg
              style={{ animation: 'spin 1s linear infinite' }}
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
            >
              <path d="M21 12a9 9 0 1 1-6.219-8.56" />
            </svg>
          )}
        </div>
      </div>

      {limits.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {limits.map(limit => {
            const percent = Math.min(100, Math.max(0, Math.round(limit.percent)))
            const color = barColor(limit)
            const reset = formatReset(limit.resetAt, now)
            return (
              <div key={limit.key}>
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'baseline',
                    justifyContent: 'space-between',
                    marginBottom: 4,
                  }}
                >
                  <span style={{ fontSize: 11, color: 'var(--text)' }}>{limit.label}</span>
                  <span style={{ fontSize: 10, color: 'var(--text-dim)' }}>
                    <span style={{ color: percent >= 75 ? color : 'var(--text)' }}>{percent}% used</span>
                    {reset && (
                      <>
                        <span style={{ opacity: 0.5, margin: '0 4px' }}>·</span>
                        <span style={{ color: 'var(--text)', opacity: 0.9, fontVariantNumeric: 'tabular-nums' }}>
                          {reset}
                        </span>
                      </>
                    )}
                  </span>
                </div>
                <div
                  style={{
                    height: 4,
                    borderRadius: 2,
                    background: 'rgba(255,255,255,0.06)',
                    overflow: 'hidden',
                  }}
                >
                  <div
                    style={{
                      height: '100%',
                      width: `${percent}%`,
                      borderRadius: 2,
                      background: color,
                      transition: 'width 0.4s ease',
                    }}
                  />
                </div>
              </div>
            )
          })}
        </div>
      )}

      {creditsText && <div style={{ color: 'var(--text-dim)', fontSize: 10 }}>{creditsText}</div>}

      {showRateLimit && (
        <div style={{ padding: '2px 0', color: 'var(--text-dim)', fontSize: 10 }}>
          Rate limited — retrying in {formatDuration(rateLimitedUntil - now)}
        </div>
      )}

      {error && !showRateLimit && limits.length === 0 && (
        <div style={{ padding: '4px 0', color: '#f7768e', fontSize: 11, lineHeight: 1.4 }}>{error}</div>
      )}

      {!error && limits.length === 0 && !loading && !showRateLimit && (
        <div style={{ padding: '4px 0', color: 'var(--text-dim)', fontSize: 11 }}>No usage data available.</div>
      )}
    </div>
  )
}
