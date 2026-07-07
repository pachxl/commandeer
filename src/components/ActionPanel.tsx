import { useEffect, useRef } from 'react'
import type { ActionItem } from '../types'
import { scrollToReveal } from '../lib/scroll'
import { getIconSvg, hasIcon } from './Icon'

interface ActionPanelProps {
  items: ActionItem[]
  selectedIndex: number
  onSelect: (item: ActionItem) => void
  onHover: (index: number) => void
  // When inside a submenu: its label (shown in the header) and a back handler
  title?: string
  onBack?: () => void
}

export default function ActionPanel({ items, selectedIndex, onSelect, onHover, title, onBack }: ActionPanelProps) {
  const listRef = useRef<HTMLDivElement>(null)
  const selectedRef = useRef<HTMLDivElement>(null)
  const lastMousePos = useRef<{ x: number; y: number } | null>(null)

  useEffect(() => {
    scrollToReveal(listRef.current, selectedRef.current)
  }, [selectedIndex])

  // Movement-guarded hover selection — see ResultsList for why plain
  // mouseenter is wrong on WKWebView.
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
    const row = target.closest('[data-action-index]') as HTMLElement | null
    if (row) {
      const index = parseInt(row.dataset.actionIndex ?? '', 10)
      if (!Number.isNaN(index) && index !== selectedIndex) onHover(index)
    }
  }

  return (
    <div
      ref={listRef}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => { lastMousePos.current = null }}
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
      <div
        onClick={title && onBack ? onBack : undefined}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 4,
          padding: '2px 8px 8px',
          fontSize: 10,
          fontFamily: 'var(--font-ui)',
          color: 'var(--text-dim)',
          textTransform: 'uppercase',
          letterSpacing: 0.6,
          cursor: title && onBack ? 'pointer' : 'default',
        }}
      >
        {title ? `‹ ${title}` : 'Actions'}
      </div>
      {items.map((item, i) => {
        const selected = i === selectedIndex
        return (
          <div
            key={item.id}
            ref={selected ? selectedRef : null}
            data-action-index={i}
            onClick={() => onSelect(item)}
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
            {item.submenu && (
              <span style={{
                fontSize: 13,
                lineHeight: '14px',
                color: selected ? '#ffffff' : 'var(--text-dim)',
                flexShrink: 0,
              }}>
                ›
              </span>
            )}
          </div>
        )
      })}
    </div>
  )
}
