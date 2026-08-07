export type PathPlatform = 'unix' | 'windows'

export interface ParsedPaths {
  paths: string[]
  invalid: string[]
}

function unquote(value: string): string {
  if (value.length < 2) return value
  const first = value[0]
  const last = value[value.length - 1]
  return (first === '"' && last === '"') || (first === "'" && last === "'") ? value.slice(1, -1).trim() : value
}

/**
 * Keep configured filesystem paths predictable without rewriting separators or
 * case. Tilde expansion would depend on a shell (the Rust scanner does not use
 * one), so only fully-qualified paths are accepted.
 */
export function normalizeAbsolutePath(raw: string, platform: PathPlatform): string | null {
  const value = unquote(raw.trim())
  if (!value || value.includes('\0')) return null

  const absolute =
    platform === 'windows'
      ? /^[a-zA-Z]:[\\/]/.test(value) || /^(?:\\\\|\/\/)[^\\/]+[\\/][^\\/]+/.test(value)
      : value.startsWith('/')
  return absolute ? value : null
}

/** Parse one absolute search root per line, dropping blanks and duplicates. */
export function parseSearchRoots(raw: string, platform: PathPlatform): ParsedPaths {
  const paths: string[] = []
  const invalid: string[] = []
  const seen = new Set<string>()

  for (const line of raw.split(/\r?\n/)) {
    if (!line.trim()) continue
    const path = normalizeAbsolutePath(line, platform)
    if (!path) {
      invalid.push(line.trim())
      continue
    }
    const key = platform === 'windows' ? path.toLowerCase() : path
    if (!seen.has(key)) {
      seen.add(key)
      paths.push(path)
    }
  }

  return { paths, invalid }
}
