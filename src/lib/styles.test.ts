// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest'
import { applyStyle, getAllStyles, getAppliedStyleName, getStyleName, UI_STYLE_CHANGE_EVENT } from './styles'

describe('UI styles', () => {
  afterEach(() => {
    applyStyle('Default')
    document.documentElement.removeAttribute('data-style')
    document.documentElement.removeAttribute('style')
  })

  it('applies Black Water variables and announces the selected style', () => {
    const listener = vi.fn()
    window.addEventListener(UI_STYLE_CHANGE_EVENT, listener)

    applyStyle('onIX')

    expect(document.documentElement.dataset.style).toBe('onix')
    expect(getAppliedStyleName()).toBe('Onix')
    expect(document.documentElement.style.getPropertyValue('--onix-material')).toBe('rgba(7, 9, 12, 0.82)')
    expect(document.documentElement.style.getPropertyValue('--onix-capsule-height')).toBe('66px')
    expect(document.documentElement.style.getPropertyValue('--row-selected-bg')).toBe('transparent')
    expect(listener).toHaveBeenCalledOnce()
    expect((listener.mock.calls[0][0] as CustomEvent<string>).detail).toBe('Onix')

    window.removeEventListener(UI_STYLE_CHANGE_EVENT, listener)
  })

  it('removes Onix-only variables and restores Default without touching theme colour', () => {
    document.documentElement.style.setProperty('--accent', 'hotpink')
    applyStyle('Onix')
    expect(document.documentElement.style.getPropertyValue('--onix-material-deep')).not.toBe('')

    applyStyle('Default')

    expect(document.documentElement.dataset.style).toBe('default')
    expect(getAppliedStyleName()).toBe('Default')
    expect(document.documentElement.style.getPropertyValue('--onix-material')).toBe('')
    expect(document.documentElement.style.getPropertyValue('--onix-material-deep')).toBe('')
    expect(document.documentElement.style.getPropertyValue('--row-padding')).toBe('4px 10px')
    expect(document.documentElement.style.getPropertyValue('--row-selected-bg')).toBe('var(--accent)')
    expect(document.documentElement.style.getPropertyValue('--accent')).toBe('hotpink')
  })

  it('keeps the public names stable and falls back to Default', () => {
    expect(getAllStyles().map(style => style.name)).toEqual(['Default', 'Onix'])
    expect(getStyleName(undefined)).toBe('Default')
    expect(getStyleName('not-a-style')).toBe('Default')

    const listener = vi.fn()
    window.addEventListener(UI_STYLE_CHANGE_EVENT, listener)
    applyStyle('not-a-style')
    expect(document.documentElement.dataset.style).toBe('default')
    expect((listener.mock.calls[0][0] as CustomEvent<string>).detail).toBe('Default')
    window.removeEventListener(UI_STYLE_CHANGE_EVENT, listener)
  })
})
