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
        gap: 10,
        padding: '8px 14px',
        borderBottom: '1px solid var(--border)',
      }}>
        {loading ? (
          <svg width="16" height="16" viewBox="0 0 14 14" fill="none" style={{ flexShrink: 0, color: 'var(--text-dim)' }}>
            <circle cx="7" cy="7" r="5" stroke="currentColor" strokeWidth="1.5"
              strokeDasharray="16" strokeLinecap="round"
              style={{ animation: 'spin 0.7s linear infinite', transformOrigin: 'center' }} />
          </svg>
        ) : (
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" style={{ flexShrink: 0, color: 'var(--text-dim)' }}>
            <circle cx="10.5" cy="10.5" r="6.5" stroke="currentColor" strokeWidth="2" />
            <path d="M15.5 15.5L20 20" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
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
            fontSize: 15,
            fontWeight: 400,
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
