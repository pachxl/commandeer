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
// Executables, shortcuts, .desktop entries, macOS .app bundles, and
// extensionless files (Linux/macOS binaries) carry individual icons, so those
// cache per path. Resolved values are also kept synchronously so re-rendered
// rows (every keystroke re-ranks the list) paint the real icon immediately
// instead of flashing the generic one for a frame.
const shellIconCache = new Map<string, Promise<string | null>>()
const resolvedIconCache = new Map<string, string | null>()

function shellIconKey(item: PaletteItem): string {
  const path = item.iconPath!
  const ext = /\.([^./\\]+)$/.exec(path)?.[1]?.toLowerCase() ?? ''
  // .app first: bundles are directories, so they'd otherwise fall into the
  // shared folder slot and every app would show the first-resolved app's icon.
  if (ext === 'app') return path.toLowerCase()
  if (item.icon === 'folder') return ':folder:'
  return path.startsWith('shell:') || ext === 'exe' || ext === 'lnk' || ext === 'desktop' || ext === ''
    ? path.toLowerCase()
    : ext
}

function shellIconFor(item: PaletteItem): Promise<string | null> {
  const key = shellIconKey(item)
  let cached = shellIconCache.get(key)
  if (!cached) {
    cached = pathIcon(item.iconPath!).catch(() => null)
    shellIconCache.set(key, cached)
    cached.then(icon => resolvedIconCache.set(key, icon))
  }
  return cached
}

const ResultRow = forwardRef<HTMLDivElement, ResultRowProps>(({ item, index, selected, onSelect }, ref) => {
  const wantsShellIcon = !!item.iconPath && !item.icon.startsWith('data:')
  const [shellIcon, setShellIcon] = useState<string | null>(() =>
    wantsShellIcon ? (resolvedIconCache.get(shellIconKey(item)) ?? null) : null,
  )

  // Upgrade named file/folder icons to the real shell icon when available
  useEffect(() => {
    if (!wantsShellIcon) {
      setShellIcon(null)
      return
    }
    // Already-resolved icons apply synchronously (no generic-icon flash)
    const known = resolvedIconCache.get(shellIconKey(item))
    setShellIcon(known ?? null)
    if (known !== undefined) return
    let cancelled = false
    shellIconFor(item).then(icon => {
      if (!cancelled && icon) setShellIcon(icon)
    })
    return () => {
      cancelled = true
    }
  }, [item, wantsShellIcon])

  const displayIcon = shellIcon ?? item.icon
  const isDataUrl = displayIcon.startsWith('data:')
  const isNamedIcon = hasIcon(displayIcon)
  const hasIconValue = displayIcon.length > 0
  const bg = selected ? 'var(--row-selected-bg)' : 'transparent'
  const fg = selected ? 'var(--row-selected-fg)' : 'var(--text)'
  const subFg = selected ? 'var(--row-selected-sublabel-fg)' : 'var(--text-dim)'
  // An explicit item color overrides the theme icon tint (e.g. the Claude
  // orange or game-mode green in Settings)
  const iconColor = item.iconColor ?? item.color ?? (selected ? 'var(--row-selected-fg)' : subFg)

  return (
    <div
      ref={ref}
      data-list-index={index}
      data-selected={selected || undefined}
      onClick={onSelect}
      style={{
        display: 'flex',
        alignItems: 'center',
        gap: 'var(--row-gap)',
        padding: 'var(--row-padding)',
        borderRadius: 'var(--row-radius)',
        cursor: 'pointer',
        background: bg,
        boxShadow: selected ? 'var(--row-selected-shadow)' : 'none',
        transition: 'var(--row-transition)',
        userSelect: 'none',
      }}
    >
      {hasIconValue && (
        <div
          style={{
            width: 'var(--icon-size)',
            height: 'var(--icon-size)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            flexShrink: 0,
            fontSize: 'var(--icon-font-size)',
            color: iconColor,
          }}
        >
          {isDataUrl ? (
            <img src={displayIcon} width="100%" height="100%" style={{ objectFit: 'contain' }} />
          ) : isNamedIcon ? (
            <div
              data-row-icon
              dangerouslySetInnerHTML={{ __html: getIconSvg(displayIcon, iconColor, 16) ?? '' }}
              style={{ display: 'flex' }}
            />
          ) : (
            displayIcon
          )}
        </div>
      )}

      {item.running && (
        // Running-app status dot: a small green pip before the label, with a
        // soft glow so it reads on both the transparent and accent-selected row.
        <span
          title="Running"
          style={{
            width: 6,
            height: 6,
            borderRadius: '50%',
            flexShrink: 0,
            background: selected ? 'var(--row-selected-fg)' : 'var(--row-active-indicator-bg)',
            boxShadow: selected ? 'none' : '0 0 4px rgba(48,209,88,0.9)',
          }}
        />
      )}

      <div
        style={{
          flex: 1,
          minWidth: 0,
          display: 'flex',
          alignItems: 'baseline',
          gap: 6,
          overflow: 'hidden',
        }}
      >
        <span
          style={{
            minWidth: 0,
            fontSize: 'var(--label-font-size)',
            fontFamily: 'var(--font)',
            color: fg,
            fontWeight: 400,
            whiteSpace: 'nowrap',
            overflow: 'hidden',
            textOverflow: 'ellipsis',
            lineHeight: '1.3',
          }}
        >
          {item.label}
        </span>
        {item.sublabel && (
          <span
            style={{
              marginLeft: 'var(--sublabel-margin-left)',
              minWidth: 0,
              fontSize: 'var(--sublabel-font-size)',
              fontFamily: 'var(--sublabel-font)',
              color: subFg,
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              flexShrink: 'var(--sublabel-flex-shrink)',
            }}
          >
            {item.sublabel}
          </span>
        )}
      </div>

      {item.accessories && item.accessories.length > 0 && (
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            flexShrink: 0,
          }}
        >
          {item.accessories.map((acc, i) => (
            <span
              key={i}
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: 4,
                fontSize: 'var(--accessory-font-size)',
                fontFamily: 'var(--accessory-font)',
                color: acc.color ?? subFg,
                background: 'var(--accessory-bg)',
                border: 'var(--accessory-border-width) solid var(--accessory-border)',
                padding: 'var(--accessory-padding)',
                borderRadius: 'var(--accessory-radius)',
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
        <span
          style={{
            fontSize: 'var(--label-font-size)',
            color: subFg,
            flexShrink: 0,
            lineHeight: '1.3',
          }}
        >
          ›
        </span>
      )}
    </div>
  )
})

ResultRow.displayName = 'ResultRow'
export default ResultRow
