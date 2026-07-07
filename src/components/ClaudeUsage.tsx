import { useCallback, useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { claudeUsage, type ClaudeLimit } from '../lib/tauri'

const CACHE_KEY = 'commandeer:claude-usage'
const STATE_KEY = 'commandeer:claude-poll-state'

// The usage numbers move on the scale of hours (5h rolling session window, 7d
// weekly); the only time-sensitive value is the reset countdown, and that's
// computed locally each second from `resets_at`. So a multi-minute network
// floor costs no perceptible accuracy while keeping us well under the shared,
// undocumented per-OAuth-token rate limit (the /api/oauth/usage endpoint
// returns no budget headers on success — we only learn we're over on a 429).
const BASE_INTERVAL_MS = 5 * 60_000
const MAX_INTERVAL_MS = 30 * 60_000
// Small randomisation so our polls don't phase-lock with Claude Code's own
// usage polling on the same token (they share one rate-limit bucket).
const JITTER_MS = 15_000

// Only the overall session + weekly limits are shown. The per-model
// (weekly_scoped, e.g. Fable) limit is intentionally omitted.
const KIND_ORDER = ['session', 'weekly_all']
const CLAUDE_ORANGE = '#D97757'

// The Claude Code pixel-art mark, traced from the source PNG onto a 16×10 grid
// (each unit = one 40px module). Rendered inline in Claude orange so it needs no
// network/brand asset. The eye notches are left unfilled (they show the panel
// background through), matching the original.
const CLAUDE_LOGO_RECTS: Array<[number, number, number, number]> = [
  [2, 0, 12, 2], // head top
  [2, 2, 2, 2],  // eye row — left of left eye
  [5, 2, 6, 2],  // eye row — between eyes
  [12, 2, 2, 2], // eye row — right of right eye
  [0, 4, 16, 2], // arms (full width)
  [2, 6, 12, 2], // lower body
  [3, 8, 1, 2],  // legs
  [5, 8, 1, 2],
  [10, 8, 1, 2],
  [12, 8, 1, 2],
]

function ClaudeLogo({ height = 14 }: { height?: number }) {
  return (
    <svg
      width={height * 1.6}
      height={height}
      viewBox="0 0 16 10"
      shapeRendering="crispEdges"
      role="img"
      aria-label="Claude Code"
    >
      <title>Claude Code</title>
      {CLAUDE_LOGO_RECTS.map(([x, y, w, h], i) => (
        <rect key={i} x={x} y={y} width={w} height={h} fill={CLAUDE_ORANGE} />
      ))}
    </svg>
  )
}

interface CachedUsage {
  limits: ClaudeLimit[]
  fetchedAt: number
}

// Adaptive poll state, persisted so it survives palette hide/show remounts.
// `interval` is the current spacing between fetches — it doubles on every 429
// and halves back toward the base on success, auto-tuning to whatever the
// shared bucket tolerates. `nextAllowedAt` is the earliest we may fetch again.
interface PollState {
  interval: number
  nextAllowedAt: number
}

function clampInterval(ms: number): number {
  return Math.min(MAX_INTERVAL_MS, Math.max(BASE_INTERVAL_MS, ms))
}

function limitLabel(limit: ClaudeLimit): string {
  if (limit.kind === 'session') return 'Current session'
  if (limit.kind === 'weekly_all') return 'Current week (all models)'
  const model = limit.scope?.model?.display_name
  return model ? `Current week (${model})` : 'Current week'
}

// Fixed traffic-light palette (blue → yellow → red) so usage reads the same
// in every theme, rather than following the accent color.
function barColor(limit: ClaudeLimit): string {
  if (limit.severity === 'error' || limit.severity === 'exceeded' || limit.percent >= 90) return '#f7484f'
  if (limit.severity === 'warning' || limit.percent >= 75) return '#f5c542'
  return '#4a9eff'
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
    /* fall through to default */
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

function formatDuration(ms: number): string {
  const totalSeconds = Math.max(0, Math.ceil(ms / 1000))
  const minutes = Math.floor(totalSeconds / 60)
  const seconds = totalSeconds % 60
  if (minutes > 0) return `${minutes}m ${seconds}s`
  return `${seconds}s`
}

export default function ClaudeUsage() {
  const cacheRef = useRef<CachedUsage | null>(loadCached())
  const stateRef = useRef<PollState>(loadState())
  // Coalesces concurrent refresh() calls (rapid focus events, StrictMode, a
  // reopen mid-request) onto a single in-flight request — no request bursts.
  const inFlightRef = useRef<Promise<void> | null>(null)

  const [cache, setCache] = useState<CachedUsage | null>(cacheRef.current)
  const [loading, setLoading] = useState(
    !cacheRef.current && Date.now() >= stateRef.current.nextAllowedAt,
  )
  const [error, setError] = useState<string | null>(null)
  const [rateLimitedUntil, setRateLimitedUntil] = useState<number>(0)
  const [now, setNow] = useState<number>(Date.now())
  const limits = cache?.limits ?? null

  // Tick every second so the "resets in..." and "retrying in..." countdowns update.
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [])

  const refresh = useCallback((force = false): Promise<void> => {
    // A request is already running — reuse it rather than firing another.
    if (inFlightRef.current) return inFlightRef.current
    // Respect the adaptive spacing / backoff window unless explicitly forced.
    if (!force && Date.now() < stateRef.current.nextAllowedAt) {
      setLoading(false)
      return Promise.resolve()
    }

    setLoading(true)
    setError(null)

    const run = (async () => {
      try {
        const data = await claudeUsage()
        const sorted = (data.limits ?? [])
          .filter(l => KIND_ORDER.includes(l.kind))
          .sort((a, b) => KIND_ORDER.indexOf(a.kind) - KIND_ORDER.indexOf(b.kind))
        const next = { limits: sorted, fetchedAt: Date.now() }
        cacheRef.current = next
        setCache(next)
        try {
          localStorage.setItem(CACHE_KEY, JSON.stringify(next))
        } catch {
          /* ignore storage failures */
        }

        // Success: decay the interval back toward the base and hold off until
        // it elapses (+ jitter so we don't re-sync with Claude Code's polling).
        const interval = clampInterval(stateRef.current.interval / 2)
        const jitter = Math.floor(Math.random() * JITTER_MS)
        stateRef.current = { interval, nextAllowedAt: Date.now() + interval + jitter }
        saveState(stateRef.current)
        setRateLimitedUntil(0)
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        const retrySeconds = parseRateLimitSeconds(message)
        if (retrySeconds !== null) {
          // Rate limited: escalate hard. Wait at least the server's retry-after,
          // and at least double our current interval, so repeated limits ramp
          // the spacing up until we fall under the shared budget.
          const interval = clampInterval(
            Math.max(retrySeconds * 1000, stateRef.current.interval * 2),
          )
          const nextAllowedAt = Date.now() + interval
          stateRef.current = { interval, nextAllowedAt }
          saveState(stateRef.current)
          setRateLimitedUntil(nextAllowedAt)
        } else {
          // Transient (network) error: brief cooldown, keep the interval as-is.
          stateRef.current = {
            ...stateRef.current,
            nextAllowedAt: Date.now() + BASE_INTERVAL_MS,
          }
          saveState(stateRef.current)
        }
        setError(message)
        console.error('claude usage:', err)
        // Keep showing stale cached data instead of wiping it on error.
      } finally {
        setLoading(false)
        inFlightRef.current = null
      }
    })()

    inFlightRef.current = run
    return run
  }, [])

  useEffect(() => {
    refresh()
    const unlisten = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) refresh()
    })
    return () => { unlisten.then(fn => fn()) }
  }, [refresh])

  const showRateLimit = rateLimitedUntil > now
  const hasLimits = !!limits && limits.length > 0

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
        <ClaudeLogo height={14} />
        {loading && (
          <svg style={{ animation: 'spin 1s linear infinite' }} width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M21 12a9 9 0 1 1-6.219-8.56" />
          </svg>
        )}
      </div>

      {/* Show the last-known bars whenever we have them — even while rate
          limited or erroring — so the widget stays as informative as possible. */}
      {hasLimits && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {limits!.map(limit => {
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

      {/* Backoff countdown — subtle when we still have bars to show, since it's
          just informational; the stale numbers above remain useful. */}
      {showRateLimit && (
        <div style={{
          padding: '2px 0',
          color: 'var(--text-dim)',
          fontSize: 10,
        }}>
          Rate limited — retrying in {formatDuration(rateLimitedUntil - now)}
        </div>
      )}

      {/* Hard error with nothing cached to fall back on. */}
      {error && !showRateLimit && !hasLimits && (
        <div style={{
          padding: '4px 0',
          color: '#f7768e',
          fontSize: 11,
          lineHeight: 1.4,
        }}>
          {error}
        </div>
      )}

      {!error && !hasLimits && !loading && !showRateLimit && (
        <div style={{
          padding: '4px 0',
          color: 'var(--text-dim)',
          fontSize: 11,
        }}>
          No usage data available.
        </div>
      )}
    </div>
  )
}
