import { useEffect, useState } from 'react'

export type ToastKind = 'success' | 'error' | 'info'

export interface ToastMessage {
  id: number
  message: string
  kind: ToastKind
}

interface ToastProps {
  message: string
  kind: ToastKind
}

function kindColor(kind: ToastKind): string {
  switch (kind) {
    case 'success':
      return '#9ece6a'
    case 'error':
      return '#f7768e'
    case 'info':
      return 'var(--accent)'
  }
}

export default function Toast({ message, kind }: ToastProps) {
  const [visible, setVisible] = useState(false)

  useEffect(() => {
    const enter = requestAnimationFrame(() => setVisible(true))
    return () => cancelAnimationFrame(enter)
  }, [])

  return (
    <div
      data-toast
      style={{
        padding: '6px 12px',
        borderRadius: 6,
        background: 'var(--bg-elevated)',
        color: kind === 'info' ? 'var(--text)' : kindColor(kind),
        border: '1px solid var(--border)',
        fontSize: 13,
        fontFamily: 'var(--font)',
        boxShadow: '0 4px 20px rgba(0,0,0,0.18)',
        opacity: visible ? 1 : 0,
        transform: visible ? 'translateY(0)' : 'translateY(-8px)',
        transition: 'opacity 180ms ease, transform 180ms ease',
        pointerEvents: 'none',
        whiteSpace: 'nowrap',
        maxWidth: '90vw',
        overflow: 'hidden',
        textOverflow: 'ellipsis',
      }}
    >
      {message}
    </div>
  )
}

interface ToastContainerProps {
  toasts: ToastMessage[]
}

export function ToastContainer({ toasts }: ToastContainerProps) {
  if (toasts.length === 0) return null
  return (
    <div
      style={{
        position: 'absolute',
        top: 8,
        left: 0,
        right: 0,
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: 6,
        zIndex: 100,
        padding: '0 12px',
      }}
    >
      {toasts.map(t => (
        <Toast key={t.id} message={t.message} kind={t.kind} />
      ))}
    </div>
  )
}
