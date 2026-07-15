// @vitest-environment jsdom

import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { PaletteAction } from '../types'
import { usePaletteFeedback } from './usePaletteFeedback'

const hide = vi.fn<() => Promise<void>>(() => Promise.resolve())

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ hide }),
}))

describe('usePaletteFeedback lifecycle', () => {
  const dispatch = vi.fn<(action: PaletteAction) => void>()

  beforeEach(() => {
    dispatch.mockClear()
    hide.mockClear()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('settles a superseded confirmation as cancelled', async () => {
    const { result } = renderHook(() => usePaletteFeedback(dispatch))
    let first!: Promise<boolean>
    let second!: Promise<boolean>

    act(() => { first = result.current.requestConfirm({ message: 'First' }) })
    act(() => { second = result.current.requestConfirm({ message: 'Second' }) })

    await expect(first).resolves.toBe(false)
    act(() => { result.current.resolveConfirm(true) })
    await expect(second).resolves.toBe(true)
  })

  it('cancels a pending confirmation when the palette session resets', async () => {
    const { result } = renderHook(() => usePaletteFeedback(dispatch))
    let pending!: Promise<boolean>

    act(() => { pending = result.current.requestConfirm({ message: 'Delete it?' }) })
    act(() => { result.current.resetFeedback() })

    await expect(pending).resolves.toBe(false)
    expect(result.current.confirmReq).toBeNull()
  })

  it('cancels an old HUD timer so it cannot hide a reopened palette', () => {
    vi.useFakeTimers()
    const { result } = renderHook(() => usePaletteFeedback(dispatch))

    act(() => { result.current.showHud('Copied') })
    act(() => { result.current.resetFeedback() })
    act(() => { vi.advanceTimersByTime(1000) })

    expect(hide).not.toHaveBeenCalled()
    expect(dispatch).not.toHaveBeenCalledWith({ type: 'RESET' })
  })
})
