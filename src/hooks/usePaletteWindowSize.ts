// Keep the palette window sized to its content.
//
// Extracted from Palette.tsx.
//
// Windows: a single setSize per height change — the window is positioned once
// per show (Rust side, top fixed at ~20% of the monitor), so resizes only move
// the bottom edge and typing stays smooth. A small dead-band skips sub-2px
// churn; user resizing is prevented by resizable: false in tauri.conf.json.
// Re-asserted on focus because a size set while hidden isn't always honoured.
//
// Linux/Wayland (cosmic-comp): the palette is a layer-shell surface whose size
// comes from the GTK size request, so resizes go through the backend's
// resize_palette (in-place, no flicker). Linux/X11 has no layer shell — the
// window is a normal toplevel positioned by the backend on show, so it uses the
// same setSize path as Windows.

import { useCallback, useEffect, useRef, type RefObject } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalSize } from '@tauri-apps/api/dpi'
import { IS_LINUX, envInfo, recenterPalette, resizePalette } from '../lib/tauri'

// Base (unscaled) logical widths of the palette window. The scale factor
// multiplies the active style's width and is applied as a CSS zoom on the
// content, so the whole palette grows/shrinks uniformly.
export const DEFAULT_PALETTE_WIDTH = 669
export const ONIX_PALETTE_WIDTH = 770

export interface UsePaletteWindowSize {
  // The unscaled wrapper whose measured height drives the window height.
  sizeRef: RefObject<HTMLDivElement>
  // The scaled palette container (focused on slider steps).
  containerRef: RefObject<HTMLDivElement>
}

export function usePaletteWindowSize(scale: number, baseWidth = DEFAULT_PALETTE_WIDTH): UsePaletteWindowSize {
  // sizeRef is the *unscaled* wrapper we measure; its height already includes
  // the inner zoom (the zoomed content lays out scaled in the wrapper), so it is
  // the final logical window height. Width is derived from the scale directly.
  const sizeRef = useRef<HTMLDivElement>(null)
  const containerRef = useRef<HTMLDivElement>(null)
  const scaleRef = useRef(scale)
  scaleRef.current = scale
  const baseWidthRef = useRef(baseWidth)
  baseWidthRef.current = baseWidth
  const lastHeightRef = useRef(0)
  const lastWidthRef = useRef(0)

  const applySize = useCallback(async () => {
    const el = sizeRef.current
    if (!el) return
    const h = Math.ceil(el.getBoundingClientRect().height)
    if (!h) return
    const w = Math.round(baseWidthRef.current * scaleRef.current)
    const widthChanged = w !== lastWidthRef.current
    // Skip only when nothing meaningful changed (small height churn while typing
    // is absorbed by the dead-band; a width change always goes through).
    if (!widthChanged && Math.abs(h - lastHeightRef.current) < 2) return
    lastHeightRef.current = h
    // Re-center only when the width actually changes (scale), not on the height
    // churn from typing. Guard against the first apply (no prior width yet).
    const shouldRecenter = widthChanged && lastWidthRef.current > 0
    lastWidthRef.current = w
    if (IS_LINUX && (await envInfo()).wayland) {
      await resizePalette(h, w)
      return
    }
    await getCurrentWindow().setSize(new LogicalSize(w, h))
    // setSize keeps the top-left corner fixed, so a width change grows the
    // window rightward and drifts off-center. Let Rust re-center on the current
    // monitor (authoritative, reads the live size — no frontend DPI/position
    // races), matching the show-time centering.
    if (shouldRecenter) {
      await recenterPalette()
    }
  }, [])

  // Re-apply the window size whenever the scale changes, even if the measured
  // height happens to land within the dead-band (width still needs updating).
  useEffect(() => {
    lastHeightRef.current = 0
    void applySize()
  }, [scale, baseWidth, applySize])

  useEffect(() => {
    const el = sizeRef.current
    if (!el) return
    const observer = new ResizeObserver(() => { void applySize() })
    observer.observe(el)
    const unlistenPromise = getCurrentWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) {
          // Force a re-apply even if the height didn't change while hidden
          lastHeightRef.current = 0
          void applySize()
        }
      })
    return () => {
      observer.disconnect()
      void unlistenPromise.then(unlisten => unlisten())
    }
  }, [applySize])

  return { sizeRef, containerRef }
}
