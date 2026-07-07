import type { ConfirmOptions } from '../lib/confirm'

interface ConfirmOverlayProps {
  options: ConfirmOptions
  remember: boolean
  focus: 'confirm' | 'cancel'
  onToggleRemember: () => void
  onResolve: (ok: boolean) => void
}

// Modal confirmation shown over the palette body. Keyboard is driven by
// Palette's handler (Enter/Esc/←/→, R to toggle remember); clicks here mirror
// those. Rendered above the action panel so it owns interaction while pending.
export default function ConfirmOverlay({
  options, remember, focus, onToggleRemember, onResolve,
}: ConfirmOverlayProps) {
  const confirmBg = focus === 'confirm'
    ? (options.danger ? '#f7768e' : 'var(--accent)')
    : 'transparent'
  const confirmFg = focus === 'confirm' ? '#ffffff' : (options.danger ? '#f7768e' : 'var(--text)')

  return (
    <div style={{
      position: 'absolute',
      inset: 0,
      zIndex: 300,
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      background: 'var(--bg)',
      backdropFilter: 'blur(60px) saturate(180%)',
      WebkitBackdropFilter: 'blur(60px) saturate(180%)',
      borderRadius: 'inherit',
      padding: 20,
    }}>
      <div style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 14,
        width: '100%',
        maxWidth: 360,
        padding: 18,
        borderRadius: 12,
        background: 'var(--bg-elevated, rgba(36,40,59,0.92))',
        border: '1px solid var(--border)',
        boxShadow: '0 10px 40px rgba(0,0,0,0.4)',
      }}>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
          <span style={{ fontSize: 15, fontFamily: 'var(--font)', color: 'var(--text)', fontWeight: 600 }}>
            {options.message}
          </span>
          {options.detail && (
            <span style={{ fontSize: 12, fontFamily: 'var(--font)', color: 'var(--text-dim)' }}>
              {options.detail}
            </span>
          )}
        </div>

        {options.key && (
          <div
            onClick={onToggleRemember}
            style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: 'pointer', userSelect: 'none' }}
          >
            <span style={{
              width: 15, height: 15, borderRadius: 4, flexShrink: 0,
              border: '1px solid var(--border-strong, var(--border))',
              background: remember ? 'var(--accent)' : 'transparent',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              color: '#fff', fontSize: 11, lineHeight: '15px',
            }}>
              {remember ? '✓' : ''}
            </span>
            <span style={{ fontSize: 12, fontFamily: 'var(--font)', color: 'var(--text-dim)' }}>
              Don't ask again
            </span>
          </div>
        )}

        <div style={{ display: 'flex', gap: 8, justifyContent: 'flex-end' }}>
          <button
            onClick={() => onResolve(false)}
            style={{
              padding: '6px 14px', borderRadius: 6, cursor: 'pointer',
              fontSize: 13, fontFamily: 'var(--font)',
              background: focus === 'cancel' ? 'var(--bg-select, rgba(122,162,247,0.14))' : 'transparent',
              border: '1px solid var(--border)',
              color: 'var(--text)',
            }}
          >
            {options.cancelLabel ?? 'Cancel'}
          </button>
          <button
            onClick={() => onResolve(true)}
            style={{
              padding: '6px 14px', borderRadius: 6, cursor: 'pointer',
              fontSize: 13, fontFamily: 'var(--font)', fontWeight: 600,
              background: confirmBg,
              border: `1px solid ${options.danger ? '#f7768e' : 'var(--accent)'}`,
              color: confirmFg,
            }}
          >
            {options.confirmLabel ?? 'Confirm'}
          </button>
        </div>
      </div>
    </div>
  )
}
