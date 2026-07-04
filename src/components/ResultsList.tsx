import { useEffect, useRef } from 'react'
import type { PaletteItem } from '../types'
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

  // Manual scroll instead of Element.scrollIntoView({ block: 'nearest' }):
  // WKWebView (macOS) interprets 'nearest' by recentering the element, which
  // makes the selection jump to the middle when paging past the bottom of a long
  // list. Computing scrollTop from the rects is deterministic across engines
  // (Chromium on Windows/Linux, WebKit on macOS) and only scrolls the minimum
  // needed to bring the selected row fully into view.
  useEffect(() => {
    const container = listRef.current
    const el = selectedRef.current
    if (!container || !el) return
    const cRect = container.getBoundingClientRect()
    const eRect = el.getBoundingClientRect()
    if (eRect.top < cRect.top) {
      container.scrollTop += eRect.top - cRect.top
    } else if (eRect.bottom > cRect.bottom) {
      container.scrollTop += eRect.bottom - cRect.bottom
    }
  }, [selectedIndex])

  return (
    <div
      ref={listRef}
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
          selected={i === selectedIndex}
          ref={i === selectedIndex ? selectedRef : null}
          onSelect={() => onSelect(item)}
          onHover={() => onHover(i)}
        />
      ))}
    </div>
  )
}
