import { useEffect, useRef, useState } from 'react'
import type { FormField } from '../types'

interface FormViewProps {
  fields: FormField[]
  values: Record<string, unknown>
  onChange: (id: string, value: unknown) => void
  onSubmit: () => void
  submitLabel?: string
}

export default function FormView({ fields, values, onChange, onSubmit, submitLabel = 'Submit' }: FormViewProps) {
  const [focusedIndex, setFocusedIndex] = useState(0)
  const inputRefs = useRef<(HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement | HTMLButtonElement | null)[]>(
    [],
  )

  useEffect(() => {
    const el = inputRefs.current[focusedIndex]
    if (el) el.focus()
  }, [focusedIndex])

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.target instanceof HTMLTextAreaElement) {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault()
        onSubmit()
      }
      // A textarea owns Enter and arrow keys. Move focus with Tab, or submit
      // directly with Ctrl/Cmd+Enter.
      return
    }
    if (e.key === 'ArrowDown' || (e.key.toLowerCase() === 'n' && e.ctrlKey)) {
      e.preventDefault()
      setFocusedIndex(i => Math.min(i + 1, fields.length))
      return
    }
    if (e.key === 'ArrowUp' || (e.key.toLowerCase() === 'p' && e.ctrlKey)) {
      e.preventDefault()
      setFocusedIndex(i => Math.max(0, i - 1))
      return
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      if (focusedIndex === fields.length) {
        onSubmit()
      } else {
        setFocusedIndex(i => Math.min(i + 1, fields.length))
      }
      return
    }
  }

  return (
    <div
      data-form-view
      style={{
        flex: 1,
        minHeight: 0,
        overflowY: 'auto',
        padding: 'var(--form-padding)',
        display: 'flex',
        flexDirection: 'column',
        gap: 12,
        scrollbarWidth: 'thin',
        scrollbarColor: 'var(--border-strong) transparent',
      }}
      onKeyDown={handleKeyDown}
    >
      {fields.map((field, i) => {
        const focused = i === focusedIndex
        return (
          <div key={field.id} style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            <label
              htmlFor={`form-field-${field.id}`}
              style={{
                fontSize: 11,
                fontFamily: 'var(--font-ui)',
                color: 'var(--text-dim)',
                fontWeight: 500,
              }}
            >
              {field.label}
            </label>
            {field.type === 'text' && (
              <input
                id={`form-field-${field.id}`}
                ref={el => (inputRefs.current[i] = el)}
                type="text"
                value={String(values[field.id] ?? field.defaultValue ?? '')}
                placeholder={field.placeholder}
                onChange={e => onChange(field.id, e.target.value)}
                onFocus={() => setFocusedIndex(i)}
                style={{
                  background: 'var(--form-field-bg)',
                  border: `1px solid ${focused ? 'var(--accent)' : 'var(--border)'}`,
                  borderRadius: 'var(--form-field-radius)',
                  padding: '6px 10px',
                  color: 'var(--text)',
                  fontSize: 14,
                  fontFamily: 'var(--font)',
                  outline: 'none',
                }}
                spellCheck={false}
                autoComplete="off"
              />
            )}
            {field.type === 'textarea' && (
              <textarea
                id={`form-field-${field.id}`}
                ref={el => (inputRefs.current[i] = el)}
                value={String(values[field.id] ?? field.defaultValue ?? '')}
                placeholder={field.placeholder}
                onChange={e => onChange(field.id, e.target.value)}
                onFocus={() => setFocusedIndex(i)}
                rows={6}
                style={{
                  background: 'var(--form-field-bg)',
                  border: `1px solid ${focused ? 'var(--accent)' : 'var(--border)'}`,
                  borderRadius: 'var(--form-field-radius)',
                  padding: '8px 10px',
                  color: 'var(--text)',
                  fontSize: 13,
                  fontFamily: 'var(--font)',
                  lineHeight: 1.45,
                  outline: 'none',
                  resize: 'vertical',
                  minHeight: 96,
                }}
                spellCheck={false}
                autoComplete="off"
              />
            )}
            {field.type === 'dropdown' && (
              <select
                id={`form-field-${field.id}`}
                ref={el => (inputRefs.current[i] = el)}
                value={String(values[field.id] ?? field.defaultValue ?? '')}
                onChange={e => onChange(field.id, e.target.value)}
                onFocus={() => setFocusedIndex(i)}
                style={{
                  background: 'var(--form-field-bg)',
                  border: `1px solid ${focused ? 'var(--accent)' : 'var(--border)'}`,
                  borderRadius: 'var(--form-field-radius)',
                  padding: '6px 10px',
                  color: 'var(--text)',
                  fontSize: 14,
                  fontFamily: 'var(--font)',
                  outline: 'none',
                }}
              >
                {field.options?.map(opt => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            )}
            {field.type === 'checkbox' && (
              <label
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 8,
                  cursor: 'pointer',
                  padding: '4px 0',
                }}
              >
                <input
                  id={`form-field-${field.id}`}
                  ref={el => (inputRefs.current[i] = el as HTMLInputElement)}
                  type="checkbox"
                  checked={Boolean(values[field.id] ?? field.defaultValue ?? false)}
                  onChange={e => onChange(field.id, e.target.checked)}
                  onFocus={() => setFocusedIndex(i)}
                  style={{ accentColor: 'var(--accent)' }}
                />
                <span style={{ fontSize: 13, fontFamily: 'var(--font)', color: 'var(--text)' }}>
                  {field.placeholder ?? 'Enable'}
                </span>
              </label>
            )}
            {field.description && (
              <span
                style={{
                  color: 'var(--text-dim)',
                  fontSize: 11,
                  fontFamily: 'var(--font-ui)',
                  lineHeight: 1.4,
                }}
              >
                {field.description}
              </span>
            )}
          </div>
        )
      })}
      <button
        ref={el => (inputRefs.current[fields.length] = el)}
        onClick={onSubmit}
        onFocus={() => setFocusedIndex(fields.length)}
        style={{
          marginTop: 4,
          padding: '8px 12px',
          borderRadius: 'var(--form-field-radius)',
          border: `1px solid ${focusedIndex === fields.length ? 'var(--accent)' : 'var(--border)'}`,
          background: focusedIndex === fields.length ? 'var(--accent)' : 'var(--form-field-bg)',
          color: focusedIndex === fields.length ? 'var(--bg)' : 'var(--text)',
          fontSize: 13,
          fontFamily: 'var(--font-ui)',
          fontWeight: 500,
          cursor: 'pointer',
        }}
      >
        {submitLabel}
      </button>
    </div>
  )
}
