// @vitest-environment jsdom

import { cleanup, render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import OnixOpticalShell from './OnixOpticalShell'

interface MediaPreferences {
  reducedMotion?: boolean
  reducedTransparency?: boolean
  forcedColors?: boolean
}

function installMatchMedia(preferences: MediaPreferences = {}) {
  Object.defineProperty(window, 'matchMedia', {
    configurable: true,
    value: vi.fn((query: string) => ({
      matches:
        (query.includes('reduced-motion') && preferences.reducedMotion === true) ||
        (query.includes('reduced-transparency') && preferences.reducedTransparency === true) ||
        (query.includes('forced-colors') && preferences.forcedColors === true),
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(() => true),
    })),
  })
}

class ResizeObserverMock {
  observe() {}
  unobserve() {}
  disconnect() {}
}

describe('OnixOpticalShell', () => {
  beforeEach(() => {
    installMatchMedia()
    vi.stubGlobal('ResizeObserver', ResizeObserverMock)
    vi.stubGlobal(
      'requestAnimationFrame',
      vi.fn(() => 1),
    )
    vi.stubGlobal('cancelAnimationFrame', vi.fn())
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(null)
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('fails closed to the CSS optical material without WebGL2', async () => {
    const { container } = render(
      <div style={{ position: 'relative', width: 700, height: 72 }}>
        <OnixOpticalShell expanded={false} />
      </div>,
    )

    const shell = container.querySelector<HTMLElement>('[data-onix-optical-shell]')
    expect(shell).not.toBeNull()
    expect(shell?.getAttribute('aria-hidden')).toBe('true')
    expect(shell?.style.pointerEvents).toBe('none')
    expect(shell?.dataset.onixOptics).toBe('css')
    expect(container.querySelector('[data-onix-css-material]')).not.toBeNull()
  })

  it('uses the opaque accessibility material for reduced transparency', async () => {
    installMatchMedia({ reducedTransparency: true })
    const { container } = render(
      <div style={{ position: 'relative', width: 700, height: 420 }}>
        <OnixOpticalShell expanded />
      </div>,
    )

    const shell = container.querySelector<HTMLElement>('[data-onix-optical-shell]')
    const material = container.querySelector<HTMLElement>('[data-onix-css-material]')
    expect(shell?.dataset.onixReducedTransparency).toBe('true')
    expect(shell?.dataset.onixOptics).toBe('css')
    expect(material?.style.background).toBe('rgb(6, 7, 11)')
    expect(container.querySelector('[data-onix-css-caustic]')).toBeNull()
  })

  it('marks reduced motion and removes shape transitions', async () => {
    installMatchMedia({ reducedMotion: true })
    const { container } = render(
      <div style={{ position: 'relative', width: 700, height: 72 }}>
        <OnixOpticalShell expanded={false} />
      </div>,
    )

    const shell = container.querySelector<HTMLElement>('[data-onix-optical-shell]')
    expect(shell?.dataset.onixReducedMotion).toBe('true')
    expect(shell?.style.transition).toBe('none')
    expect(container.querySelector('[data-onix-morph-guard]')).toBeNull()
  })

  it('covers newly exposed native glass only while the panel blooms', async () => {
    const { container, rerender } = render(
      <div style={{ position: 'relative', width: 700, height: 72 }}>
        <OnixOpticalShell expanded={false} />
      </div>,
    )
    expect(container.querySelector('[data-onix-morph-guard]')).toBeNull()

    rerender(
      <div style={{ position: 'relative', width: 700, height: 420 }}>
        <OnixOpticalShell expanded />
      </div>,
    )

    const guard = container.querySelector<HTMLElement>('[data-onix-morph-guard]')
    expect(guard).not.toBeNull()
    expect(guard?.style.borderRadius).toBe('inherit')
    expect(guard?.style.animation).toContain('180ms')
  })
})
