// Query ranking for the command palette.
//
// Pure, JSX-free helpers extracted from Palette.tsx: user overrides (aliases &
// pins) and the weighted fuzzy ranking that orders root query results.

import { fuzzyScoreFieldsBatch } from './fuzzy'
import { frecencyBonus } from './frecency'
import type { CommandOverride } from './tauri'
import type { PaletteItem } from '../types'

export type Overrides = Record<string, CommandOverride>

// Fold a user alias into the item's search text and display metadata
export function applyOverride(item: PaletteItem, ov?: CommandOverride): PaletteItem {
  if (!ov?.alias) return item
  return {
    ...item,
    searchText: `${item.searchText ?? item.label} ${ov.alias}`,
  }
}

// Alias-prefix matches sort above everything else; shorter aliases win ties.
export function aliasPrefixRank(query: string, ov?: CommandOverride): { tier: number; len: number } | null {
  if (!ov?.alias) return null
  const alias = ov.alias.toLowerCase()
  const q = query.trim().toLowerCase()
  if (alias === q) return { tier: 0, len: alias.length }
  if (alias.startsWith(q)) return { tier: 1, len: alias.length }
  return null
}

// Weighted fields for multi-field fuzzy scoring: label is the strongest signal,
// sublabel weaker, and the full search text (description, folder, keywords)
// weakest — enough to surface a match without outranking label hits.
const RANK_FIELDS = [
  { text: (item: PaletteItem) => item.label, weight: 1.0 },
  { text: (item: PaletteItem) => item.sublabel, weight: 0.5 },
  { text: (item: PaletteItem) => item.searchText, weight: 0.35 },
]

// Root query results: scripts and provider results ranked together by weighted
// fuzzy score, hard bonuses for exact/prefix label matches, alias matches,
// pins, and frecency. Alias-prefix matches are hoisted above everything.
// Array.sort is stable, so ties preserve input order (no row flicker).
export function buildQueryResults(items: PaletteItem[], query: string, overrides: Overrides): PaletteItem[] {
  const q = query.trim().toLowerCase()
  const baseScores = fuzzyScoreFieldsBatch(items, query, RANK_FIELDS)
  const ranked = items
    .map(item => {
      const baseScore = baseScores.get(item)
      if (baseScore === undefined) return null
      let score = baseScore
      const label = item.label.toLowerCase()
      if (label === q) score += 300
      else if (label.startsWith(q)) score += 120
      else if (label.includes(q)) score += 40

      const ov = overrides[item.id]
      const alias = ov?.alias?.toLowerCase()
      if (alias) {
        if (alias === q) score += 200
        else if (alias.startsWith(q)) score += 80
        else if (alias.includes(q)) score += 25
      }

      score += frecencyBonus(item.id)
      if (ov?.pinned) score += 10

      return {
        item,
        score,
        aliasRank: aliasPrefixRank(query, ov),
        // Scripts/shortcuts from the commands folder always sort above
        // provider results (calculator, kill, …)
        scriptTier: item.source === 'script' ? 0 : 1,
      }
    })
    .filter((r): r is NonNullable<typeof r> => r !== null)
  ranked.sort((a, b) => {
    if (a.aliasRank && b.aliasRank) {
      if (a.aliasRank.tier !== b.aliasRank.tier) return a.aliasRank.tier - b.aliasRank.tier
      return a.aliasRank.len - b.aliasRank.len
    }
    if (a.aliasRank) return -1
    if (b.aliasRank) return 1
    if (a.scriptTier !== b.scriptTier) return a.scriptTier - b.scriptTier
    return b.score - a.score
  })
  return ranked.map(r => r.item)
}
