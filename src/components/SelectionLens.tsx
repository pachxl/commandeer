import { useLayoutEffect, useRef, type CSSProperties, type RefObject } from 'react'

export type SelectionLensSurface = 'list' | 'grid' | 'action'

interface SelectionLensProps {
  containerRef: RefObject<HTMLElement>
  targetRef: RefObject<HTMLElement>
  surface: SelectionLensSurface
  active?: boolean
}

type LensStyle = CSSProperties & {
  '--selection-lens-x': string
  '--selection-lens-y': string
  '--selection-lens-width': string
  '--selection-lens-height': string
}

const radiusBySurface: Record<SelectionLensSurface, string> = {
  list: 'var(--row-radius)',
  grid: 'var(--grid-cell-radius)',
  action: 'var(--action-row-radius)',
}

/**
 * One shared highlight layer for a selectable surface. Geometry lives in CSS
 * variables so Onix can animate the lens without making selection state depend
 * on animation timing. Default keeps the lens transparent and continues using
 * each row's existing inline selected background.
 */
export default function SelectionLens({ containerRef, targetRef, surface, active = true }: SelectionLensProps) {
  const lensRef = useRef<HTMLDivElement>(null)

  useLayoutEffect(() => {
    const lens = lensRef.current
    if (!lens) return
    // A child layout effect can run before React attaches the host ref on its
    // parent surface during the first commit. The lens is always a direct child
    // of that surface, so parentElement is the equivalent synchronous fallback.
    const container = containerRef.current ?? lens.parentElement
    const target = targetRef.current

    const hide = () => {
      lens.removeAttribute('data-visible')
      lens.style.setProperty('--selection-lens-width', '0px')
      lens.style.setProperty('--selection-lens-height', '0px')
    }

    if (!active || !container || !target) {
      hide()
      return
    }

    const measure = () => {
      // All current consumers render the selected item as a direct child of
      // the positioned scroll surface. offset* values stay in that surface's
      // local CSS coordinate space, unlike getBoundingClientRect(), which is
      // already multiplied by the palette's CSS zoom.
      lens.style.setProperty('--selection-lens-x', `${target.offsetLeft}px`)
      lens.style.setProperty('--selection-lens-y', `${target.offsetTop}px`)
      lens.style.setProperty('--selection-lens-width', `${target.offsetWidth}px`)
      lens.style.setProperty('--selection-lens-height', `${target.offsetHeight}px`)
      lens.setAttribute('data-visible', 'true')
    }

    measure()

    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure)
    observer?.observe(container)
    observer?.observe(target)
    window.addEventListener('resize', measure)

    return () => {
      observer?.disconnect()
      window.removeEventListener('resize', measure)
    }
  })

  const style: LensStyle = {
    '--selection-lens-x': '0px',
    '--selection-lens-y': '0px',
    '--selection-lens-width': '0px',
    '--selection-lens-height': '0px',
    position: 'absolute',
    left: 0,
    top: 0,
    width: 'var(--selection-lens-width)',
    height: 'var(--selection-lens-height)',
    transform: 'translate3d(var(--selection-lens-x), var(--selection-lens-y), 0)',
    borderRadius: radiusBySurface[surface],
    border: 'var(--selection-lens-border, 0 solid transparent)',
    background: 'var(--selection-lens-bg, transparent)',
    boxShadow: 'var(--selection-lens-shadow, none)',
    opacity: active ? 'var(--selection-lens-opacity, 0)' : 0,
    transition: 'var(--selection-lens-transition, none)',
    pointerEvents: 'none',
    zIndex: 0,
    willChange: active ? 'var(--selection-lens-will-change, auto)' : undefined,
  }

  return (
    <div
      ref={lensRef}
      data-selection-lens={surface}
      data-active={active ? 'true' : 'false'}
      aria-hidden="true"
      style={style}
    />
  )
}
