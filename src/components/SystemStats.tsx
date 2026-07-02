// Minimal task-manager widget: CPU / RAM / GPU utilization, polled while the
// palette is focused. Rendered below the Claude usage panel; toggled in
// Settings.
import { useEffect, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { systemStats, type SystemStats } from '../lib/tauri'

const POLL_MS = 1500

function barColor(pct: number): string {
  if (pct >= 90) return '#f7768e'
  if (pct >= 75) return '#e0af68'
  return 'var(--accent)'
}

function gb(bytes: number): string {
  return (bytes / 1024 ** 3).toFixed(1)
}

function StatCell({ label, percent, detail }: { label: string; percent: number | null; detail?: string }) {
  const pct = percent === null ? null : Math.min(100, Math.max(0, Math.round(percent)))
  return (
    <div style={{ flex: 1, minWidth: 0 }}>
      <div style={{
        display: 'flex',
        alignItems: 'baseline',
        justifyContent: 'space-between',
        marginBottom: 4,
      }}>
        <span style={{ fontSize: 11, color: 'var(--text)' }}>{label}</span>
        <span style={{ fontSize: 10, color: 'var(--text-dim)', fontVariantNumeric: 'tabular-nums' }}>
          {detail && <><span>{detail}</span><span style={{ opacity: 0.5, margin: '0 4px' }}>·</span></>}
          <span style={{ color: pct !== null && pct >= 75 ? barColor(pct) : 'var(--text)' }}>
            {pct === null ? '—' : `${pct}%`}
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
          width: `${pct ?? 0}%`,
          borderRadius: 2,
          background: barColor(pct ?? 0),
          transition: 'width 0.4s ease',
        }} />
      </div>
    </div>
  )
}

export default function SystemStatsPanel() {
  const [stats, setStats] = useState<SystemStats | null>(null)

  // Poll only while the palette is focused (it's hidden otherwise)
  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | undefined
    let disposed = false

    const poll = () => {
      systemStats().then(s => { if (!disposed) setStats(s) }).catch(console.error)
    }
    const start = () => {
      if (timer !== undefined) return
      poll()
      timer = setInterval(poll, POLL_MS)
    }
    const stop = () => {
      if (timer !== undefined) clearInterval(timer)
      timer = undefined
    }

    start()
    let unlisten: (() => void) | undefined
    getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      if (focused) start()
      else stop()
    }).then(fn => { unlisten = fn })

    return () => { disposed = true; stop(); unlisten?.() }
  }, [])

  return (
    <div style={{
      display: 'flex',
      gap: 16,
      padding: '8px 12px 10px',
      borderTop: '1px solid var(--border)',
      fontFamily: 'var(--font-ui)',
      userSelect: 'none',
    }}>
      <StatCell label="CPU" percent={stats ? stats.cpu : null} />
      <StatCell
        label="RAM"
        percent={stats ? stats.mem_percent : null}
        detail={stats && stats.mem_total > 0 ? `${gb(stats.mem_used)}/${gb(stats.mem_total)} GB` : undefined}
      />
      <StatCell label="GPU" percent={stats ? stats.gpu : null} />
    </div>
  )
}
