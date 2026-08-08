// Minimal task-manager widget: CPU / RAM / GPU utilization, polled while the
// palette is focused. Rendered below the Claude usage panel; toggled in
// Settings.
import { useEffect, useState } from 'react'
import { useWindowFocused } from '../hooks/useWindowFocused'
import { IS_MAC, systemStats, type SystemStats } from '../lib/tauri'

// The backend poll is a few syscalls (~µs); 1s matches Task Manager's cadence
const POLL_MS = 1000

export type SystemStatLevel = 'healthy' | 'warning' | 'critical'

const SYSTEM_STAT_COLORS: Record<SystemStatLevel, string> = {
  healthy: '#30d158',
  warning: '#ff9f0a',
  critical: '#ff453a',
}

export function systemStatLevel(percent: number): SystemStatLevel {
  if (percent >= 90) return 'critical'
  if (percent >= 75) return 'warning'
  return 'healthy'
}

export function systemStatColor(percent: number): string {
  return SYSTEM_STAT_COLORS[systemStatLevel(percent)]
}

function gb(bytes: number): string {
  return (bytes / 1024 ** 3).toFixed(1)
}

function StatCell({ label, percent, detail }: { label: string; percent: number | null; detail?: string }) {
  const pct = percent === null || !Number.isFinite(percent) ? null : Math.min(100, Math.max(0, Math.round(percent)))
  const level = pct === null ? 'pending' : systemStatLevel(pct)
  const color = systemStatColor(pct ?? 0)
  return (
    <div data-stat-cell={label.toLowerCase()} data-stat-level={level} style={{ flex: 1, minWidth: 0 }}>
      <div
        data-stat-header
        style={{
          display: 'flex',
          alignItems: 'baseline',
          justifyContent: 'space-between',
          marginBottom: 4,
        }}
      >
        <span style={{ fontSize: 11, color: 'var(--text)' }}>{label}</span>
        <span style={{ fontSize: 10, color: 'var(--text-dim)', fontVariantNumeric: 'tabular-nums' }}>
          {detail && (
            <>
              <span>{detail}</span>
              <span style={{ opacity: 0.5, margin: '0 4px' }}>·</span>
            </>
          )}
          <span
            data-stat-value
            style={{
              color: pct !== null && pct >= 75 ? color : 'var(--text)',
              transition: 'color 180ms ease',
            }}
          >
            {pct === null ? '—' : `${pct}%`}
          </span>
        </span>
      </div>
      <div
        data-stat-track
        role="progressbar"
        aria-label={`${label} utilization`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct ?? undefined}
        aria-valuetext={pct === null ? 'Waiting for data' : detail ? `${pct}%, ${detail}` : `${pct}%`}
        style={{
          height: 4,
          borderRadius: 2,
          background: 'rgba(255,255,255,0.06)',
          overflow: 'hidden',
        }}
      >
        <div
          data-stat-fill
          data-stat-level={level}
          style={{
            height: '100%',
            width: `${pct ?? 0}%`,
            borderRadius: 2,
            backgroundColor: color,
            transition: 'width 400ms cubic-bezier(0.2, 0.8, 0.2, 1), background-color 180ms ease',
          }}
        />
      </div>
    </div>
  )
}

export default function SystemStatsPanel() {
  const [stats, setStats] = useState<SystemStats | null>(null)
  const windowFocused = useWindowFocused()

  // Poll only while the palette is focused (it's hidden otherwise)
  useEffect(() => {
    if (!windowFocused) return
    let disposed = false

    const poll = () => {
      systemStats()
        .then(s => {
          if (!disposed) setStats(s)
        })
        .catch(console.error)
    }
    poll()
    const timer = setInterval(poll, POLL_MS)

    return () => {
      disposed = true
      clearInterval(timer)
    }
  }, [windowFocused])

  return (
    <div
      data-system-stats
      role="region"
      aria-label="System utilization"
      aria-busy={stats === null}
      style={{
        display: 'flex',
        gap: 16,
        padding: '8px 12px 10px',
        borderTop: '1px solid var(--border)',
        fontFamily: 'var(--font-ui)',
        userSelect: 'none',
      }}
    >
      <StatCell label="CPU" percent={stats ? stats.cpu : null} />
      <StatCell
        label="RAM"
        percent={stats ? stats.mem_percent : null}
        detail={stats && stats.mem_total > 0 ? `${gb(stats.mem_used)}/${gb(stats.mem_total)} GB` : undefined}
      />
      {/* No unprivileged cross-vendor GPU metric exists on macOS (the backend
          always returns null there), so drop the cell rather than render a
          permanently empty gauge. */}
      {!IS_MAC && <StatCell label="GPU" percent={stats ? stats.gpu : null} />}
    </div>
  )
}
