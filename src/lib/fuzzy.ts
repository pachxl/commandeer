import { Fzf } from 'fzf'

export interface FuzzyMatch {
  score: number
  // Indices into `text` of the characters that matched the query, for highlighting
  positions: number[]
}

export function fuzzyMatch(query: string, text: string): FuzzyMatch | null {
  if (!query) return { score: 0, positions: [] }
  const fzf = new Fzf([text], { casing: 'case-insensitive' })
  const results = fzf.find(query)
  if (results.length === 0) return null
  const r = results[0]
  return { score: r.score, positions: [...r.positions].sort((a, b) => a - b) }
}

export interface ScoredItem<T> {
  item: T
  score: number
}

// A weighted field used for multi-field fuzzy ranking. The highest weighted
// score across all fields is used as the item's score.
export interface FuzzyField<T> {
  text: (item: T) => string | undefined
  weight: number
}

// Score a whole list against a set of weighted fields in one pass, returning a
// map of item → best weighted field score. Builds a single Fzf index per field
// across all items (not one per item), so ranking N items over F fields costs F
// index builds instead of N×F. This is the hot path on every keystroke, so the
// batched form matters — the per-item variant below is a thin wrapper for
// callers that only have one item.
export function fuzzyScoreFieldsBatch<T>(items: T[], query: string, fields: FuzzyField<T>[]): Map<T, number> {
  const best = new Map<T, number>()

  // Fast path for empty query: all items get score 0
  if (!query) {
    for (const item of items) best.set(item, 0)
    return best
  }

  // Fast path for single-character queries: use simple includes check instead of Fzf
  // This is much faster for very short queries where fuzzy matching adds little value
  if (query.length === 1) {
    const q = query.toLowerCase()
    for (const item of items) {
      let maxScore = 0
      for (const field of fields) {
        const text = field.text(item)
        if (text && text.toLowerCase().includes(q)) {
          maxScore = Math.max(maxScore, field.weight * 100)
        }
      }
      best.set(item, maxScore)
    }
    return best
  }

  for (const field of fields) {
    const fieldItems = items
      .map(item => ({ item, text: field.text(item) }))
      .filter((x): x is { item: T; text: string } => !!x.text)
    if (fieldItems.length === 0) continue
    const fzf = new Fzf(fieldItems, { selector: x => x.text, casing: 'case-insensitive' })
    for (const r of fzf.find(query)) {
      const weighted = r.score * field.weight
      const existing = best.get(r.item.item)
      if (existing === undefined || weighted > existing) {
        best.set(r.item.item, weighted)
      }
    }
  }
  return best
}

// Like fuzzyFilterScored but supports a list of weighted fields.
function fuzzyFilterFields<T>(items: T[], query: string, fields: FuzzyField<T>[]): ScoredItem<T>[] {
  if (!query) return items.map(item => ({ item, score: 0 }))
  const best = fuzzyScoreFieldsBatch(items, query, fields)
  return items
    .map(item => {
      const score = best.get(item)
      return score === undefined ? null : { item, score }
    })
    .filter((r): r is ScoredItem<T> => r !== null)
}

function fuzzyFilterScoredSingle<T>(items: T[], query: string, getText: (item: T) => string): ScoredItem<T>[] {
  if (!query) return items.map(item => ({ item, score: 0 }))
  const wrapped = items.map(item => ({ item, text: getText(item) }))
  const fzf = new Fzf(wrapped, { selector: x => x.text, casing: 'case-insensitive' })
  return fzf.find(query).map(r => ({ item: r.item.item, score: r.score }))
}

// Like fuzzyFilter but returns scores so callers can blend in other signals
// (e.g. frecency) before sorting. Accepts either a single text getter or a
// list of weighted fields.
export function fuzzyFilterScored<T>(items: T[], query: string, getText: (item: T) => string): ScoredItem<T>[]
export function fuzzyFilterScored<T>(items: T[], query: string, fields: FuzzyField<T>[]): ScoredItem<T>[]
export function fuzzyFilterScored<T>(
  items: T[],
  query: string,
  getTextOrFields: ((item: T) => string) | FuzzyField<T>[],
): ScoredItem<T>[] {
  if (Array.isArray(getTextOrFields)) {
    return fuzzyFilterFields(items, query, getTextOrFields)
  }
  return fuzzyFilterScoredSingle(items, query, getTextOrFields)
}

export function fuzzyFilter<T>(items: T[], query: string, getText: (item: T) => string): T[]
export function fuzzyFilter<T>(items: T[], query: string, fields: FuzzyField<T>[]): T[]
export function fuzzyFilter<T>(
  items: T[],
  query: string,
  getTextOrFields: ((item: T) => string) | FuzzyField<T>[],
): T[] {
  if (!query) return items
  const scored = Array.isArray(getTextOrFields)
    ? fuzzyFilterScored(items, query, getTextOrFields as FuzzyField<T>[])
    : fuzzyFilterScored(items, query, getTextOrFields as (item: T) => string)
  return scored.sort((a, b) => b.score - a.score).map(r => r.item)
}

// Lightweight substring-based position finder for highlighting only.
// Much faster than Fzf for this use case since we only need character positions,
// not fuzzy scoring. Used by ResultsGrid for highlight rendering.
export function getHighlightPositions(query: string, text: string): number[] {
  if (!query) return []
  const q = query.toLowerCase()
  const t = text.toLowerCase()
  const positions: number[] = []
  let idx = -1
  while (true) {
    idx = t.indexOf(q, idx + 1)
    if (idx === -1) break
    for (let i = 0; i < q.length; i++) {
      positions.push(idx + i)
    }
  }
  return positions
}
