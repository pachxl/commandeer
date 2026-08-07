// @vitest-environment jsdom

import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useInlineScripts, type InlineScript } from './useInlineScripts'

type FocusHandler = (event: { payload: boolean }) => void

const mocks = vi.hoisted(() => ({
  isFocused: vi.fn<() => Promise<boolean>>(),
  onFocusChanged: vi.fn<(handler: FocusHandler) => Promise<() => void>>(),
  runScriptCapture: vi.fn<(path: string) => Promise<string>>(),
}))

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => mocks,
}))

vi.mock('../lib/tauri', () => ({
  runScriptCapture: mocks.runScriptCapture,
}))

const scripts: InlineScript[] = [{ path: '/tmp/status.sh', refreshSeconds: 1 }]

describe('useInlineScripts lifecycle', () => {
  let focusHandler: FocusHandler | undefined

  beforeEach(() => {
    vi.useFakeTimers()
    vi.spyOn(Math, 'random').mockReturnValue(0)
    mocks.isFocused.mockReset().mockResolvedValue(false)
    mocks.onFocusChanged.mockReset().mockImplementation(handler => {
      focusHandler = handler
      return Promise.resolve(vi.fn())
    })
    mocks.runScriptCapture.mockReset().mockResolvedValue('ready')
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('does not run scripts while hidden and seeds only once on focus', async () => {
    renderHook(() => useInlineScripts(scripts))
    await act(async () => {})

    act(() => vi.advanceTimersByTime(10_000))
    expect(mocks.runScriptCapture).not.toHaveBeenCalled()

    act(() => focusHandler?.({ payload: true }))
    await act(async () => vi.advanceTimersByTime(0))
    expect(mocks.runScriptCapture).toHaveBeenCalledTimes(1)

    await act(async () => vi.advanceTimersByTime(999))
    expect(mocks.runScriptCapture).toHaveBeenCalledTimes(1)
    await act(async () => vi.advanceTimersByTime(1))
    expect(mocks.runScriptCapture).toHaveBeenCalledTimes(2)
  })
})
