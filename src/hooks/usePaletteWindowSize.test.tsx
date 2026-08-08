// @vitest-environment jsdom

import { act, cleanup, render, waitFor } from '@testing-library/react'
import { usePaletteWindowSize } from './usePaletteWindowSize'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  setSize: vi.fn(),
  onFocusChanged: vi.fn(),
  envInfo: vi.fn(),
  resizePalette: vi.fn(),
  resizePaletteWindow: vi.fn(),
  recenterPalette: vi.fn(),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    setSize: mocks.setSize,
    onFocusChanged: mocks.onFocusChanged,
  }),
}))

vi.mock('@tauri-apps/api/dpi', () => ({
  LogicalSize: class LogicalSize {
    constructor(
      readonly width: number,
      readonly height: number,
    ) {}
  },
}))

// Keeping IS_LINUX true lets each test select Wayland or the ordinary
// setSize path through the mocked runtime environment.
vi.mock('../lib/tauri', () => ({
  IS_LINUX: true,
  IS_MAC: true,
  envInfo: mocks.envInfo,
  resizePalette: mocks.resizePalette,
  resizePaletteWindow: mocks.resizePaletteWindow,
  recenterPalette: mocks.recenterPalette,
}))

let measuredHeight = 66
let resizeObservers: TestResizeObserver[] = []

class TestResizeObserver {
  readonly observe = vi.fn()
  readonly disconnect = vi.fn()

  constructor(private readonly callback: ResizeObserverCallback) {
    resizeObservers.push(this)
  }

  trigger() {
    this.callback([], this as unknown as ResizeObserver)
  }
}

function rectWithHeight(height: number): DOMRect {
  return {
    x: 0,
    y: 0,
    width: 0,
    height,
    top: 0,
    right: 0,
    bottom: height,
    left: 0,
    toJSON: () => ({}),
  }
}

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void
  const promise = new Promise<T>(done => {
    resolve = done
  })
  return { promise, resolve }
}

function Harness({ scale = 1, width = 770, animate = false }: { scale?: number; width?: number; animate?: boolean }) {
  const { sizeRef, containerRef } = usePaletteWindowSize(scale, width, animate)
  return (
    <div ref={sizeRef} data-size-probe>
      <div ref={containerRef} />
    </div>
  )
}

describe('usePaletteWindowSize', () => {
  beforeEach(() => {
    measuredHeight = 66
    resizeObservers = []
    vi.clearAllMocks()
    vi.stubGlobal('ResizeObserver', TestResizeObserver)
    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function (this: HTMLElement) {
      return rectWithHeight(this.hasAttribute('data-size-probe') ? measuredHeight : 0)
    })
    mocks.envInfo.mockResolvedValue({ os: 'linux', wayland: false, desktop: '', home: '' })
    mocks.setSize.mockResolvedValue(undefined)
    mocks.resizePalette.mockResolvedValue(undefined)
    mocks.resizePaletteWindow.mockResolvedValue(undefined)
    mocks.recenterPalette.mockResolvedValue(undefined)
    mocks.onFocusChanged.mockResolvedValue(vi.fn())
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('serializes compact-to-expanded native sizes and coalesces queued geometry', async () => {
    const firstResize = deferred<void>()
    mocks.setSize.mockImplementationOnce(() => firstResize.promise).mockResolvedValue(undefined)
    render(<Harness />)

    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(1))
    expect(mocks.setSize.mock.calls[0][0]).toMatchObject({ width: 770, height: 66 })

    measuredHeight = 300
    act(() => resizeObservers[0].trigger())
    measuredHeight = 428
    act(() => resizeObservers[0].trigger())
    expect(mocks.setSize).toHaveBeenCalledTimes(1)

    await act(async () => {
      firstResize.resolve()
      await firstResize.promise
    })
    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(2))
    expect(mocks.setSize.mock.calls[1][0]).toMatchObject({ width: 770, height: 428 })
  })

  it('scales width, recenters only width changes, and keeps height updates anchored', async () => {
    const { rerender } = render(<Harness />)
    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(1))
    expect(mocks.recenterPalette).not.toHaveBeenCalled()

    rerender(<Harness scale={1.25} />)
    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(2))
    expect(mocks.setSize.mock.calls[1][0]).toMatchObject({ width: 963, height: 66 })
    await waitFor(() => expect(mocks.recenterPalette).toHaveBeenCalledOnce())

    measuredHeight = 420
    act(() => resizeObservers[0].trigger())
    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(3))
    expect(mocks.setSize.mock.calls[2][0]).toMatchObject({ width: 963, height: 420 })
    expect(mocks.recenterPalette).toHaveBeenCalledOnce()
  })

  it('uses the layer-shell resize command on Wayland', async () => {
    mocks.envInfo.mockResolvedValue({ os: 'linux', wayland: true, desktop: 'COSMIC', home: '/tmp' })
    measuredHeight = 432
    render(<Harness scale={0.8} />)

    await waitFor(() => expect(mocks.resizePalette).toHaveBeenCalledWith(432, 616))
    expect(mocks.setSize).not.toHaveBeenCalled()
    expect(mocks.recenterPalette).not.toHaveBeenCalled()
  })

  it('animates the macOS Onix expansion after the initial compact size', async () => {
    render(<Harness animate />)
    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(1))

    measuredHeight = 428
    act(() => resizeObservers[0].trigger())

    await waitFor(() => expect(mocks.resizePaletteWindow).toHaveBeenCalledWith(770, 428, true))
    expect(mocks.setSize).toHaveBeenCalledTimes(1)
    expect(mocks.recenterPalette).not.toHaveBeenCalled()
  })

  it('uses the direct resize path when reduced motion is requested', async () => {
    vi.stubGlobal(
      'matchMedia',
      vi.fn(() => ({ matches: true })),
    )
    render(<Harness animate />)
    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(1))

    measuredHeight = 428
    act(() => resizeObservers[0].trigger())

    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(2))
    expect(mocks.setSize.mock.calls[1][0]).toMatchObject({ width: 770, height: 428 })
    expect(mocks.resizePaletteWindow).not.toHaveBeenCalled()
  })

  it('reapplies on focus and cleans up a listener that registers after unmount', async () => {
    const registration = deferred<() => void>()
    const unlisten = vi.fn()
    let focusChanged: ((event: { payload: boolean }) => void) | undefined
    mocks.onFocusChanged.mockImplementation(callback => {
      focusChanged = callback
      return registration.promise
    })

    const { unmount } = render(<Harness />)
    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(1))
    expect(focusChanged).toBeDefined()

    act(() => focusChanged?.({ payload: false }))
    expect(mocks.setSize).toHaveBeenCalledTimes(1)
    act(() => focusChanged?.({ payload: true }))
    await waitFor(() => expect(mocks.setSize).toHaveBeenCalledTimes(2))

    unmount()
    expect(resizeObservers[0].disconnect).toHaveBeenCalledOnce()
    await act(async () => {
      registration.resolve(unlisten)
      await registration.promise
    })
    await waitFor(() => expect(unlisten).toHaveBeenCalledOnce())
  })
})
