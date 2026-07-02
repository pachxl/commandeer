// Kill Process: browsable command (grouped by executable name, sorted by
// memory) plus "kill <name>" inline results at root via the provider search.
import type { Command, CommandProvider, PaletteItem, Step } from '../types'
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

function groupSublabel(g: ProcessGroup): string {
  return `${g.pids.length > 1 ? `${g.pids.length} processes · ` : ''}${formatMemory(g.memory)}`
}

export function processGroupToItem(g: ProcessGroup): PaletteItem {
  return {
    id: `process:${g.name.toLowerCase()}`,
    label: g.name,
    sublabel: groupSublabel(g),
    icon: 'trash',
    iconPath: g.exePath ?? undefined,
    source: 'system',
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
  source: 'system',
  keywords: ['kill', 'process', 'task', 'terminate', 'end'],
  createRootStep: () => killProcessStep(),
}

export const processesProvider: CommandProvider = {
  id: 'processes',
  name: 'Processes',
  priority: 20,
  getCommands: (): Command[] => [killProcessCommand],
  // `kill <name>` surfaces matching processes directly in the root results
  search: async (query: string): Promise<Command[]> => {
    const match = /^kill\s+(.{2,})$/i.exec(query.trim())
    if (!match) return []
    const needle = match[1].toLowerCase()
    const groups = await loadProcessGroups()
    return groups
      .filter(g => g.name.toLowerCase().includes(needle))
      .slice(0, 8)
      .map(g => ({
        id: `process:kill:${g.name.toLowerCase()}`,
        label: `Kill ${g.name}`,
        description: groupSublabel(g),
        icon: 'trash',
        iconPath: g.exePath ?? undefined,
        source: 'system' as const,
        // Match on the full query (incl. the "kill" prefix) so ranking works
        keywords: ['kill'],
        data: g,
        actionLabel: 'Kill',
        action: async () => {
          await killAll(g.pids)
        },
      }))
  },
}
