import { useEffect, useRef } from 'react'
import type { PaletteItem } from '../types'
import { fuzzyMatch } from '../lib/fuzzy'
import { scrollToReveal } from '../lib/scroll'
import { getIconSvg, hasIcon } from './Icon'

interface ResultsGridProps {
  items: PaletteItem[]
  selectedIndex: number
  query?: string
  columns?: number
  onSelect: (item: PaletteItem) => void
  onHover: (index: number) => void
}

// Split `text` into alternating plain/matched segments for highlight rendering
function highlightSegments(text: string, positions: number[]): { text: string; matched: boolean }[] {
  if (positions.length === 0) return [{ text, matched: false }]
  const matched = new Set(positions)
  const segments: { text: string; matched: boolean }[] = []
  for (let i = 0; i < text.length; i++) {
    const isMatch = matched.has(i)
    const last = segments[segments.length - 1]
    if (last && last.matched === isMatch) last.text += text[i]
    else segments.push({ text: text[i], matched: isMatch })
  }
  return segments
}

export default function ResultsGrid({ items, selectedIndex, query, columns = 4, onSelect, onHover }: ResultsGridProps) {
  const gridRef = useRef<HTMLDivElement>(null)
  const selectedRef = useRef<HTMLDivElement>(null)
  const lastMousePos = useRef<{ x: number; y: number } | null>(null)

  useEffect(() => {
    scrollToReveal(gridRef.current, selectedRef.current)
  }, [selectedIndex])

  function handleMouseMove(e: React.MouseEvent) {
    const pos = { x: e.clientX, y: e.clientY }
    const last = lastMousePos.current
    if (!last) {
      lastMousePos.current = pos
      return
    }
    if (last.x === pos.x && last.y === pos.y) return
    lastMousePos.current = pos

    const target = e.target as HTMLElement
    const cell = target.closest('[data-grid-index]') as HTMLElement | null
    if (cell) {
      const index = parseInt(cell.dataset.gridIndex ?? '', 10)
      if (!Number.isNaN(index) && index !== selectedIndex) onHover(index)
    }
  }

  function handleMouseLeave() {
    lastMousePos.current = null
  }

  return (
    <div
      ref={gridRef}
      data-results-list
      style={{
        flex: 1,
        minHeight: 0,
        overflowY: 'auto',
        padding: 'var(--grid-padding)',
        display: 'grid',
        gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))`,
        gap: 'var(--grid-gap)',
        alignContent: 'start',
        scrollbarWidth: 'thin',
        scrollbarColor: 'var(--border-strong) transparent',
      }}
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
    >
      {items.map((item, i) => {
        const selected = i === selectedIndex
        const labelMatch = query ? fuzzyMatch(query, item.label) : null
        const labelSegments = labelMatch
          ? highlightSegments(item.label, labelMatch.positions)
          : [{ text: item.label, matched: false }]

        const isDataUrl = item.icon.startsWith('data:')
        const isNamedIcon = hasIcon(item.icon)
        const hasIconValue = item.icon.length > 0

        return (
          <div
            key={item.id}
            ref={selected ? selectedRef : null}
            data-grid-index={i}
            onClick={() => onSelect(item)}
            style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              gap: 6,
              padding: 'var(--grid-cell-padding)',
              borderRadius: 'var(--grid-cell-radius)',
              cursor: 'pointer',
              background: selected ? 'var(--accent)' : 'transparent',
              border: `1px solid ${selected ? 'var(--accent)' : 'transparent'}`,
              userSelect: 'none',
              minHeight: 72,
            }}
          >
            {hasIconValue && (
              <div style={{
                width: 'var(--grid-icon-size)',
                height: 'var(--grid-icon-size)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                flexShrink: 0,
                fontSize: 'var(--grid-icon-font-size)',
                color: selected ? '#ffffff' : 'var(--text)',
              }}>
                {isDataUrl
                  ? <img src={item.icon} width="100%" height="100%" style={{ objectFit: 'contain' }} />
                  : isNamedIcon
                    ? <div dangerouslySetInnerHTML={{ __html: getIconSvg(item.icon, selected ? '#ffffff' : 'var(--text)') ?? '' }} style={{ display: 'flex' }} />
                    : item.icon
                }
              </div>
            )}
            <span style={{
              fontSize: 'var(--grid-label-font-size)',
              fontFamily: item.fontFamily ? `"${item.fontFamily}", var(--font)` : 'var(--font)',
              color: selected ? '#ffffff' : 'var(--text)',
              fontWeight: 400,
              textAlign: 'center',
              lineHeight: '14px',
              display: '-webkit-box',
              WebkitLineClamp: 2,
              WebkitBoxOrient: 'vertical',
              overflow: 'hidden',
              wordBreak: 'break-word',
            }}>
              {labelSegments.map((seg, j) => seg.matched
                ? <span key={j} style={{ fontWeight: 600, color: selected ? '#ffffff' : 'var(--accent)' }}>{seg.text}</span>
                : <span key={j}>{seg.text}</span>
              )}
            </span>
            {item.sublabel && (
              <span style={{
                fontSize: 'var(--grid-sublabel-font-size)',
                fontFamily: 'var(--font-ui)',
                color: selected ? 'rgba(255,255,255,0.78)' : 'var(--text-dim)',
                textAlign: 'center',
                lineHeight: '11px',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                whiteSpace: 'nowrap',
                maxWidth: '100%',
              }}>
                {item.sublabel}
              </span>
            )}
          </div>
        )
      })}
    </div>
  )
}
