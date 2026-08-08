// @vitest-environment jsdom

import { useRef } from 'react'
import { render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import SelectionLens from './SelectionLens'

interface Geometry {
  x: number
  y: number
  width: number
  height: number
}

let geometry: Geometry
let observers: TestResizeObserver[]

class TestResizeObserver {
  readonly observe = vi.fn()
  readonly disconnect = vi.fn()

  constructor(private readonly callback: ResizeObserverCallback) {
    observers.push(this)
  }

  trigger() {
    this.callback([], this as unknown as ResizeObserver)
  }
}

function Fixture({ active = true }: { active?: boolean }) {
  const containerRef = useRef<HTMLDivElement>(null)
  const targetRef = useRef<HTMLDivElement>(null)

  return (
    <div ref={containerRef} data-testid="surface">
      <div ref={targetRef} data-lens-test-target />
      <SelectionLens containerRef={containerRef} targetRef={targetRef} surface="list" active={active} />
    </div>
  )
}

describe('SelectionLens', () => {
  beforeEach(() => {
    geometry = { x: 12, y: 34, width: 240, height: 38 }
    observers = []
    vi.stubGlobal('ResizeObserver', TestResizeObserver)
    vi.spyOn(HTMLElement.prototype, 'offsetLeft', 'get').mockImplementation(function (this: HTMLElement) {
      return this.hasAttribute('data-lens-test-target') ? geometry.x : 0
    })
    vi.spyOn(HTMLElement.prototype, 'offsetTop', 'get').mockImplementation(function (this: HTMLElement) {
      return this.hasAttribute('data-lens-test-target') ? geometry.y : 0
    })
    vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockImplementation(function (this: HTMLElement) {
      return this.hasAttribute('data-lens-test-target') ? geometry.width : 0
    })
    vi.spyOn(HTMLElement.prototype, 'offsetHeight', 'get').mockImplementation(function (this: HTMLElement) {
      return this.hasAttribute('data-lens-test-target') ? geometry.height : 0
    })
  })

  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('publishes selected-item geometry through CSS variables and tracks resize', () => {
    const { container } = render(<Fixture />)
    const lens = container.querySelector<HTMLElement>('[data-selection-lens="list"]')
    expect(lens).not.toBeNull()
    expect(lens?.getAttribute('data-visible')).toBe('true')
    expect(lens?.style.getPropertyValue('--selection-lens-x')).toBe('12px')
    expect(lens?.style.getPropertyValue('--selection-lens-y')).toBe('34px')
    expect(lens?.style.getPropertyValue('--selection-lens-width')).toBe('240px')
    expect(lens?.style.getPropertyValue('--selection-lens-height')).toBe('38px')
    expect(observers).toHaveLength(1)
    expect(observers[0].observe).toHaveBeenCalledTimes(2)

    geometry = { x: 18, y: 90, width: 300, height: 42 }
    observers[0].trigger()
    expect(lens?.style.getPropertyValue('--selection-lens-x')).toBe('18px')
    expect(lens?.style.getPropertyValue('--selection-lens-y')).toBe('90px')
    expect(lens?.style.getPropertyValue('--selection-lens-width')).toBe('300px')
    expect(lens?.style.getPropertyValue('--selection-lens-height')).toBe('42px')
  })

  it('collapses and marks the lens inactive without intercepting input', () => {
    const { container, rerender } = render(<Fixture />)
    rerender(<Fixture active={false} />)

    const lens = container.querySelector<HTMLElement>('[data-selection-lens="list"]')
    expect(lens?.getAttribute('data-active')).toBe('false')
    expect(lens?.hasAttribute('data-visible')).toBe(false)
    expect(lens?.style.getPropertyValue('--selection-lens-width')).toBe('0px')
    expect(lens?.style.getPropertyValue('--selection-lens-height')).toBe('0px')
    expect(lens?.style.pointerEvents).toBe('none')
    expect(lens?.getAttribute('aria-hidden')).toBe('true')
  })
})
