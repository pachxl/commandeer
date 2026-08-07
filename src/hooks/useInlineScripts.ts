// Live-refreshing inline scripts for the command palette.
//
// Extracted from Palette.tsx: seeds and polls each inline script on its
// interval while the palette is focused, capturing stdout keyed by path. The
// captured output overlays the row's sublabel at render time (see displayItems
// in Palette) — kept out of the reducer so a changing output never re-ranks
// the list mid-tick.

import { useCallback, useEffect, useState } from 'react'
import { runScriptCapture } from '../lib/tauri'
import { useWindowFocused } from './useWindowFocused'

// An inline script the palette polls on a timer: its captured stdout replaces
// the row's sublabel live (at render time, outside the ranked search text so
// refreshes never re-rank the list).
export interface InlineScript {
  path: string
  refreshSeconds: number
}

export interface UseInlineScripts {
  // Captured stdout keyed by script path (or "…" until the first refresh).
  inlineOutputs: Record<string, string>
  // Re-run a script and update its live output. Used by Enter on an inline row.
  refreshInline: (path: string) => Promise<void>
}

export function useInlineScripts(inlineScripts: InlineScript[]): UseInlineScripts {
  // Live-captured stdout for inline scripts, keyed by script path.
  const [inlineOutputs, setInlineOutputs] = useState<Record<string, string>>({})
  // Whether the palette window is focused — polling pauses while hidden so we
  // don't run user scripts in the background.
  const windowFocused = useWindowFocused()

  // Re-run an inline script and update its live sublabel. Used by the polling
  // timers and by Enter on an inline row (force-refresh). On error the
  // previous output is kept (first failure shows an ellipsis).
  const refreshInline = useCallback(async (path: string) => {
    try {
      const out = await runScriptCapture(path)
      setInlineOutputs(prev => (prev[path] === out ? prev : { ...prev, [path]: out }))
    } catch {
      setInlineOutputs(prev => (path in prev ? prev : { ...prev, [path]: '…' }))
    }
  }, [])

  // Seed + poll each inline script on its @vicinae.refreshTime interval. Only
  // runs while focused; re-seeds on re-focus.
  // Add random jitter to avoid thundering herd when multiple scripts poll simultaneously.
  useEffect(() => {
    if (!windowFocused) return
    const timers: number[] = []
    for (const s of inlineScripts) {
      const jitter = Math.random() * 500
      const interval = Math.max(1, s.refreshSeconds) * 1000
      // Seed once after a small jitter, then start the recurring timer from
      // that point so each focus session cannot execute the script twice.
      const initialTimeout = window.setTimeout(() => {
        void refreshInline(s.path)
        const intervalId = window.setInterval(() => {
          void refreshInline(s.path)
        }, interval)
        timers.push(intervalId)
      }, jitter)
      timers.push(initialTimeout)
    }
    return () => {
      timers.forEach(clearTimeout)
      timers.forEach(clearInterval)
    }
  }, [inlineScripts, windowFocused, refreshInline])

  return { inlineOutputs, refreshInline }
}
