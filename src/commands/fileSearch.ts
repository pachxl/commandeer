// "search:" prefix search over the folder open in the previously-focused File
// Explorer window. The whole tree is loaded once (parallel walk in Rust,
// capped), then every keystroke filters client-side — no IPC while typing.
import type { PaletteItem } from '../types'
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
    // Swap in the real shell icon once a row becomes visible (cached per ext)
    iconPath: f.path,
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
