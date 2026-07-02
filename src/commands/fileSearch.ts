// Search files/folders in the folder open in the previously-focused File
// Explorer window. The whole tree is loaded once (parallel walk in Rust,
// capped), then every keystroke filters client-side — no IPC while typing.
import type { Command, PaletteItem, Step } from '../types'
import { explorerLocation, listFilesRecursive, openPath, type FileEntry } from '../lib/tauri'

// Enough for any normal folder; keeps the one-time IPC payload small enough
// to stay instant. The walk is breadth-first, so past the cap it's the
// deepest entries that become unsearchable, never shallow ones.
const FILE_CAP = 50000

function fileToItem(f: FileEntry): PaletteItem {
  return {
    id: `file:${f.path}`,
    label: f.name,
    sublabel: f.rel,
    icon: f.is_dir ? 'folder' : 'file',
    searchText: f.rel,
    data: f.path,
    actionLabel: 'Open',
  }
}

export async function loadActiveFolderItems(): Promise<PaletteItem[]> {
  const folder = await explorerLocation()
  if (!folder) throw new Error('No File Explorer folder is focused')
  const files = await listFilesRecursive(folder, FILE_CAP)
  return files.map(fileToItem)
}

export async function openFileItem(item: PaletteItem): Promise<void> {
  await openPath(item.data as string)
}

export const searchFolderCommand: Command = {
  id: 'builtin:find',
  label: 'Search Folder',
  description: 'Find files in the active Explorer folder',
  icon: 'search',
  keywords: ['find', 'file', 'search', 'folder', 'explorer'],
  actionLabel: 'Search',
  createRootStep: (): Step => ({
    id: 'find-step',
    label: 'Search Folder',
    placeholder: 'Search files in this folder...',
    load: async (): Promise<PaletteItem[]> => loadActiveFolderItems(),
    onSelect: async item => {
      await openFileItem(item)
      return { type: 'done' }
    },
  }),
}
