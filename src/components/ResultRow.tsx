import { forwardRef, useState } from 'react'
import type { PaletteItem } from '../types'

interface ResultRowProps {
  item: PaletteItem
  selected: boolean
  onSelect: () => void
  onHover: () => void
}

const ResultRow = forwardRef<HTMLDivElement, ResultRowProps>(
  ({ item, selected, onSelect, onHover }, ref) => {
    const [hovered, setHovered] = useState(false)
    const isDataUrl = item.icon.startsWith('data:')
    const hasIcon = item.icon.length > 0
    const active = selected || hovered

    return (
      <div
        ref={ref}
        onClick={onSelect}
        onMouseEnter={() => { setHovered(true); onHover() }}
        onMouseLeave={() => setHovered(false)}
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '3px 10px',
          cursor: 'pointer',
          background: selected ? 'var(--bg-select)' : hovered ? 'var(--bg-hover)' : 'transparent',
          userSelect: 'none',
        }}
      >
        {hasIcon && (
          <div style={{
            width: 18,
            height: 18,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
            fontSize: 12,
            color: active ? '#fff' : 'var(--text-dim)',
          }}>
            {isDataUrl
              ? <img src={item.icon} width={16} height={16} style={{ objectFit: 'contain' }} />
              : item.icon
            }
          </div>
        )}

        <span style={{
          flex: 1,
          fontSize: 13,
          fontFamily: 'var(--font)',
          color: active ? '#fff' : 'var(--text)',
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          lineHeight: '22px',
        }}>
          {item.label}
        </span>

        {item.sublabel && (
          <span style={{
            fontSize: 11,
            fontFamily: 'var(--font-ui)',
            color: 'var(--text-dim)',
            whiteSpace: 'nowrap',
            flexShrink: 0,
          }}>
            {item.sublabel}
          </span>
        )}

        {item.isFolder && (
          <span style={{
            fontSize: 13,
            color: active ? 'rgba(255,255,255,0.5)' : 'var(--text-dim)',
            flexShrink: 0,
            lineHeight: '22px',
          }}>
            ›
          </span>
        )}
      </div>
    )
  }
)

ResultRow.displayName = 'ResultRow'
export default ResultRow
