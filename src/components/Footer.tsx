import type { PaletteItem } from '../types'

interface FooterProps {
  selectedItem: PaletteItem | null
  primaryAction: string | null
}

// Raycast-style footer: the selected item's icon and its primary (Enter)
// action on the left.
export default function Footer({ selectedItem, primaryAction }: FooterProps) {
  const icon = selectedItem?.icon ?? ''
  const isDataUrl = icon.startsWith('data:')

  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: '4px 10px',
      borderTop: '1px solid var(--border)',
      fontSize: 11,
      fontFamily: 'var(--font-ui)',
      color: 'var(--text-dim)',
      userSelect: 'none',
      minHeight: 26,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
        {selectedItem && icon && (
          <div style={{
            width: 14,
            height: 14,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
            fontSize: 11,
          }}>
            {isDataUrl
              ? <img src={icon} width={14} height={14} style={{ objectFit: 'contain' }} />
              : icon
            }
          </div>
        )}
        {primaryAction && (
          <>
            <span style={{
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}>
              {primaryAction}
            </span>
            <kbd style={kbdStyle}>↵</kbd>
          </>
        )}
      </div>
    </div>
  )
}

const kbdStyle: React.CSSProperties = {
  fontFamily: 'var(--font-ui)',
  fontSize: 10,
  padding: '1px 5px',
  borderRadius: 3,
  background: 'rgba(255,255,255,0.06)',
  border: '1px solid var(--border)',
  color: 'var(--text-dim)',
}
