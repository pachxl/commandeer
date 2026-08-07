import Icon from './Icon'

interface PaletteStatePanelProps {
  kind: 'loading' | 'empty' | 'error'
  title: string
  message?: string
}

export default function PaletteStatePanel({ kind, title, message }: PaletteStatePanelProps) {
  const icon = kind === 'loading' ? 'refresh' : kind === 'error' ? 'x' : 'search'
  const color = kind === 'error' ? '#f7768e' : 'var(--text-dim)'

  return (
    <div
      data-palette-state={kind}
      role={kind === 'error' ? 'alert' : 'status'}
      style={{
        minHeight: 86,
        padding: '18px 20px',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 11,
        color,
        borderBottom: '1px solid var(--divider)',
      }}
    >
      <div className={kind === 'loading' ? 'palette-state-spin' : undefined} style={{ display: 'flex' }}>
        <Icon name={icon} width={18} height={18} color={color} />
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 3, minWidth: 0 }}>
        <span style={{ color: kind === 'error' ? color : 'var(--text)', fontSize: 13, fontWeight: 600 }}>{title}</span>
        {message && <span style={{ color: 'var(--text-dim)', fontSize: 11, lineHeight: 1.4 }}>{message}</span>}
      </div>
    </div>
  )
}
