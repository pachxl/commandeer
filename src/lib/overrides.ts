// Shared cache for user command overrides (aliases, pins) stored in
// overrides.json. Used by Palette (ranking, sections, actions) and by
// providers that resolve aliases (quicklink inline arguments).
import { readOverrides, writeOverrides, type CommandOverride } from './tauri'

let cache: Record<string, CommandOverride> | null = null

export async function getOverrides(): Promise<Record<string, CommandOverride>> {
  if (!cache) cache = await readOverrides()
  return cache
}

export function invalidateOverridesCache() {
  cache = null
}

export async function setOverride(id: string, patch: CommandOverride): Promise<void> {
  const all = { ...(await getOverrides()) }
  const next: CommandOverride = { ...all[id], ...patch }
  if (!next.alias) delete next.alias
  if (!next.pinned) delete next.pinned
  if (!next.hotkey) delete next.hotkey
  if (!next.showAtRoot) delete next.showAtRoot
  if (Object.keys(next).length === 0) delete all[id]
  else all[id] = next
  await writeOverrides(all)
  cache = all
}
