// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import SearchInput from './SearchInput'

const baseProps = {
  value: '',
  placeholder: 'Search commands...',
  loading: false,
  onChange: vi.fn(),
}

describe('SearchInput compact state', () => {
  afterEach(cleanup)

  it('shows the configured hotkey only in an unpreviewed compact capsule', () => {
    const { container, rerender } = render(<SearchInput {...baseProps} compact hotkeyHint="⌘ ⇧ Space" />)

    expect(container.querySelector('[data-search-shell]')?.hasAttribute('data-compact')).toBe(true)
    expect(screen.getByLabelText('Palette shortcut ⌘ ⇧ Space').textContent).toBe('⌘ ⇧ Space')

    rerender(<SearchInput {...baseProps} compact hotkeyHint="⌘ ⇧ Space" preview={{ label: '42', copy: '42' }} />)
    expect(container.querySelector('[data-search-hotkey]')).toBeNull()

    rerender(<SearchInput {...baseProps} hotkeyHint="⌘ ⇧ Space" />)
    expect(container.querySelector('[data-search-shell]')?.hasAttribute('data-compact')).toBe(false)
    expect(container.querySelector('[data-search-hotkey]')).toBeNull()
  })

  it('engages from a pointer press while preserving controlled input changes', () => {
    const onEngage = vi.fn()
    const onChange = vi.fn()
    render(<SearchInput {...baseProps} compact onEngage={onEngage} onChange={onChange} />)

    const input = screen.getByRole('textbox')
    fireEvent.pointerDown(input)
    expect(onEngage).toHaveBeenCalledOnce()

    fireEvent.change(input, { target: { value: 'files' } })
    expect(onChange).toHaveBeenCalledWith('files')
  })
})
