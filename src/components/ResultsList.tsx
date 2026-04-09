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

  // Scroll selected item into view
  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  return (
    <div
      ref={listRef}
      style={{
        maxHeight: 280,
        overflowY: 'auto',
        padding: 0,
        scrollbarWidth: 'thin',
        scrollbarColor: 'var(--border) transparent',
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
