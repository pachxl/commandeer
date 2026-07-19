// Frecency (frequency + recency) tracking for command ranking.
//
// Frequency and recency are stored separately and combined when scored, so a
// frequently-used item never fully decays to zero and recent use still gets a
// strong boost. This matches Vicinae's two-term frecency model.

interface FrecencyEntry {
  visitCount: number
  lastVisitedAt: number
}

// Legacy single-decayed-weight entry, migrated on load.
interface LegacyFrecencyEntry {
  weight: number
  last: number
}

const STORAGE_KEY = 'commandeer:frecency'
const MAX_ENTRIES = 200
const SAVE_DEBOUNCE_MS = 50

let cache: Record<string, FrecencyEntry> | null = null
let pendingEntries: Record<string, FrecencyEntry> | null = null
let saveTimeout: ReturnType<typeof setTimeout> | null = null

function migrate(entries: Record<string, unknown>): Record<string, FrecencyEntry> {
  const out: Record<string, FrecencyEntry> = {}
  for (const [id, raw] of Object.entries(entries)) {
    const e = raw as Partial<FrecencyEntry & LegacyFrecencyEntry> | null
    if (!e) continue
    if (typeof e.visitCount === 'number' && typeof e.lastVisitedAt === 'number') {
      out[id] = { visitCount: e.visitCount, lastVisitedAt: e.lastVisitedAt }
    } else if (typeof e.weight === 'number' && typeof e.last === 'number') {
      // Migrate old decayed-weight format: visits inferred from the stored
      // weight, timestamp preserved so recency is not lost.
      out[id] = {
        visitCount: Math.max(1, Math.round(e.weight)),
        lastVisitedAt: e.last,
      }
    }
  }
  return out
}

function load(): Record<string, FrecencyEntry> {
  if (cache) return cache
  try {
    cache = migrate(JSON.parse(localStorage.getItem(STORAGE_KEY) ?? '{}'))
  } catch {
    cache = {}
  }
  return cache!
}

function doSave(entries: Record<string, FrecencyEntry>) {
  // Evict the weakest entries so dynamic ids (files, clipboard) can't grow unbounded
  const keys = Object.keys(entries)
  if (keys.length > MAX_ENTRIES) {
    keys
      .sort((a, b) => frecencyScoreInternal(entries[a]) - frecencyScoreInternal(entries[b]))
      .slice(0, keys.length - MAX_ENTRIES)
      .forEach(k => delete entries[k])
  }
  localStorage.setItem(STORAGE_KEY, JSON.stringify(entries))
}

function save(entries: Record<string, FrecencyEntry>) {
  // Debounce writes to avoid blocking the main thread on every keystroke
  pendingEntries = entries
  if (saveTimeout) clearTimeout(saveTimeout)
  saveTimeout = setTimeout(() => {
    if (pendingEntries !== null) {
      doSave(pendingEntries)
      pendingEntries = null
    }
    saveTimeout = null
  }, SAVE_DEBOUNCE_MS)
}

function daysSince(entry: FrecencyEntry, now = Date.now()): number {
  return (now - entry.lastVisitedAt) / (24 * 60 * 60 * 1000)
}

// Combined frecency boost: frequency + recency, capped.
//   freq    = 5 * ln(1 + visits * 0.1)
//   recency = 10 * exp(-daysSinceLastUse / 30)
//   boost   = min(25, freq + recency)
function frecencyScoreInternal(entry: FrecencyEntry, now = Date.now()): number {
  const freq = 5 * Math.log(1 + entry.visitCount * 0.1)
  const recency = 10 * Math.exp(-daysSince(entry, now) / 30)
  return Math.min(25, freq + recency)
}

export function recordUse(id: string) {
  const entries = load()
  const now = Date.now()
  const prev = entries[id]
  entries[id] = {
    visitCount: (prev?.visitCount ?? 0) + 1,
    lastVisitedAt: now,
  }
  // Merge with any pending entries from a previous debounced save
  if (pendingEntries !== null) {
    Object.assign(pendingEntries, entries)
  }
  save(entries)
}

// Public frecency boost, used for root suggestions and query ranking.
export function frecencyBonus(id: string): number {
  const entry = load()[id]
  return entry ? frecencyScoreInternal(entry) : 0
}
