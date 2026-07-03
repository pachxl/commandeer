import { forwardRef } from 'react'
import Icon from './Icon'
import type { LivePreview } from '../types'

interface SearchInputProps {
  value: string
  placeholder: string
  loading: boolean
  onChange: (value: string) => void
  preview?: LivePreview | null
}

interface SliderInputProps {
  value: number
  min: number
  max: number
  step: number
  icon?: string
  onChange: (value: number) => void
}

const SearchInput = forwardRef<HTMLInputElement, SearchInputProps>(
  ({ value, placeholder, loading, onChange, preview }, ref) => {
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
          data-palette-search
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
            minWidth: 0,
          }}
          spellCheck={false}
          autoComplete="off"
        />
        {preview && (
          <div
            title={preview.sublabel ? `${preview.label} · ${preview.sublabel}` : preview.label}
            style={{
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'flex-end',
              flexShrink: 0,
              maxWidth: '45%',
              minWidth: 0,
              userSelect: 'none',
            }}
          >
            <span style={{
              color: 'var(--text)',
              fontSize: 15,
              fontWeight: 500,
              lineHeight: '20px',
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
              maxWidth: '100%',
            }}>
              {preview.label}
            </span>
            {preview.sublabel && (
              <span style={{
                color: 'var(--text-dim)',
                fontSize: 11,
                lineHeight: '14px',
                whiteSpace: 'nowrap',
                overflow: 'hidden',
                textOverflow: 'ellipsis',
                maxWidth: '100%',
              }}>
                {preview.sublabel}
              </span>
            )}
          </div>
        )}
      </div>
    )
  }
)

SearchInput.displayName = 'SearchInput'

export const SliderInput = ({ value, min, max, step, icon = 'eye', onChange }: SliderInputProps) => {
  const percent = ((value - min) / (max - min)) * 100

  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      gap: 10,
      padding: '8px 14px',
      borderBottom: '1px solid var(--border)',
    }}>
      <Icon name={icon} width={16} height={16} color="var(--text-dim)" />
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
