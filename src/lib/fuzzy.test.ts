import { describe, expect, it } from 'vitest'
import { fuzzyFilter, fuzzyScoreFieldsBatch } from './fuzzy'

interface Item {
  title: string
  alias?: string
}

const fields = [
  { text: (item: Item) => item.title, weight: 1 },
  { text: (item: Item) => item.alias, weight: 2 },
]

describe('weighted fuzzy matching', () => {
  it('excludes non-matches from the single-character fast path', () => {
    const items = [{ title: 'Alpha' }, { title: 'Beta' }, { title: 'Clock' }]

    expect(fuzzyFilter(items, 'a', fields)).toEqual([items[0], items[1]])
  })

  it('matches a single character case-insensitively', () => {
    const items = [{ title: 'Alpha' }, { title: 'Beta' }]

    expect(fuzzyFilter(items, 'A', fields)).toEqual(items)
  })

  it('uses the highest matching field weight for short queries', () => {
    const titleMatch = { title: 'Calculator' }
    const aliasMatch = { title: 'System information', alias: 'Calculator' }
    const scores = fuzzyScoreFieldsBatch([titleMatch, aliasMatch], 'c', fields)

    expect(scores.get(titleMatch)).toBe(100)
    expect(scores.get(aliasMatch)).toBe(200)
    expect(fuzzyFilter([titleMatch, aliasMatch], 'c', fields)).toEqual([aliasMatch, titleMatch])
  })
})
