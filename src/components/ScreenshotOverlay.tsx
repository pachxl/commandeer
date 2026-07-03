// Fullscreen "frozen frame" rendered in the dedicated `screenshot` window
// (Lightshot-style): the captured screen under a dim veil, drag to select a
// region shown at full brightness with a live W×H badge. Mouse-up finishes
// (copy + save on the Rust side), Esc cancels. The window is reused across
// captures — all selection state resets when a new frame arrives.
import { useCallback, useEffect, useRef, useState } from 'react'
import { convertFileSrc } from '@tauri-apps/api/core'
import {
  cancelScreenshot,
  finishScreenshot,
  onScreenshotFrame,
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

const VEIL = 'rgba(0, 0, 0, 0.45)'
// Drags smaller than this (CSS px) are treated as a stray click, not a snip.
const MIN_DRAG = 4

export default function ScreenshotOverlay() {
  const [frame, setFrame] = useState<Frame | null>(null)
  const [drag, setDrag] = useState<Drag | null>(null)
  // Guards against a double mouse-up racing two finish invokes.
  const finishing = useRef(false)

  useEffect(() => {
    const unlisten = onScreenshotFrame(f => {
      finishing.current = false
      setDrag(null)
      // The frame path is reused every capture, so bust the webview's cache.
      setFrame({ ...f, src: `${convertFileSrc(f.path)}?v=${Date.now()}` })
    })
    return () => {
      unlisten.then(fn => fn())
    }
  }, [])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setDrag(null)
        setFrame(null)
        void cancelScreenshot()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  const onMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return
    setDrag({ x0: e.clientX, y0: e.clientY, x1: e.clientX, y1: e.clientY })
  }, [])

  const onMouseMove = useCallback((e: React.MouseEvent) => {
    setDrag(d => (d ? { ...d, x1: e.clientX, y1: e.clientY } : d))
  }, [])

  const onMouseUp = useCallback(
    (e: React.MouseEvent) => {
      if (e.button !== 0 || !drag || !frame || finishing.current) return
      const left = Math.min(drag.x0, drag.x1)
      const top = Math.min(drag.y0, drag.y1)
      const w = Math.abs(drag.x1 - drag.x0)
      const h = Math.abs(drag.y1 - drag.y0)
      if (w < MIN_DRAG || h < MIN_DRAG) {
        setDrag(null)
        return
      }
      // CSS px → frame-image px. The overlay covers exactly the captured
      // area, so a uniform scale is correct regardless of display scaling.
      const scaleX = frame.width / window.innerWidth
      const scaleY = frame.height / window.innerHeight
      finishing.current = true
      setDrag(null)
      setFrame(null)
      void finishScreenshot({
        x: Math.round(left * scaleX),
        y: Math.round(top * scaleY),
        w: Math.max(1, Math.round(w * scaleX)),
        h: Math.max(1, Math.round(h * scaleY)),
      }).catch(err => console.error('finish_screenshot failed:', err))
    },
    [drag, frame]
  )

  const sel = drag
    ? {
        left: Math.min(drag.x0, drag.x1),
        top: Math.min(drag.y0, drag.y1),
        width: Math.abs(drag.x1 - drag.x0),
        height: Math.abs(drag.y1 - drag.y0),
      }
    : null

  const scaleX = frame ? frame.width / window.innerWidth : 1
  const scaleY = frame ? frame.height / window.innerHeight : 1
  // Badge above the selection unless it would leave the screen.
  const badgeAbove = sel ? sel.top > 28 : true

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        overflow: 'hidden',
        cursor: 'crosshair',
        userSelect: 'none',
        background: '#000',
      }}
      onMouseDown={onMouseDown}
      onMouseMove={onMouseMove}
      onMouseUp={onMouseUp}
    >
      {frame && (
        <img
          src={frame.src}
          onLoad={() => void showScreenshotOverlay()}
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
      {sel ? (
        <div
          style={{
            position: 'absolute',
            left: sel.left,
            top: sel.top,
            width: sel.width,
            height: sel.height,
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
            {Math.max(1, Math.round(sel.width * scaleX))} × {Math.max(1, Math.round(sel.height * scaleY))}
          </div>
        </div>
      ) : (
        <div style={{ position: 'absolute', inset: 0, background: VEIL, pointerEvents: 'none' }} />
      )}
    </div>
  )
}
