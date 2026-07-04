import { forwardRef, useEffect, useState } from 'react'
import type { PaletteItem } from '../types'
import { pathIcon } from '../lib/tauri'
import { getIconSvg, hasIcon } from './Icon'

interface ResultRowProps {
  item: PaletteItem
  /** Position in the list, exposed as data-list-index for the container's
   *  movement-guarded hover handler (see ResultsList). */
  index: number
  selected: boolean
  onSelect: () => void
}

// Shell icons resolve per extension (folders share one entry), so a shared
// cache means one IPC round trip per distinct extension, not per row. In-flight
// promises are cached too so simultaneous rows coalesce onto one request.
// Executables, shortcuts, .desktop entries, and extensionless files (Linux
// binaries) carry individual icons, so those cache per path.
const shellIconCache = new Map<string, Promise<string | null>>()

function shellIconFor(item: PaletteItem): Promise<string | null> {
  const path = item.iconPath!
  const ext = /\.([^./\\]+)$/.exec(path)?.[1]?.toLowerCase() ?? ''
  const key = item.icon === 'folder'
    ? ':folder:'
    : path.startsWith('shell:') || ext === 'exe' || ext === 'lnk' || ext === 'desktop' || ext === ''
      ? path.toLowerCase()
      : ext
  let cached = shellIconCache.get(key)
  if (!cached) {
    cached = pathIcon(path).catch(() => null)
    shellIconCache.set(key, cached)
  }
  return cached
}

const ResultRow = forwardRef<HTMLDivElement, ResultRowProps>(
  ({ item, index, selected, onSelect }, ref) => {
    const [shellIcon, setShellIcon] = useState<string | null>(null)

    // Upgrade named file/folder icons to the real shell icon when available
    useEffect(() => {
      setShellIcon(null)
      if (!item.iconPath || item.icon.startsWith('data:')) return
      let cancelled = false
      shellIconFor(item).then(icon => {
        if (!cancelled && icon) setShellIcon(icon)
      })
      return () => { cancelled = true }
    }, [item])

    const displayIcon = shellIcon ?? item.icon
    const isDataUrl = displayIcon.startsWith('data:')
    const isNamedIcon = hasIcon(displayIcon)
    const hasIconValue = displayIcon.length > 0
    const bg = selected ? 'var(--accent)' : 'transparent'
    const fg = selected ? '#ffffff' : 'var(--text)'
    const subFg = selected ? 'rgba(255,255,255,0.78)' : 'var(--text-dim)'
    // An explicit item color overrides the theme icon tint (e.g. the Claude
    // orange or game-mode green in Settings)
    const iconColor = item.iconColor ?? item.color ?? (selected ? '#ffffff' : subFg)

    return (
      <div
        ref={ref}
        data-list-index={index}
        onClick={onSelect}
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
              ? <img src={displayIcon} width={18} height={18} style={{ objectFit: 'contain' }} />
              : isNamedIcon
                ? <div dangerouslySetInnerHTML={{ __html: getIconSvg(displayIcon, iconColor, 16) ?? '' }} style={{ display: 'flex' }} />
                : displayIcon
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

        {item.accessories && item.accessories.length > 0 && (
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            flexShrink: 0,
          }}>
            {item.accessories.map((acc, i) => (
              <span
                key={i}
                style={{
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: 4,
                  fontSize: 10,
                  fontFamily: 'var(--font-ui)',
                  color: acc.color ?? subFg,
                  background: 'rgba(255,255,255,0.06)',
                  padding: '1px 5px',
                  borderRadius: 3,
                  whiteSpace: 'nowrap',
                }}
              >
                {acc.icon && hasIcon(acc.icon) && (
                  <span
                    style={{ width: 10, height: 10, display: 'flex', alignItems: 'center', justifyContent: 'center' }}
                    dangerouslySetInnerHTML={{
                      __html: getIconSvg(acc.icon, acc.color ?? subFg, 10) ?? '',
                    }}
                  />
                )}
                {acc.text}
              </span>
            ))}
          </div>
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
