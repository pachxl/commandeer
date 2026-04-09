export function fuzzyScore(query: string, text: string): number | null {
  if (!query) return 0
  const q = query.toLowerCase()
  const t = text.toLowerCase()
  let qi = 0
  let score = 0
  let lastMatch = -1

  for (let ti = 0; ti < t.length && qi < q.length; ti++) {
    if (t[ti] === q[qi]) {
      score += 1
      if (lastMatch === ti - 1) score += 2          // contiguous bonus
      if (ti === 0 || t[ti - 1] === ' ' || t[ti - 1] === '/' || t[ti - 1] === '\\') {
        score += 3                                    // word boundary bonus
      }
      lastMatch = ti
      qi++
    }
  }

  return qi === q.length ? score : null
}

export function fuzzyFilter<T>(
  items: T[],
  query: string,
  getText: (item: T) => string,
): T[] {
  if (!query) return items
  return items
    .map(item => ({ item, score: fuzzyScore(query, getText(item)) }))
    .filter((r): r is { item: T; score: number } => r.score !== null)
    .sort((a, b) => b.score - a.score)
    .map(r => r.item)
}
