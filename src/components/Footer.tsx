import type { PaletteItem } from '../types'
import { getIconSvg, hasIcon } from './Icon'

interface FooterProps {
  selectedItem: PaletteItem | null
  primaryAction: string | null
  onOpenSettings?: () => void
  settingsVisible?: boolean
  gameModeEnabled?: boolean
  onToggleGameMode?: () => void
}

// Raycast-style footer: primary action + footer controls.
export default function Footer({
  selectedItem,
  primaryAction,
  onOpenSettings,
  settingsVisible,
  gameModeEnabled,
  onToggleGameMode,
}: FooterProps) {
  const icon = selectedItem?.icon ?? ''
  const isDataUrl = icon.startsWith('data:')
  const isNamedIcon = hasIcon(icon)

  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: 'var(--footer-padding)',
      borderTop: '1px solid var(--divider)',
      background: 'var(--footer-bg)',
      fontSize: 'var(--footer-font-size)',
      fontFamily: 'var(--footer-font)',
      color: 'var(--text-dim)',
      userSelect: 'none',
      minHeight: 'var(--footer-height)',
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
              : isNamedIcon
                ? <div dangerouslySetInnerHTML={{ __html: getIconSvg(icon, 'var(--text-dim)', 14) ?? '' }} style={{ display: 'flex' }} />
                : icon
            }
          </div>
        )}
        {primaryAction && (
          <div style={{
            display: 'var(--footer-primary-left-display)',
            alignItems: 'center',
            gap: 6,
            minWidth: 0,
          }}>
            <span style={{
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              color: 'var(--footer-primary-fg)',
            }}>
              {primaryAction}
            </span>
            <kbd style={kbdStyle}>↵</kbd>
          </div>
        )}
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
        {primaryAction && (
          <div style={{
            display: 'var(--footer-primary-right-display)',
            alignItems: 'center',
            gap: 6,
            color: 'var(--footer-primary-fg)',
            whiteSpace: 'nowrap',
          }}>
            <span>{primaryAction}</span>
            <kbd style={kbdStyle}>↵</kbd>
          </div>
        )}
        {primaryAction && (onToggleGameMode || settingsVisible) && (
          <span style={{
            display: 'var(--footer-primary-right-display)',
            width: 1,
            height: 14,
            background: 'var(--divider)',
          }} />
        )}
        {onToggleGameMode && (
          <button
            type="button"
            onClick={onToggleGameMode}
            title={gameModeEnabled ? 'Disable game mode' : 'Enable game mode'}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 4,
              padding: 'var(--footer-button-padding)',
              borderRadius: 'var(--footer-button-radius)',
              border: 'none',
              background: 'transparent',
              color: gameModeEnabled ? 'var(--accent)' : 'var(--text-dim)',
              fontFamily: 'var(--footer-font)',
              fontSize: 'var(--footer-font-size)',
              cursor: 'pointer',
              flexShrink: 0,
              outline: 'none',
              boxShadow: 'none',
              WebkitAppearance: 'none',
            }}
            onMouseEnter={e => {
              e.currentTarget.style.background = 'var(--footer-hover-bg)'
              e.currentTarget.style.color = gameModeEnabled ? 'var(--accent)' : 'var(--text)'
            }}
            onMouseLeave={e => {
              e.currentTarget.style.background = 'transparent'
              e.currentTarget.style.color = gameModeEnabled ? 'var(--accent)' : 'var(--text-dim)'
            }}
          >
            <div style={{ width: 14, height: 14, display: 'flex' }}>
              <div dangerouslySetInnerHTML={{ __html: getIconSvg('gamepad', 'currentColor', 14) ?? '' }} style={{ display: 'flex' }} />
            </div>
            <span>{gameModeEnabled ? 'Game On' : 'Game'}</span>
          </button>
        )}

        {settingsVisible && (
          <button
            type="button"
            onClick={onOpenSettings}
            title="Settings (Ctrl+,)"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 4,
              padding: 'var(--footer-button-padding)',
              borderRadius: 'var(--footer-button-radius)',
              border: 'none',
              background: 'transparent',
              color: 'var(--text-dim)',
              fontFamily: 'var(--footer-font)',
              fontSize: 'var(--footer-font-size)',
              cursor: 'pointer',
              flexShrink: 0,
              outline: 'none',
              boxShadow: 'none',
              WebkitAppearance: 'none',
            }}
            onMouseEnter={e => {
              e.currentTarget.style.background = 'var(--footer-hover-bg)'
              e.currentTarget.style.color = 'var(--text)'
            }}
            onMouseLeave={e => {
              e.currentTarget.style.background = 'transparent'
              e.currentTarget.style.color = 'var(--text-dim)'
            }}
          >
            <div style={{ width: 14, height: 14, display: 'flex' }}>
              <div dangerouslySetInnerHTML={{ __html: getIconSvg('settings', 'currentColor', 14) ?? '' }} style={{ display: 'flex' }} />
            </div>
            <span>Settings</span>
            <kbd style={kbdStyle}>,</kbd>
          </button>
        )}
      </div>
    </div>
  )
}

const kbdStyle: React.CSSProperties = {
  fontFamily: 'var(--font-ui)',
  fontSize: 'var(--kbd-font-size)',
  padding: 'var(--kbd-padding)',
  borderRadius: 'var(--kbd-radius)',
  background: 'var(--kbd-bg)',
  border: '1px solid var(--kbd-border)',
  color: 'var(--kbd-fg)',
  boxShadow: 'var(--kbd-shadow)',
}
