import { useEffect, useRef } from 'react'
import type { PaletteItem } from '../types'
import { scrollToReveal } from '../lib/scroll'
import ResultRow from './ResultRow'

interface ResultsListProps {
  items: PaletteItem[]
  selectedIndex: number
  onSelect: (item: PaletteItem) => void
  onHover: (index: number) => void
}

export default function ResultsList({ items, selectedIndex, onSelect, onHover }: ResultsListProps) {
  const listRef = useRef<HTMLDivElement>(null)
  const selectedRef = useRef<HTMLDivElement>(null)
  const lastMousePos = useRef<{ x: number; y: number } | null>(null)

  useEffect(() => {
    scrollToReveal(listRef.current, selectedRef.current)
  }, [selectedIndex])

  // Hover-selection follows *physical* mouse movement only (same guard as
  // ResultsGrid): WKWebView re-fires enter/move events when rows re-rank or
  // scroll under a stationary cursor, which yanked the selection to whatever
  // row sat under the mouse on every keystroke. Coordinates identical to the
  // last event = not a real move, so it is ignored.
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
    const row = target.closest('[data-list-index]') as HTMLElement | null
    if (row) {
      const index = parseInt(row.dataset.listIndex ?? '', 10)
      if (!Number.isNaN(index) && index !== selectedIndex) onHover(index)
    }
  }

  function handleMouseLeave() {
    lastMousePos.current = null
  }

  return (
    <div
      ref={listRef}
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
      style={{
        maxHeight: 360,
        overflowY: 'auto',
        padding: '4px 6px',
        scrollbarWidth: 'thin',
        scrollbarColor: 'var(--border-strong) transparent',
      }}
    >
      {items.map((item, i) => (
        <ResultRow
          key={item.id}
          item={item}
          index={i}
          selected={i === selectedIndex}
          ref={i === selectedIndex ? selectedRef : null}
          onSelect={() => onSelect(item)}
        />
      ))}
    </div>
  )
}
