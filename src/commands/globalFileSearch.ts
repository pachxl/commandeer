// "find:" prefix search across the whole machine. Each (debounced) keystroke
// asks the backend for candidates — self-hosted FTS5 index first, then the
// Everything index, then a walkdir fallback — and the fzf-based ranking plus
// relevance multipliers below decide the final order.
import type { AppConfig, PaletteItem } from '../types'
import { searchFiles } from '../lib/tauri'
import { fuzzyFilterScored } from '../lib/fuzzy'

// Down-rank temp/backup/build files so real results float to the top.
const JUNK_PATTERNS: { pattern: RegExp; multiplier: number }[] = [
  { pattern: /\.(tmp|obj|pdb|o|swp|swo|swm)$/i, multiplier: 0.5 },
  { pattern: /\.crdownload$/i, multiplier: 0.3 },
  { pattern: /~$/, multiplier: 0.3 },
  { pattern: /#autosave#/, multiplier: 0.1 },
  { pattern: /\.#lock/, multiplier: 0.1 },
  { pattern: /(^|[/\\])~\$/, multiplier: 0.1 }, // Office lock files ~$foo.docx
  { pattern: /(^|[/\\])Thumbs\.db$/i, multiplier: 0.1 },
  { pattern: /(^|[/\\])desktop\.ini$/i, multiplier: 0.1 },
]

function fileRelevanceMultiplier(path: string): number {
  for (const { pattern, multiplier } of JUNK_PATTERNS) {
    if (pattern.test(path)) return multiplier
  }
  return 1
}

// Boost results whose filename contains the query verbatim, strongest when the
// match starts the name or a word within it.
function substringMatchMultiplier(query: string, filename: string): number {
  const q = query.toLowerCase()
  const f = filename.toLowerCase()
  const idx = f.indexOf(q)
  if (idx < 0) return 1
  if (idx === 0) return 1.5
  const prev = filename[idx - 1]
  if (' /\\-_.:(['.includes(prev)) return 1.5
  return 1.05
}

function rankFileItems(items: PaletteItem[], query: string): PaletteItem[] {
  const q = query.trim()
  if (!q) return items
  return fuzzyFilterScored(items, q, i => `${i.label} ${i.sublabel ?? ''}`)
    .map(r => {
      const relevance = fileRelevanceMultiplier(r.item.sublabel ?? '')
      const substring = substringMatchMultiplier(q, r.item.label)
      return { item: r.item, score: r.score * relevance * substring }
    })
    .sort((a, b) => b.score - a.score)
    .map(r => r.item)
}

export async function loadGlobalFileResults(query: string, config: AppConfig): Promise<PaletteItem[]> {
  if (!query.trim()) return []
  const results = await searchFiles(query, config.search_paths ?? [])
  const items = results.map(r => ({
    id: `file:${r.path}`,
    label: r.name,
    sublabel: r.path,
    icon: r.icon ?? 'file',
    data: r.path,
    actionLabel: 'Open',
  }))
  return rankFileItems(items, query)
}
