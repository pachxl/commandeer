// Kill Process: browsable command (grouped by executable name, sorted by
// memory) plus the "kill <name>" inline shortcut handled in Palette.
import type { Command, PaletteItem, Step } from '../types'
import { killProcess, listProcesses } from '../lib/tauri'

export interface ProcessGroup {
  name: string
  pids: number[]
  memory: number
  exePath: string | null
}

function formatMemory(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`
  return `${Math.round(bytes / (1024 * 1024))} MB`
}

export async function loadProcessGroups(): Promise<ProcessGroup[]> {
  const procs = await listProcesses()
  const groups = new Map<string, ProcessGroup>()
  for (const p of procs) {
    const key = p.name.toLowerCase()
    const group = groups.get(key) ?? { name: p.name, pids: [], memory: 0, exePath: null }
    group.pids.push(p.pid)
    group.memory += p.memory_bytes
    group.exePath ??= p.exe_path
    groups.set(key, group)
  }
  return [...groups.values()].sort((a, b) => b.memory - a.memory)
}

export function processGroupToItem(g: ProcessGroup): PaletteItem {
  return {
    id: `process:${g.name.toLowerCase()}`,
    label: g.name,
    sublabel: `${g.pids.length > 1 ? `${g.pids.length} processes · ` : ''}${formatMemory(g.memory)}`,
    icon: 'trash',
    iconPath: g.exePath ?? undefined,
    data: g,
    actionLabel: 'Kill',
  }
}

// Row for the "kill <name>" inline shortcut at root. Matching runs against the
// bare process name (searchText), not the "Kill " label.
export function killShortcutItem(g: ProcessGroup): PaletteItem {
  return {
    id: `process:kill:${g.name.toLowerCase()}`,
    label: `Kill ${g.name}`,
    sublabel: `${g.pids.length > 1 ? `${g.pids.length} processes · ` : ''}${formatMemory(g.memory)}`,
    icon: 'trash',
    iconPath: g.exePath ?? undefined,
    searchText: g.name,
    data: g,
    actionLabel: 'Kill',
  }
}

export async function killAll(pids: number[]): Promise<void> {
  const errors: string[] = []
  for (const pid of pids) {
    try {
      await killProcess(pid)
    } catch (err) {
      errors.push(String(err))
    }
  }
  // Only fail if nothing could be killed (some pids exit between list and kill)
  if (errors.length === pids.length && errors.length > 0) throw new Error(errors[0])
}

function killProcessStep(): Step {
  return {
    id: 'process:kill',
    label: 'Kill Process',
    placeholder: 'Search processes to kill...',
    load: async () => (await loadProcessGroups()).map(processGroupToItem),
    onSelect: async (item) => {
      await killAll((item.data as ProcessGroup).pids)
      return { type: 'done' }
    },
  }
}

export const killProcessCommand: Command = {
  id: 'builtin:kill-process',
  label: 'Kill Process',
  description: 'Terminate a running process',
  icon: 'trash',
  keywords: ['kill', 'process', 'task', 'terminate', 'end'],
  createRootStep: () => killProcessStep(),
}
