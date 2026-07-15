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
      data-action-panel
      onMouseMove={handleMouseMove}
      onMouseLeave={() => {
        lastMousePos.current = null
      }}
      style={{
        position: 'absolute',
        top: 0,
        right: 0,
        bottom: 0,
        width: 'var(--action-panel-width)',
        background: 'var(--bg-elevated)',
        borderLeft: '1px solid var(--divider)',
        display: 'flex',
        flexDirection: 'column',
        padding: 'var(--action-panel-padding)',
        overflowY: 'auto',
        zIndex: 10,
        boxShadow: 'var(--action-panel-shadow)',
      }}
    >
      <div
        onClick={title && onBack ? onBack : undefined}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 4,
          padding: '2px 8px 8px',
          fontSize: 'var(--accessory-font-size)',
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
              padding: 'var(--action-row-padding)',
              borderRadius: 'var(--action-row-radius)',
              cursor: 'pointer',
              background: selected ? 'var(--action-row-selected-bg)' : 'transparent',
              userSelect: 'none',
            }}
          >
            {item.icon && hasIcon(item.icon) && (
              <div
                style={{
                  width: 'var(--action-icon-size)',
                  height: 'var(--action-icon-size)',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  flexShrink: 0,
                  color: selected ? 'var(--action-row-selected-fg)' : 'var(--text-dim)',
                }}
                dangerouslySetInnerHTML={{
                  __html: getIconSvg(item.icon, selected ? 'var(--action-row-selected-fg)' : 'var(--text-dim)') ?? '',
                }}
              />
            )}
            <span
              style={{
                flex: 1,
                fontSize: 'var(--action-font-size)',
                fontFamily: 'var(--font)',
                color: selected ? 'var(--action-row-selected-fg)' : 'var(--text)',
                whiteSpace: 'nowrap',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
              }}
            >
              {item.label}
            </span>
            {item.shortcut && (
              <kbd
                style={{
                  fontFamily: 'var(--font-ui)',
                  fontSize: 'var(--kbd-font-size)',
                  padding: 'var(--kbd-padding)',
                  borderRadius: 'var(--kbd-radius)',
                  background: selected ? 'var(--action-kbd-selected-bg)' : 'var(--action-kbd-bg)',
                  border: '1px solid var(--kbd-border)',
                  boxShadow: 'var(--kbd-shadow)',
                  color: selected ? 'var(--action-row-selected-fg)' : 'var(--text-dim)',
                  flexShrink: 0,
                }}
              >
                {item.shortcut}
              </kbd>
            )}
            {item.submenu && (
              <span
                style={{
                  fontSize: 'var(--action-font-size)',
                  lineHeight: '14px',
                  color: selected ? 'var(--action-row-selected-fg)' : 'var(--text-dim)',
                  flexShrink: 0,
                }}
              >
                ›
              </span>
            )}
          </div>
        )
      })}
    </div>
  )
}
