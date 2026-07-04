import type { Command, CommandProvider, PaletteItem, Step, StepResult } from '../types'
import { listBookmarks, openUrl, type Bookmark } from '../lib/tauri'

function bookmarkCommand(bookmark: Bookmark): Command {
  return {
    id: `bookmark:${bookmark.url}`,
    label: bookmark.name,
    description: `${bookmark.browser} · ${bookmark.url}`,
    icon: 'bookmark',
    source: 'bookmark' as const,
    folderName: 'Bookmarks',
    keywords: [bookmark.name, bookmark.url, bookmark.browser],
    actionLabel: 'Open',
    data: bookmark,
    action: async () => {
      await openUrl(bookmark.url)
    },
  }
}

function bookmarksStep(): Step {
  return {
    id: 'bookmarks:browse',
    label: 'Bookmarks',
    placeholder: 'Search bookmarks...',
    load: async (): Promise<PaletteItem[]> => {
      const bookmarks = await listBookmarks()
      return bookmarks.map(b => ({
        id: `bookmark:${b.url}`,
        label: b.name,
        sublabel: `${b.browser} · ${b.url}`,
        icon: 'bookmark',
        source: 'bookmark' as const,
        data: b,
        actionLabel: 'Open',
      }))
    },
    onSelect: async (item): Promise<StepResult> => {
      const bookmark = item.data as Bookmark
      await openUrl(bookmark.url)
      return { type: 'done' }
    },
  }
}

const BROWSE_COMMAND: Command = {
  id: 'bookmark:browse',
  label: 'Bookmarks',
  description: 'Search browser bookmarks',
  icon: 'bookmark',
  source: 'bookmark' as const,
  keywords: ['bookmark', 'browser', 'favorites'],
  actionLabel: 'Open',
  createRootStep: bookmarksStep,
}

export const bookmarksProvider: CommandProvider = {
  id: 'bookmarks',
  name: 'Bookmarks',
  priority: 10,
  getCommands: async () => {
    const bookmarks = await listBookmarks()
    if (bookmarks.length === 0) {
      return [BROWSE_COMMAND]
    }
    return [
      BROWSE_COMMAND,
      ...bookmarks.map(bookmarkCommand),
    ]
  },
  search: async (query: string): Promise<Command[]> => {
    const trimmed = query.trim().toLowerCase()
    if (!trimmed || trimmed.length < 2) return []
    const bookmarks = await listBookmarks()
    return bookmarks
      .filter(b =>
        b.name.toLowerCase().includes(trimmed) ||
        b.url.toLowerCase().includes(trimmed)
      )
      .map(bookmarkCommand)
  },
}
