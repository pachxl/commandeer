import { forwardRef } from 'react'

interface SearchInputProps {
  value: string
  placeholder: string
  loading: boolean
  onChange: (value: string) => void
}

const SearchInput = forwardRef<HTMLInputElement, SearchInputProps>(
  ({ value, placeholder, loading, onChange }, ref) => {
    return (
      <div style={{
        display: 'flex',
        alignItems: 'center',
        gap: 8,
        padding: '9px 12px',
        borderBottom: '1px solid var(--border)',
        background: 'var(--bg-tab)',
      }}>
        {loading ? (
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" style={{ flexShrink: 0, color: 'var(--accent)' }}>
            <circle cx="7" cy="7" r="5" stroke="currentColor" strokeWidth="1.5"
              strokeDasharray="16" strokeLinecap="round"
              style={{ animation: 'spin 0.7s linear infinite', transformOrigin: 'center' }} />
          </svg>
        ) : (
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none" style={{ flexShrink: 0, color: 'var(--text-dim)' }}>
            <circle cx="6" cy="6" r="4" stroke="currentColor" strokeWidth="1.5" />
            <path d="M9.5 9.5L12 12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        )}
        <input
          ref={ref}
          type="text"
          value={value}
          placeholder={placeholder}
          onChange={e => onChange(e.target.value)}
          style={{
            flex: 1,
            background: 'transparent',
            border: 'none',
            outline: 'none',
            color: 'var(--text)',
            fontSize: 13,
            fontFamily: 'var(--font)',
            caretColor: 'var(--accent)',
            lineHeight: '20px',
          }}
          spellCheck={false}
          autoComplete="off"
        />
      </div>
    )
  }
)

SearchInput.displayName = 'SearchInput'
export default SearchInput
