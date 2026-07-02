import { useEffect, useRef } from 'react'
import type { ActionItem } from '../types'
import { getIconSvg, hasIcon } from './Icon'

interface ActionPanelProps {
  items: ActionItem[]
  selectedIndex: number
  onSelect: (item: ActionItem) => void
  onHover: (index: number) => void
}

export default function ActionPanel({ items, selectedIndex, onSelect, onHover }: ActionPanelProps) {
  const listRef = useRef<HTMLDivElement>(null)
  const selectedRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    selectedRef.current?.scrollIntoView({ block: 'nearest' })
  }, [selectedIndex])

  return (
    <div
      ref={listRef}
      style={{
        position: 'absolute',
        top: 0,
        right: 0,
        bottom: 0,
        width: 240,
        background: 'var(--bg-elevated)',
        borderLeft: '1px solid var(--border)',
        display: 'flex',
        flexDirection: 'column',
        padding: '8px 6px',
        overflowY: 'auto',
        zIndex: 10,
      }}
    >
      <div style={{
        padding: '2px 8px 8px',
        fontSize: 10,
        fontFamily: 'var(--font-ui)',
        color: 'var(--text-dim)',
        textTransform: 'uppercase',
        letterSpacing: 0.6,
      }}>
        Actions
      </div>
      {items.map((item, i) => {
        const selected = i === selectedIndex
        return (
          <div
            key={item.id}
            ref={selected ? selectedRef : null}
            onClick={() => onSelect(item)}
            onMouseEnter={() => onHover(i)}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 8,
              padding: '5px 8px',
              borderRadius: 5,
              cursor: 'pointer',
              background: selected ? 'var(--accent)' : 'transparent',
              userSelect: 'none',
            }}
          >
            {item.icon && hasIcon(item.icon) && (
              <div
                style={{ width: 14, height: 14, display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, color: selected ? '#ffffff' : 'var(--text-dim)' }}
                dangerouslySetInnerHTML={{ __html: getIconSvg(item.icon, selected ? '#ffffff' : 'var(--text-dim)') ?? '' }}
              />
            )}
            <span style={{
              flex: 1,
              fontSize: 12,
              fontFamily: 'var(--font)',
              color: selected ? '#ffffff' : 'var(--text)',
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}>
              {item.label}
            </span>
            {item.shortcut && (
              <kbd style={{
                fontFamily: 'var(--font-ui)',
                fontSize: 10,
                padding: '1px 5px',
                borderRadius: 3,
                background: selected ? 'rgba(255,255,255,0.18)' : 'rgba(255,255,255,0.06)',
                border: '1px solid var(--border)',
                color: selected ? '#ffffff' : 'var(--text-dim)',
                flexShrink: 0,
              }}>
                {item.shortcut}
              </kbd>
            )}
          </div>
        )
      })}
    </div>
  )
}
