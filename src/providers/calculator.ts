import type { Command, CommandProvider } from '../types'
import { getRates, openUrl } from '../lib/tauri'
import { evaluateSmart, type CurrencyRates } from '../lib/math'

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
      .then(r => {
        ratesCache = r
      })
      .catch(() => {
        /* offline with no cache: retry on a later query */
      })
      .finally(() => {
        ratesPending = false
      })
  }
  return undefined
}

export interface CalcDisplay {
  display: string
  sublabel?: string
  copy: string
}

// One-stop evaluation for the @calc prefix and the Tools Calculator step:
// expressions/units/currency.
export function evaluateCalcQuery(query: string): CalcDisplay | null {
  const trimmed = query.trim()
  if (!trimmed) return null
  const result = evaluateSmart(trimmed, currencyRates())
  return result ? { display: result.label, sublabel: result.sublabel, copy: result.label } : null
}

export const calculatorProvider: CommandProvider = {
  id: 'calculator',
  name: 'Calculator',
  priority: 10,
  search: async (query: string): Promise<Command[]> => {
    const trimmed = query.trim()
    if (!trimmed) return []

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
