// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import ClaudeUsage from './ClaudeUsage'

const mocks = vi.hoisted(() => ({
  claudeUsage: vi.fn(),
}))
const storage = new Map<string, string>()
const localStorageMock = {
  getItem: (key: string) => storage.get(key) ?? null,
  setItem: (key: string, value: string) => storage.set(key, value),
  removeItem: (key: string) => storage.delete(key),
  clear: () => storage.clear(),
  key: (index: number) => [...storage.keys()][index] ?? null,
  get length() {
    return storage.size
  },
} as Storage

vi.mock('../hooks/useWindowFocused', () => ({
  useWindowFocused: () => true,
}))

vi.mock('../lib/tauri', () => ({
  claudeUsage: mocks.claudeUsage,
}))

describe('ClaudeUsage', () => {
  beforeEach(() => {
    storage.clear()
    vi.stubGlobal('localStorage', localStorageMock)
    mocks.claudeUsage.mockReset()
    vi.spyOn(console, 'error').mockImplementation(() => {})
  })

  afterEach(() => {
    cleanup()
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('shows credential guidance without a contradictory empty state', async () => {
    mocks.claudeUsage.mockRejectedValue(
      "Claude Code's macOS login Keychain is locked. In Terminal, run security unlock-keychain ~/Library/Keychains/login.keychain-db and enter your Mac login password. Then run claude auth login --claudeai and reopen Commandeer.",
    )

    const { unmount } = render(<ClaudeUsage />)

    await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('security unlock-keychain'))
    expect(screen.queryByText('No usage data available.')).toBeNull()

    unmount()
    render(<ClaudeUsage />)
    expect(screen.getByRole('alert').textContent).toContain('security unlock-keychain')
    expect(screen.queryByText('No usage data available.')).toBeNull()
  })
})
