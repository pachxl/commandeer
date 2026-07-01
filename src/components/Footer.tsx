interface FooterProps {
  gameMode: boolean
  onToggleGameMode: () => void
  claudeUsageVisible: boolean
  onToggleClaudeUsage: () => void
}

export default function Footer({
  gameMode,
  onToggleGameMode,
  claudeUsageVisible,
  onToggleClaudeUsage,
}: FooterProps) {
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
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
        <kbd style={kbdStyle}>↵</kbd>
        <span>open</span>
        <span style={{ opacity: 0.4, margin: '0 4px' }}>·</span>
        <kbd style={kbdStyle}>esc</kbd>
        <span>close</span>
      </div>

      <div style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
        <button
          onClick={onToggleClaudeUsage}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 5,
            background: 'transparent',
            border: 'none',
            color: claudeUsageVisible ? 'var(--accent)' : 'var(--text-dim)',
            fontSize: 11,
            fontFamily: 'var(--font-ui)',
            cursor: 'pointer',
            padding: '2px 6px',
            borderRadius: 4,
          }}
          title="Toggle Claude usage"
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M12 20v-6M6 20V10M18 20V4" />
          </svg>
          <span>Claude {claudeUsageVisible ? 'On' : 'Off'}</span>
        </button>

        <button
          onClick={onToggleGameMode}
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 5,
            background: 'transparent',
            border: 'none',
            color: gameMode ? '#9ece6a' : 'var(--text-dim)',
            fontSize: 11,
            fontFamily: 'var(--font-ui)',
            cursor: 'pointer',
            padding: '2px 6px',
            borderRadius: 4,
          }}
          title="Toggle Game Mode (Ctrl+G)"
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <line x1="6" y1="11" x2="10" y2="11" />
            <line x1="8" y1="9" x2="8" y2="13" />
            <line x1="15" y1="12" x2="15.01" y2="12" />
            <line x1="18" y1="10" x2="18.01" y2="10" />
            <path d="M17.32 5H6.68a4 4 0 0 0-3.978 3.59c-.006.052-.01.101-.017.152C2.604 9.416 2 14.456 2 16a3 3 0 0 0 3 3c1 0 1.5-.5 2-1l1.414-1.414A2 2 0 0 1 9.828 16h4.344a2 2 0 0 1 1.414.586L17 18c.5.5 1 1 2 1a3 3 0 0 0 3-3c0-1.545-.604-6.584-.685-7.258A4 4 0 0 0 17.32 5z" />
          </svg>
          <span>Game Mode {gameMode ? 'On' : 'Off'}</span>
        </button>
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
