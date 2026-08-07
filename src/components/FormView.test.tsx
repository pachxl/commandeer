// @vitest-environment jsdom

import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import FormView from './FormView'

describe('FormView textarea', () => {
  it('keeps regular Enter in the textarea and submits with Ctrl+Enter', () => {
    const onSubmit = vi.fn()
    render(
      <FormView
        fields={[{ id: 'roots', label: 'Roots', type: 'textarea' }]}
        values={{ roots: '/Users/alex' }}
        onChange={vi.fn()}
        onSubmit={onSubmit}
        submitLabel="Save Roots"
      />,
    )

    const textarea = screen.getByRole('textbox', { name: 'Roots' })
    fireEvent.keyDown(textarea, { key: 'Enter' })
    expect(onSubmit).not.toHaveBeenCalled()

    fireEvent.keyDown(textarea, { key: 'Enter', ctrlKey: true })
    expect(onSubmit).toHaveBeenCalledOnce()
    expect(screen.getByRole('button', { name: 'Save Roots' })).toBeTruthy()
  })
})
