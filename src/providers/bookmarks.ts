import type { Command, CommandProvider } from '../types'
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

// Bookmarks live as a sub-folder inside Tools; the folder's children are these
// item commands (App.tsx wires the folder). Each item also stays in the flat
// search via its folderName tag.
export async function loadBookmarkCommands(): Promise<Command[]> {
  const bookmarks = await listBookmarks()
  return bookmarks.map(bookmarkCommand)
}

export const bookmarksProvider: CommandProvider = {
  id: 'bookmarks',
  name: 'Bookmarks',
  priority: 10,
  getCommands: loadBookmarkCommands,
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
