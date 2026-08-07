import { beforeEach, describe, expect, it, vi } from 'vitest'

const mocks = vi.hoisted(() => ({
  getRates: vi.fn(),
  openUrl: vi.fn(),
}))

vi.mock('../lib/tauri', () => mocks)

describe('calculator currency rates', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.clearAllMocks()
  })

  it('retries after a transient initial failure', async () => {
    const rates = { base: 'USD', date: '2026-08-07', rates: { USD: 1, GBP: 0.75 } }
    mocks.getRates.mockRejectedValueOnce(new Error('offline')).mockResolvedValueOnce(rates)
    const { currencyRates } = await import('./calculator')

    expect(currencyRates()).toBeUndefined()
    await vi.waitFor(() => expect(mocks.getRates).toHaveBeenCalledTimes(1))
    await Promise.resolve()
    await Promise.resolve()

    expect(currencyRates()).toBeUndefined()
    await vi.waitFor(() => expect(mocks.getRates).toHaveBeenCalledTimes(2))
    await vi.waitFor(() => expect(currencyRates()).toEqual(rates))
  })

  it('coalesces concurrent requests while rates are loading', async () => {
    let resolveRates!: (rates: unknown) => void
    mocks.getRates.mockReturnValue(
      new Promise(resolve => {
        resolveRates = resolve
      }),
    )
    const { currencyRates } = await import('./calculator')

    expect(currencyRates()).toBeUndefined()
    expect(currencyRates()).toBeUndefined()
    expect(mocks.getRates).toHaveBeenCalledTimes(1)

    resolveRates({ base: 'USD', date: '2026-08-07', rates: { USD: 1 } })
    await vi.waitFor(() => expect(currencyRates()).toBeDefined())
  })
})
