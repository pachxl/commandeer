// Fullscreen "frozen frame" rendered in the dedicated `screenshot` window
// (Lightshot-style): the captured screen under a dim veil, drag to select a
// region shown at full brightness with a live W×H badge. Releasing the drag
// enters an annotate stage: further drags paint freehand red marker strokes
// (circle things with the mouse), Ctrl+Z (or Backspace) undoes a stroke, and
// Enter / the ✓ button finishes (copy + save with the strokes burned in on
// the Rust side). Holding Alt in the annotate stage shows a color-pick
// tooltip sampling the raw frame under the cursor; Alt+click copies that
// color (hex) instead of the image and finishes — the crop is still saved.
// Esc cancels. The window is reused across captures — all state resets when
// a new frame arrives.
import { useCallback, useEffect, useRef, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import {
  cancelScreenshot,
  finishScreenshot,
  hideScreenshotOverlay,
  IS_LINUX,
  IS_MAC,
  onScreenshotClear,
  onScreenshotFrame,
  pickFrameColor,
  revealScreenshotOverlay,
  showScreenshotOverlay,
  type ScreenshotFrame,
} from '../lib/tauri'

interface Frame extends ScreenshotFrame {
  src: string
}

interface Drag {
  x0: number
  y0: number
  x1: number
  y1: number
}

interface Rect {
  left: number
  top: number
  width: number
  height: number
}

interface Point {
  x: number
  y: number
}

const normalize = (d: Drag): Rect => ({
  left: Math.min(d.x0, d.x1),
  top: Math.min(d.y0, d.y1),
  width: Math.abs(d.x1 - d.x0),
  height: Math.abs(d.y1 - d.y0),
})

const pathExtent = (path: Point[]): number => {
  let minX = Infinity,
    minY = Infinity,
    maxX = -Infinity,
    maxY = -Infinity
  for (const p of path) {
    minX = Math.min(minX, p.x)
    minY = Math.min(minY, p.y)
    maxX = Math.max(maxX, p.x)
    maxY = Math.max(maxY, p.y)
  }
  return Math.max(maxX - minX, maxY - minY)
}

const VEIL = 'rgba(0, 0, 0, 0.45)'
// Drags smaller than this (CSS px) are treated as a stray click, not a snip.
const MIN_DRAG = 4
// Marker stroke width (CSS px; scaled to frame px on finish).
const STROKE = 3
const RED = '#ff3b30'

const buttonStyle: React.CSSProperties = {
  border: 'none',
  borderRadius: 4,
  padding: '3px 10px',
  background: 'rgba(255, 255, 255, 0.15)',
  color: '#fff',
  font: 'inherit',
  cursor: 'pointer',
  whiteSpace: 'nowrap',
}

// Linux: resolve after the cleared (frame-less, fully transparent) state has
// actually been composited — WebKitGTK replays the last composite as the first
// frame at the next map, so hiding before this paint would flash the previous
// capture the next time the overlay shows. Windows solves the same problem
// with DWM cloaking instead, so there the wait would only add latency.
const afterClearPaint = () =>
  IS_LINUX
    ? new Promise<void>(resolve => {
        requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
      })
    : Promise.resolve()

export default function ScreenshotOverlay() {
  const [frame, setFrame] = useState<Frame | null>(null)
  const [finishError, setFinishError] = useState<string | null>(null)
  const [retryCopyColor, setRetryCopyColor] = useState<string | null>(null)
  // Live selection drag (only before `sel` commits).
  const [drag, setDrag] = useState<Drag | null>(null)
  // Committed selection — non-null means the annotate stage.
  const [sel, setSel] = useState<Rect | null>(null)
  // Committed marker strokes plus the one being painted right now.
  const [paths, setPaths] = useState<Point[][]>([])
  const [livePath, setLivePath] = useState<Point[] | null>(null)
  // Alt color picker (annotate stage): tooltip visibility, its anchor, and
  // the sampled '#RRGGBB' under the cursor.
  const [altHeld, setAltHeld] = useState(false)
  const [cursor, setCursor] = useState<Point | null>(null)
  const [pickColor, setPickColor] = useState<string | null>(null)
  // Last mouse position (no re-render), so the tooltip can appear on Alt
  // keydown before the mouse moves again.
  const cursorRef = useRef<Point | null>(null)
  // Drop hover samples while one invoke is in flight.
  const sampling = useRef(false)
  // Guards against a double mouse-up racing two finish invokes.
  const finishing = useRef(false)
  // Invalidates a delayed finish/cancel when a newer capture arrives.
  const captureGeneration = useRef(0)

  const reset = () => {
    setDrag(null)
    setSel(null)
    setPaths([])
    setLivePath(null)
    setAltHeld(false)
    setCursor(null)
    setPickColor(null)
    setFinishError(null)
    setRetryCopyColor(null)
  }

  useEffect(() => {
    const unlisten = onScreenshotFrame(f => {
      captureGeneration.current++
      finishing.current = false
      reset()
      // The frame path is reused every capture, so bust the webview's cache.
      setFrame({ ...f, src: `${convertFileSrc(f.path)}?v=${Date.now()}` })
    })
    return () => {
      unlisten.then(fn => fn())
    }
  }, [])

  useEffect(() => {
    // Reveal (uncloak) only once the frame <img> has actually been PRESENTED:
    // element-timing entries are queued after the paint reaches the screen,
    // unlike onLoad/rAF, which race the GPU rasterization of the huge frame
    // texture — revealing on those flashed black for a few frames. Each
    // capture is a new image resource (cache-busted src), so a fresh entry
    // fires every time.
    let po: PerformanceObserver | null = null
    try {
      po = new PerformanceObserver(() => {
        requestAnimationFrame(() => void revealScreenshotOverlay())
      })
      po.observe({ type: 'element', buffered: true } as PerformanceObserverInit)
    } catch {
      // No element timing support: the onLoad timer / Rust fallback reveal.
    }
    return () => po?.disconnect()
  }, [])

  // With copyColor set (Alt+click pick), the color text is copied instead of
  // the image; the annotated crop is saved to disk either way.
  const finish = useCallback(
    (copyColor?: string) => {
      if (!sel || !frame || finishing.current) return
      const generation = captureGeneration.current
      // CSS px → frame-image px. The overlay covers exactly the captured
      // area, so a uniform scale is correct regardless of display scaling.
      const scaleX = frame.width / window.innerWidth
      const scaleY = frame.height / window.innerHeight
      finishing.current = true
      const region = {
        x: Math.round(sel.left * scaleX),
        y: Math.round(sel.top * scaleY),
        w: Math.max(1, Math.round(sel.width * scaleX)),
        h: Math.max(1, Math.round(sel.height * scaleY)),
      }
      const annotations = paths.map(path => ({
        points: path.map(p => [p.x * scaleX, p.y * scaleY] as [number, number]),
        stroke: (STROKE * (scaleX + scaleY)) / 2,
      }))
      reset()
      setFrame(null)
      void afterClearPaint()
        .then(async () => {
          if (captureGeneration.current !== generation) return
          await finishScreenshot(region, annotations, copyColor)
        })
        .catch(async err => {
          if (captureGeneration.current !== generation) return
          console.error('finish_screenshot failed:', err)
          // The backend retains the pending capture on failure. Restore the
          // annotate state so the user sees the error and can retry or cancel.
          finishing.current = false
          setFrame(frame)
          setSel(sel)
          setPaths(paths)
          setFinishError(`Screenshot failed: ${String(err)}`)
          setRetryCopyColor(copyColor ?? null)
          await new Promise<void>(resolve => requestAnimationFrame(() => resolve()))
          try {
            await showScreenshotOverlay()
            window.setTimeout(() => {
              void revealScreenshotOverlay().catch(console.error)
            }, 500)
          } catch (showError) {
            console.error('show_screenshot_overlay failed:', showError)
          }
        })
    },
    [sel, paths, frame],
  )

  // Sample the raw frame pixel under a CSS-px cursor position for the Alt
  // color-pick tooltip. In-flight throttled; the next mousemove resamples.
  const sampleAt = useCallback(
    (cx: number, cy: number) => {
      if (!frame || sampling.current) return
      sampling.current = true
      const x = Math.max(0, Math.round(cx * (frame.width / window.innerWidth)))
      const y = Math.max(0, Math.round(cy * (frame.height / window.innerHeight)))
      pickFrameColor(Math.min(x, frame.width - 1), Math.min(y, frame.height - 1))
        .then(setPickColor)
        .catch(() => {})
        .finally(() => {
          sampling.current = false
        })
    },
    [frame],
  )

  useEffect(() => {
    // Alt color picker, annotate stage only: hold Alt for a live tooltip of
    // the pixel color under the cursor (Alt+click picks it — see onMouseDown).
    if (!sel || !frame) return
    const down = (e: KeyboardEvent) => {
      if (e.key !== 'Alt') return
      // Keep the webview from treating Alt as a menu/system key.
      e.preventDefault()
      if (e.repeat) return
      setAltHeld(true)
      if (cursorRef.current) {
        setCursor(cursorRef.current)
        sampleAt(cursorRef.current.x, cursorRef.current.y)
      }
    }
    const up = (e: KeyboardEvent) => {
      if (e.key === 'Alt') setAltHeld(false)
    }
    const blur = () => setAltHeld(false)
    window.addEventListener('keydown', down)
    window.addEventListener('keyup', up)
    window.addEventListener('blur', blur)
    return () => {
      window.removeEventListener('keydown', down)
      window.removeEventListener('keyup', up)
      window.removeEventListener('blur', blur)
    }
  }, [sel, frame, sampleAt])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (!frame || finishing.current) return
        const generation = captureGeneration.current
        finishing.current = true
        reset()
        setFrame(null)
        void afterClearPaint()
          .then(() => {
            if (captureGeneration.current !== generation) return
            return cancelScreenshot()
          })
          .catch(err => console.error('cancel_screenshot failed:', err))
          .finally(() => {
            if (captureGeneration.current === generation) finishing.current = false
          })
      } else if (e.key === 'Enter') {
        finish(retryCopyColor ?? undefined)
      } else if ((e.key === 'z' && (e.ctrlKey || e.metaKey)) || e.key === 'Backspace') {
        setLivePath(null)
        setPaths(p => p.slice(0, -1))
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [finish, frame, retryCopyColor])

  useEffect(() => {
    // Linux re-trigger: Rust asks the still-visible overlay to clear before it
    // captures a fresh frame; we hide ourselves once the clear has painted
    // (Rust force-hides after its pre-capture delay as a fallback).
    const unlisten = onScreenshotClear(() => {
      captureGeneration.current++
      finishing.current = false
      reset()
      setFrame(null)
      void afterClearPaint().then(() => hideScreenshotOverlay())
    })
    return () => {
      unlisten.then(fn => fn())
    }
  }, [])

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (e.button !== 0) return
      if (!sel) {
        setDrag({ x0: e.clientX, y0: e.clientY, x1: e.clientX, y1: e.clientY })
      } else if (e.altKey) {
        // Alt+click color pick: sample the exact click pixel (not the possibly
        // in-flight tooltip value), copy it, and finish without the image copy.
        if (!frame || finishing.current) return
        const x = Math.min(frame.width - 1, Math.max(0, Math.round(e.clientX * (frame.width / window.innerWidth))))
        const y = Math.min(frame.height - 1, Math.max(0, Math.round(e.clientY * (frame.height / window.innerHeight))))
        void pickFrameColor(x, y)
          .then(color => finish(color))
          .catch(err => console.error('pick_frame_color failed:', err))
      } else {
        setLivePath([{ x: e.clientX, y: e.clientY }])
      }
    },
    [sel, frame, finish],
  )

  const onMouseMove = useCallback(
    (e: React.MouseEvent) => {
      cursorRef.current = { x: e.clientX, y: e.clientY }
      if (sel) {
        // Track from the modifier flag too, so a keyup missed while the
        // window lacked focus can't leave a stuck tooltip.
        setAltHeld(e.altKey)
        if (e.altKey) {
          setCursor(cursorRef.current)
          sampleAt(e.clientX, e.clientY)
        }
      }
      setDrag(d => (d ? { ...d, x1: e.clientX, y1: e.clientY } : d))
      setLivePath(path => {
        if (!path) return path
        // Thin the path: skip sub-2px jitters so strokes stay small.
        const last = path[path.length - 1]
        if (Math.hypot(e.clientX - last.x, e.clientY - last.y) < 2) return path
        return [...path, { x: e.clientX, y: e.clientY }]
      })
    },
    [sel, sampleAt],
  )

  const onMouseUp = useCallback(
    (e: React.MouseEvent) => {
      if (e.button !== 0 || !frame || finishing.current) return
      if (!sel) {
        if (!drag) return
        const rect = normalize(drag)
        setDrag(null)
        if (rect.width < MIN_DRAG || rect.height < MIN_DRAG) return
        setSel(rect)
      } else {
        if (!livePath) return
        setLivePath(null)
        // A stroke needs actual movement; anything smaller is a stray click.
        if (livePath.length < 2 || pathExtent(livePath) < MIN_DRAG) return
        setPaths(p => [...p, livePath])
      }
    },
    [drag, livePath, frame, sel],
  )

  // The selection box: committed one in the annotate stage, live drag before.
  const box = sel ?? (drag ? normalize(drag) : null)
  const allPaths = livePath && livePath.length > 1 ? [...paths, livePath] : paths

  const scaleX = frame ? frame.width / window.innerWidth : 1
  const scaleY = frame ? frame.height / window.innerHeight : 1
  // Badge above the selection unless it would leave the screen.
  const badgeAbove = box ? box.top > 28 : true
  const toolbarTop = sel ? Math.min(sel.top + sel.height + 8, window.innerHeight - 40) : 0
  const toolbarLeft = sel ? Math.max(8, Math.min(sel.left, window.innerWidth - 430)) : 0

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        overflow: 'hidden',
        cursor: 'crosshair',
        userSelect: 'none',
        // Linux: the window is transparent and the cleared state must composite
        // as fully invisible (see afterClearPaint); an opaque backdrop would
        // turn the pre-paint frame at map time into a black flash instead.
        background: IS_LINUX ? 'transparent' : '#000',
      }}
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={onMouseUp}
    >
      {frame && (
        <img
          src={frame.src}
          // Marks the frame for the element-timing observer above.
          {...({ elementtiming: 'shot-frame' } as Record<string, string>)}
          onLoad={() => {
            // The window was already shown cloaked at capture time (Windows) —
            // this is a no-op there and the show path on Linux. The timer is a
            // reveal fallback in case the element-timing entry never fires.
            void showScreenshotOverlay()
            setTimeout(() => void revealScreenshotOverlay(), 500)
          }}
          draggable={false}
          style={{
            position: 'absolute',
            inset: 0,
            width: '100%',
            height: '100%',
            display: 'block',
          }}
        />
      )}
      {finishError && frame && (
        <div
          style={{
            position: 'absolute',
            left: '50%',
            top: 24,
            transform: 'translateX(-50%)',
            zIndex: 10,
            maxWidth: 'min(640px, calc(100vw - 48px))',
            padding: '8px 14px',
            borderRadius: 6,
            background: 'rgba(160, 24, 24, 0.94)',
            color: '#fff',
            font: '13px/1.4 system-ui, sans-serif',
            boxShadow: '0 4px 18px rgba(0, 0, 0, 0.35)',
            pointerEvents: 'none',
          }}
        >
          {finishError} — press Enter to retry or Esc to cancel.
        </div>
      )}
      {box ? (
        <div
          style={{
            position: 'absolute',
            left: box.left,
            top: box.top,
            width: box.width,
            height: box.height,
            // The veil is the shadow: everything outside the selection dims,
            // the selection itself stays at full brightness.
            boxShadow: `0 0 0 100000px ${VEIL}`,
            outline: '1px solid rgba(255, 255, 255, 0.85)',
            pointerEvents: 'none',
          }}
        >
          <div
            style={{
              position: 'absolute',
              left: 0,
              top: badgeAbove ? -26 : 6,
              padding: '2px 8px',
              borderRadius: 4,
              background: 'rgba(0, 0, 0, 0.75)',
              color: '#fff',
              font: '12px/1.5 system-ui, sans-serif',
              whiteSpace: 'nowrap',
            }}
          >
            {Math.max(1, Math.round(box.width * scaleX))} × {Math.max(1, Math.round(box.height * scaleY))}
          </div>
        </div>
      ) : frame ? (
        // Full-screen veil only while a frame is up: the cleared state (frame
        // null) must render nothing so it composites fully transparent.
        <div style={{ position: 'absolute', inset: 0, background: VEIL, pointerEvents: 'none' }} />
      ) : null}
      {sel && allPaths.length > 0 && (
        // Strokes clipped to the selection, so the preview matches what the
        // Rust side draws into the cropped image.
        <svg
          width={sel.width}
          height={sel.height}
          style={{
            position: 'absolute',
            left: sel.left,
            top: sel.top,
            overflow: 'hidden',
            pointerEvents: 'none',
          }}
        >
          {allPaths.map((path, i) => (
            <polyline
              key={i}
              points={path.map(p => `${p.x - sel.left},${p.y - sel.top}`).join(' ')}
              fill="none"
              stroke={RED}
              strokeWidth={STROKE}
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          ))}
        </svg>
      )}
      {sel && (
        <div
          // Keep toolbar clicks from starting a stroke underneath.
          onMouseDown={e => e.stopPropagation()}
          onMouseUp={e => e.stopPropagation()}
          style={{
            position: 'absolute',
            left: toolbarLeft,
            top: toolbarTop,
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            padding: '4px 6px',
            borderRadius: 6,
            background: 'rgba(0, 0, 0, 0.75)',
            color: '#fff',
            font: '12px/1.5 system-ui, sans-serif',
            cursor: 'default',
          }}
        >
          <button
            onClick={() => setPaths(p => p.slice(0, -1))}
            disabled={paths.length === 0}
            style={{ ...buttonStyle, opacity: paths.length === 0 ? 0.4 : 1 }}
          >
            ↩ Undo
          </button>
          <button onClick={() => finish()} style={{ ...buttonStyle, background: RED }}>
            ✓ Copy
          </button>
          <span style={{ color: 'rgba(255, 255, 255, 0.55)', whiteSpace: 'nowrap', padding: '0 4px' }}>
            draw with the mouse · Enter to copy · hold {IS_MAC ? '⌥' : 'Alt'} to pick a colour
          </span>
        </div>
      )}
      {sel && altHeld && cursor && pickColor && (
        // Alt color-pick tooltip: swatch + hex of the raw frame pixel under
        // the cursor, kept just inside the screen edges.
        <div
          style={{
            position: 'absolute',
            left: Math.min(cursor.x + 14, window.innerWidth - 104),
            top: Math.min(cursor.y + 18, window.innerHeight - 32),
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            padding: '2px 8px',
            borderRadius: 4,
            background: 'rgba(0, 0, 0, 0.8)',
            color: '#fff',
            font: '12px/1.5 system-ui, sans-serif',
            whiteSpace: 'nowrap',
            pointerEvents: 'none',
          }}
        >
          <span
            style={{
              width: 12,
              height: 12,
              borderRadius: 2,
              background: pickColor,
              boxShadow: 'inset 0 0 0 1px rgba(255, 255, 255, 0.45)',
            }}
          />
          {pickColor}
        </div>
      )}
    </div>
  )
}
