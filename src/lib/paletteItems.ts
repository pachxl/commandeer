// Command → PaletteItem construction for the command palette.
//
// Pure, JSX-free helpers extracted from Palette.tsx: they turn the root
// Command list into the PaletteItem rows the palette ranks and renders.

import type { Command, PaletteItem } from '../types'

// Extra search terms (folder name, keywords, aliases) folded into the
// fuzzy-match text
export function searchTextFor(cmd: Command, prefix?: string): string | undefined {
  if (!prefix && !cmd.keywords?.length && !cmd.aliases?.length) return undefined
  return [prefix, cmd.label, cmd.description, ...(cmd.keywords ?? []), ...(cmd.aliases ?? [])].filter(Boolean).join(' ')
}

export function commandToItem(cmd: Command): PaletteItem {
  return {
    id: cmd.id,
    label: cmd.label,
    sublabel: cmd.isFolder ? undefined : cmd.description,
    icon: cmd.icon,
    iconPath: cmd.iconPath,
    isFolder: cmd.isFolder,
    source: cmd.source,
    actionLabel: cmd.actionLabel,
    searchText: searchTextFor(cmd),
    keywords: cmd.keywords,
    data: cmd.data ?? cmd.id,
    color: cmd.color,
    accessories: cmd.accessories,
    metadata: cmd.metadata,
    liveOutputKey: cmd.liveOutputKey,
    running: cmd.running,
    detailMarkdown: cmd.detailMarkdown,
  }
}

// Hierarchical root view: folders first, then root scripts
export function commandsToItems(commands: Command[]): PaletteItem[] {
  return commands.map(commandToItem)
}

// Flat view for cross-folder search: all scripts with folder as sublabel + searchText
export function commandsToFlatItems(commands: Command[]): PaletteItem[] {
  return commands.map(cmd => ({
    ...commandToItem(cmd),
    sublabel: cmd.folderName,
    searchText: searchTextFor(cmd, cmd.folderName),
  }))
}

// Fallback commands shown when a root query matches nothing — so the palette
// is never a dead end. Injected into the results list (keyboard-navigable,
// unlike a static empty-state message). Web/GitHub open a browser; "files"
// hands off to the @find mode so the query keeps refining in-place.
export function buildFallbackItems(query: string): PaletteItem[] {
  const q = query.trim()
  if (!q) return []
  const data = (kind: string) => ({ kind, q }) as unknown
  return [
    { id: 'fallback:web', label: `Search the web for “${q}”`, icon: 'search', source: 'builtin', data: data('web'), actionLabel: 'Open' },
    { id: 'fallback:files', label: `Search files for “${q}”`, icon: 'folder', source: 'builtin', data: data('files'), actionLabel: 'Search' },
    { id: 'fallback:github', label: `Search GitHub for “${q}”`, icon: 'search', source: 'builtin', data: data('github'), actionLabel: 'Open' },
  ]
}
