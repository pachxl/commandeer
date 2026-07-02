import { forwardRef, useState } from 'react'
import type { PaletteItem } from '../types'
import { getIconSvg, hasIcon } from './Icon'

interface ResultRowProps {
  item: PaletteItem
  selected: boolean
  onSelect: () => void
  onHover: () => void
}

const ResultRow = forwardRef<HTMLDivElement, ResultRowProps>(
  ({ item, selected, onSelect, onHover }, ref) => {
    const [hovered, setHovered] = useState(false)
    const active = selected || hovered

    const isDataUrl = item.icon.startsWith('data:')
    const isNamedIcon = hasIcon(item.icon)
    const hasIconValue = item.icon.length > 0
    const bg = active
      ? (selected ? 'var(--accent)' : 'var(--bg-select)')
      : 'transparent'
    const fg = selected ? '#ffffff' : 'var(--text)'
    const subFg = selected ? 'rgba(255,255,255,0.78)' : 'var(--text-dim)'
    const iconColor = selected ? '#ffffff' : subFg

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
          padding: '4px 10px',
          borderRadius: 5,
          cursor: 'pointer',
          background: bg,
          userSelect: 'none',
        }}
      >
        {hasIconValue && (
          <div style={{
            width: 18,
            height: 18,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
            fontSize: 14,
            color: iconColor,
          }}>
            {isDataUrl
              ? <img src={item.icon} width={18} height={18} style={{ objectFit: 'contain' }} />
              : isNamedIcon
                ? <div dangerouslySetInnerHTML={{ __html: getIconSvg(item.icon, iconColor, 16) ?? '' }} style={{ display: 'flex' }} />
                : item.icon
            }
          </div>
        )}

        <span style={{
          flex: 1,
          fontSize: 13,
          fontFamily: 'var(--font)',
          color: fg,
          fontWeight: 400,
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          lineHeight: '18px',
        }}>
          {item.label}
        </span>

        {item.sublabel && (
          <span style={{
            fontSize: 11,
            fontFamily: 'var(--font-ui)',
            color: subFg,
            whiteSpace: 'nowrap',
            flexShrink: 0,
          }}>
            {item.sublabel}
          </span>
        )}

        {item.isFolder && (
          <span style={{
            fontSize: 13,
            color: subFg,
            flexShrink: 0,
            lineHeight: '18px',
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
