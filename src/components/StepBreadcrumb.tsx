import type { Step } from '../types'

interface StepBreadcrumbProps {
  steps: Step[]
}

export default function StepBreadcrumb({ steps }: StepBreadcrumbProps) {
  return (
    <div style={{
      display: 'flex',
      alignItems: 'center',
      padding: '2px 14px 4px',
      gap: 4,
    }}>
      {steps.map((step, i) => (
        <span key={step.id} style={{ display: 'flex', alignItems: 'center', gap: 4 }}>
          {i > 0 && <span style={{ color: 'var(--text-dim)', fontSize: 'var(--breadcrumb-font-size)' }}>›</span>}
          <span style={{
            fontSize: 'var(--breadcrumb-font-size)',
            color: i === steps.length - 1 ? 'var(--accent)' : 'var(--text-dim)',
          }}>
            {step.label}
          </span>
        </span>
      ))}
    </div>
  )
}
