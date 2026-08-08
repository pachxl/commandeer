import type { CSSProperties } from 'react'
import { useOnixOptics } from '../hooks/useOnixOptics'

export interface OnixOpticalShellProps {
  expanded: boolean
  radius?: number
  className?: string
  style?: CSSProperties
}

/**
 * Purely presentational optical layer. Mount it as a direct child of the
 * positioned palette shell, before a relatively positioned content layer.
 * Pointer tracking is registered on that parent because this surface remains
 * non-interactive and transparent to hit testing.
 */
export default function OnixOpticalShell({ expanded, radius = 28, className, style }: OnixOpticalShellProps) {
  const compact = !expanded
  const { canvasRef, layerRef, mode, reducedMotion, reducedTransparency, forcedColors } = useOnixOptics({
    compact,
    radius,
  })
  const borderRadius = compact ? 'var(--onix-capsule-radius)' : 'var(--onix-panel-radius)'
  const fallbackVisible = mode === 'css'
  const accessibilityFallback = reducedTransparency || forcedColors

  return (
    <div
      ref={layerRef}
      className={className}
      data-onix-optical-shell
      data-onix-optics={mode}
      data-onix-reduced-motion={reducedMotion || undefined}
      data-onix-reduced-transparency={reducedTransparency || undefined}
      data-onix-forced-colors={forcedColors || undefined}
      aria-hidden="true"
      style={{
        position: 'absolute',
        inset: 0,
        zIndex: 0,
        isolation: 'isolate',
        overflow: 'hidden',
        borderRadius,
        transition: reducedMotion ? 'none' : 'border-radius 155ms cubic-bezier(0.16, 0.82, 0.22, 1)',
        ...style,
        pointerEvents: 'none',
        userSelect: 'none',
      }}
    >
      {expanded && !reducedMotion && !accessibilityFallback && (
        <div
          data-onix-morph-guard
          style={{
            position: 'absolute',
            inset: 0,
            borderRadius: 'inherit',
            background: 'rgba(1, 2, 5, 0.96)',
            animation: 'onix-morph-guard 180ms linear both',
            pointerEvents: 'none',
          }}
        />
      )}

      <canvas
        ref={canvasRef}
        data-onix-optics-canvas
        style={{
          position: 'absolute',
          inset: 0,
          width: '100%',
          height: '100%',
          display: 'block',
          opacity: mode === 'webgl' ? 1 : 0,
          pointerEvents: 'none',
        }}
      />

      <div
        data-onix-css-material
        style={{
          position: 'absolute',
          inset: 0,
          borderRadius: 'inherit',
          opacity: fallbackVisible ? 1 : 0,
          background: accessibilityFallback
            ? forcedColors
              ? 'Canvas'
              : '#06070b'
            : 'radial-gradient(120% 85% at 50% -42%, rgba(168, 203, 255, 0.07), transparent 56%), linear-gradient(180deg, rgba(15, 18, 25, 0.91), rgba(2, 3, 7, 0.94))',
          border: forcedColors ? '1px solid CanvasText' : '1px solid rgba(210, 222, 241, 0.42)',
          boxShadow: forcedColors
            ? 'none'
            : 'inset 0 1px 0 rgba(255,255,255,0.16), inset 0 -1px 0 rgba(0,0,0,0.82), inset 10px 0 30px rgba(0,0,0,0.18)',
        }}
      />

      {!accessibilityFallback && (
        <div
          data-onix-css-caustic
          style={{
            position: 'absolute',
            inset: 1,
            borderRadius: 'inherit',
            opacity: fallbackVisible ? 'var(--onix-pointer-activity, 0)' : 0,
            background:
              'radial-gradient(36% 14% at var(--onix-pointer-x, 16%) 1%, rgba(255,255,255,0.22), rgba(144,190,255,0.06) 45%, transparent 76%), radial-gradient(20% 55% at 99% 68%, rgba(128,169,255,0.09), transparent 72%)',
            mixBlendMode: 'screen',
            transition: reducedMotion ? 'none' : 'opacity 120ms ease-out',
          }}
        />
      )}
    </div>
  )
}
