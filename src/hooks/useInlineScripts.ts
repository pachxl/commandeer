// Live-refreshing inline scripts for the command palette.
//
// Extracted from Palette.tsx: seeds and polls each inline script on its
// interval while the palette is focused, capturing stdout keyed by path. The
// captured output overlays the row's sublabel at render time (see displayItems
// in Palette) — kept out of the reducer so a changing output never re-ranks
// the list mid-tick.

import { useCallback, useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { runScriptCapture } from '../lib/tauri'

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
  const [windowFocused, setWindowFocused] = useState(true)
  const inlineTimersRef = useRef<number[]>([])

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

  // Pause polling while the palette is hidden (focus loss auto-hides it) so we
  // don't keep running user scripts in the background.
  useEffect(() => {
    const unlistenPromise = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
        setWindowFocused(focused)
    })
    return () => { void unlistenPromise.then(unlisten => unlisten()) }
  }, [])

  // Seed + poll each inline script on its @vicinae.refreshTime interval. Only
  // runs while focused; re-seeds on re-focus.
  useEffect(() => {
    inlineTimersRef.current.forEach(clearInterval)
    inlineTimersRef.current = []
    if (!windowFocused) return
    for (const s of inlineScripts) {
      void refreshInline(s.path)
      const id = window.setInterval(() => { void refreshInline(s.path) }, Math.max(1, s.refreshSeconds) * 1000)
      inlineTimersRef.current.push(id)
    }
    return () => { inlineTimersRef.current.forEach(clearInterval); inlineTimersRef.current = [] }
  }, [inlineScripts, windowFocused, refreshInline])

  return { inlineOutputs, refreshInline }
}
