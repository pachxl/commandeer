import type { PaletteItem } from '../types'
import { getIconSvg, hasIcon } from './Icon'

interface FooterProps {
  selectedItem: PaletteItem | null
  primaryAction: string | null
  onOpenSettings?: () => void
  settingsVisible?: boolean
  inStep?: boolean
  gameModeEnabled?: boolean
  onToggleGameMode?: () => void
}

// Raycast-style footer: primary action + contextual keyboard hints.
export default function Footer({
  selectedItem,
  primaryAction,
  onOpenSettings,
  settingsVisible,
  inStep,
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
              : isNamedIcon
                ? <div dangerouslySetInnerHTML={{ __html: getIconSvg(icon, 'var(--text-dim)', 14) ?? '' }} style={{ display: 'flex' }} />
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

      <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexShrink: 0 }}>
        {selectedItem && (
          <span style={hintStyle}>
            <kbd style={kbdStyle}>Ctrl</kbd>
            <kbd style={kbdStyle}>K</kbd>
            <span style={{ marginLeft: 3 }}>Actions</span>
          </span>
        )}
        {onToggleGameMode && (
          <button
            type="button"
            onClick={onToggleGameMode}
            title={gameModeEnabled ? 'Disable game mode' : 'Enable game mode'}
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 3,
              background: 'transparent',
              border: 'none',
              padding: 0,
              color: gameModeEnabled ? 'var(--accent)' : 'var(--text-dim)',
              fontFamily: 'var(--font-ui)',
              fontSize: 11,
              cursor: 'pointer',
              boxShadow: 'none',
              WebkitAppearance: 'none',
            }}
          >
            <kbd style={kbdStyle}>Ctrl</kbd>
            <kbd style={kbdStyle}>G</kbd>
            <span>{gameModeEnabled ? 'Game On' : 'Game'}</span>
          </button>
        )}
        <span style={hintStyle}>
          <kbd style={kbdStyle}>Esc</kbd>
          <span style={{ marginLeft: 3 }}>{inStep ? 'Back' : 'Close'}</span>
        </span>

        {settingsVisible && (
          <button
            type="button"
            onClick={onOpenSettings}
            title="Settings"
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: 4,
              padding: '2px 8px',
              borderRadius: 4,
              border: 'none',
              background: 'transparent',
              color: 'var(--text-dim)',
              fontFamily: 'var(--font-ui)',
              fontSize: 11,
              cursor: 'pointer',
              flexShrink: 0,
              outline: 'none',
              boxShadow: 'none',
              WebkitAppearance: 'none',
            }}
            onMouseEnter={e => {
              e.currentTarget.style.background = 'rgba(255,255,255,0.06)'
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
          </button>
        )}
      </div>
    </div>
  )
}

const hintStyle: React.CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  gap: 3,
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
