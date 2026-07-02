import type { Command, CommandProvider } from '../types'
import { getRates, openUrl } from '../lib/tauri'
import { evaluateSmart, type CurrencyRates } from '../lib/math'
import { tryColor } from '../lib/color'
import { calculatorCommand } from './tools'

// FX rates for currency conversions. Fetched once (the Rust side caches them
// for a day and serves offline), kept in module scope so search() stays sync
// and fast. The first currency query before rates land simply shows no result;
// the next keystroke picks up the cache.
let ratesCache: CurrencyRates | null = null
let ratesPending = false

export function currencyRates(): CurrencyRates | undefined {
  if (ratesCache) return ratesCache
  if (!ratesPending) {
    ratesPending = true
    getRates()
      .then(r => { ratesCache = r })
      .catch(() => { /* offline with no cache: currency stays disabled */ })
  }
  return undefined
}

export const calculatorProvider: CommandProvider = {
  id: 'calculator',
  name: 'Calculator',
  priority: 10,
  getCommands: (): Command[] => [calculatorCommand],
  search: async (query: string): Promise<Command[]> => {
    const trimmed = query.trim()
    if (!trimmed) return []

    const color = tryColor(trimmed)
    if (color) {
      return [
        {
          id: 'calculator:color',
          label: color.label,
          description: color.sublabel,
          icon: 'calculator',
          source: 'calculator',
          color: color.color,
          keywords: [query],
          action: async () => {
            await navigator.clipboard.writeText(color.copyValue)
          },
        },
        {
          id: 'calculator:google',
          label: `Google "${query}"`,
          description: 'Search this expression on Google',
          icon: 'search',
          source: 'calculator',
          keywords: [query],
          action: async () => {
            await openUrl(`https://www.google.com/search?q=${encodeURIComponent(query)}`)
          },
        },
      ]
    }

    const result = evaluateSmart(query, currencyRates())
    if (result === null) return []

    const formatted = result.label
    return [
      {
        id: 'calculator:result',
        label: formatted,
        description: result.sublabel ?? 'Copy result',
        icon: 'calculator',
        source: 'calculator',
        keywords: [query],
        action: async () => {
          await navigator.clipboard.writeText(formatted)
        },
      },
      {
        id: 'calculator:google',
        label: `Google "${query}"`,
        description: 'Search this expression on Google',
        icon: 'search',
        source: 'calculator',
        keywords: [query],
        action: async () => {
          await openUrl(`https://www.google.com/search?q=${encodeURIComponent(query)}`)
        },
      },
    ]
  },
}
