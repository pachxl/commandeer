import { forwardRef } from 'react'

interface SearchInputProps {
  value: string
  placeholder: string
  loading: boolean
  onChange: (value: string) => void
}

interface SliderInputProps {
  value: number
  min: number
  max: number
  step: number
  onChange: (value: number) => void
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

export const SliderInput = ({ value, min, max, step, onChange }: SliderInputProps) => {
  const percent = ((value - min) / (max - min)) * 100

  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 10,
      padding: '8px 14px',
      borderBottom: '1px solid var(--border)',
    }}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" style={{ flexShrink: 0, color: 'var(--text-dim)' }}>
        <path d="M12 4.5C7 4.5 2.73 7.61 1 12c1.73 4.39 6 7.5 11 7.5s9.27-3.11 11-7.5c-1.73-4.39-6-7.5-11-7.5zM12 17c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5zm0-8c-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3-1.34-3-3-3z" stroke="currentColor" strokeWidth="2" />
      </svg>
      <div style={{ flex: 1, height: 4, position: 'relative' }}>
        <div style={{
          position: 'absolute',
          left: 0,
          right: 0,
          top: 0,
          bottom: 0,
          background: 'var(--border)',
          borderRadius: '2px',
        }} />
        <div style={{
          position: 'absolute',
          left: 0,
          width: `${percent}%`,
          top: 0,
          bottom: 0,
          background: 'var(--accent)',
          borderRadius: '2px',
        }} />
        <input
          type="range"
          value={value}
          min={min}
          max={max}
          step={step}
          onChange={e => onChange(parseFloat(e.target.value))}
          style={{
            position: 'absolute',
            left: 0,
            right: 0,
            top: 0,
            bottom: 0,
            width: '100%',
            height: '100%',
            opacity: 0,
            cursor: 'pointer',
            zIndex: 1,
          }}
        />
      </div>
      <span style={{
        color: 'var(--text)',
        fontSize: 14,
        fontFamily: 'var(--font)',
        minWidth: 40,
        textAlign: 'right'
      }}>
        {value}%
      </span>
    </div>
  )
}

export default SearchInput
