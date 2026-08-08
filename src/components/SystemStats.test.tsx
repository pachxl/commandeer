// @vitest-environment jsdom

import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import SystemStatsPanel, { systemStatColor, systemStatLevel } from './SystemStats'

const mocks = vi.hoisted(() => ({
  systemStats: vi.fn(),
}))

vi.mock('../hooks/useWindowFocused', () => ({
  useWindowFocused: () => true,
}))

vi.mock('../lib/tauri', () => ({
  IS_MAC: false,
  systemStats: mocks.systemStats,
}))

describe('SystemStatsPanel', () => {
  beforeEach(() => {
    mocks.systemStats.mockReset()
    mocks.systemStats.mockResolvedValue({
      cpu: 32,
      mem_used: 12 * 1024 ** 3,
      mem_total: 16 * 1024 ** 3,
      mem_percent: 78,
      gpu: 94,
    })
  })

  afterEach(cleanup)

  it('uses fixed semantic colors at the utilization thresholds', () => {
    expect(systemStatLevel(74)).toBe('healthy')
    expect(systemStatLevel(75)).toBe('warning')
    expect(systemStatLevel(90)).toBe('critical')
    expect(systemStatColor(0)).toBe('#30d158')
    expect(systemStatColor(75)).toBe('#ff9f0a')
    expect(systemStatColor(90)).toBe('#ff453a')
  })

  it('exposes each resource as a labelled progressbar with its semantic level', async () => {
    const { container } = render(<SystemStatsPanel />)

    await waitFor(() =>
      expect(screen.getByRole('progressbar', { name: 'CPU utilization' }).getAttribute('aria-valuenow')).toBe('32'),
    )

    expect(screen.getByRole('progressbar', { name: 'RAM utilization' }).getAttribute('aria-valuenow')).toBe('78')
    expect(screen.getByRole('progressbar', { name: 'GPU utilization' }).getAttribute('aria-valuenow')).toBe('94')
    expect(container.querySelector('[data-stat-cell="cpu"]')?.getAttribute('data-stat-level')).toBe('healthy')
    expect(container.querySelector('[data-stat-cell="ram"]')?.getAttribute('data-stat-level')).toBe('warning')
    expect(container.querySelector('[data-stat-cell="gpu"]')?.getAttribute('data-stat-level')).toBe('critical')
  })
})
