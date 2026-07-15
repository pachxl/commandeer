import { getIconSvg, hasIcon } from './Icon'

interface HudOverlayProps {
  message: string
  icon?: string
}

// Full-cover confirmation pill shown after an action (copy/paste/…): the
// palette container is position:relative, so this sits over the (about-to-be-
// hidden) body and reads as a single floating HUD.
export default function HudOverlay({ message, icon }: HudOverlayProps) {
  return (
    <div
      style={{
        position: 'absolute',
        inset: 0,
        zIndex: 200,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'var(--bg)',
        backdropFilter: 'blur(60px) saturate(180%)',
        WebkitBackdropFilter: 'blur(60px) saturate(180%)',
        borderRadius: 'inherit',
      }}
    >
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          padding: '14px 22px',
          borderRadius: 12,
          background: 'var(--bg-elevated, rgba(36,40,59,0.9))',
          border: '1px solid var(--border)',
          boxShadow: '0 8px 30px rgba(0,0,0,0.35)',
          color: 'var(--text)',
          fontSize: 15,
          fontFamily: 'var(--font)',
        }}
      >
        {icon && hasIcon(icon) && (
          <div
            style={{
              width: 20,
              height: 20,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              color: '#9ece6a',
            }}
            dangerouslySetInnerHTML={{ __html: getIconSvg(icon, '#9ece6a', 18) ?? '' }}
          />
        )}
        {message}
      </div>
    </div>
  )
}
