// @vitest-environment jsdom

import { act, renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useWindowFocused } from './useWindowFocused'

type FocusHandler = (event: { payload: boolean }) => void

const windowMocks = vi.hoisted(() => ({
  isFocused: vi.fn<() => Promise<boolean>>(),
  onFocusChanged: vi.fn<(handler: FocusHandler) => Promise<() => void>>(),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => windowMocks,
}))

describe('useWindowFocused', () => {
  beforeEach(() => {
    windowMocks.isFocused.mockReset()
    windowMocks.onFocusChanged.mockReset()
  })

  it('defaults to hidden until the native focus state is known', async () => {
    const unlisten = vi.fn()
    windowMocks.onFocusChanged.mockResolvedValue(unlisten)
    windowMocks.isFocused.mockResolvedValue(true)

    const { result } = renderHook(() => useWindowFocused())

    expect(result.current).toBe(false)
    await waitFor(() => expect(result.current).toBe(true))
  })

  it('does not let an initial state read overwrite a newer focus event', async () => {
    let handler: FocusHandler | undefined
    let resolveInitial!: (focused: boolean) => void
    windowMocks.onFocusChanged.mockImplementation(callback => {
      handler = callback
      return Promise.resolve(vi.fn())
    })
    windowMocks.isFocused.mockReturnValue(
      new Promise(resolve => {
        resolveInitial = resolve
      }),
    )

    const { result } = renderHook(() => useWindowFocused())
    await waitFor(() => expect(windowMocks.isFocused).toHaveBeenCalled())
    act(() => handler?.({ payload: true }))
    expect(result.current).toBe(true)

    await act(async () => resolveInitial(false))
    expect(result.current).toBe(true)
  })

  it('unsubscribes when registration completes after unmount', async () => {
    let resolveRegistration!: (unlisten: () => void) => void
    const unlisten = vi.fn()
    windowMocks.onFocusChanged.mockReturnValue(
      new Promise(resolve => {
        resolveRegistration = resolve
      }),
    )
    windowMocks.isFocused.mockResolvedValue(true)

    const { unmount } = renderHook(() => useWindowFocused())
    unmount()
    await act(async () => resolveRegistration(unlisten))

    expect(unlisten).toHaveBeenCalledOnce()
    expect(windowMocks.isFocused).not.toHaveBeenCalled()
  })
})
