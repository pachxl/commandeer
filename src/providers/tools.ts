// The Tools virtual folder: groups utility commands (Calculator, Kill
// Process, …) so they don't clog the root list. Children carry
// folderName: 'Tools' — the Palette's root browse skips them and the flat
// search finds them with 'Tools' as the sublabel, exactly like script folders.
import type { AppConfig, Command, CommandProvider, PaletteItem, Step, StepResult } from '../types'
import { appEvents } from '../lib/appEvents'
import { tryTimeConversion } from '../lib/timezones'
import { evaluateCalcQuery } from './calculator'

function calculatorStep(): Step {
  return {
    id: 'calculator:input',
    label: 'Calculator',
    placeholder: 'Type an expression (e.g. 40+2, 100 usd to eur, #ff6363)...',
    isInputStep: true,
    onSelect: async () => ({ type: 'done' }),
    onCommitQuery: async (query): Promise<StepResult> => {
      const trimmed = query.trim()
      if (!trimmed) return { type: 'pop' }
      const result = evaluateCalcQuery(trimmed)
      if (!result) throw new Error(`Could not evaluate '${trimmed}'`)
      await navigator.clipboard.writeText(result.copy)
      appEvents.toast?.(`${result.display} — copied`, 'success')
      // Stay open so the user can keep calculating (or tweak the expression)
      return { type: 'stay' }
    },
  }
}

export const calculatorCommand: Command = {
  id: 'builtin:calculator',
  label: 'Calculator',
  description: 'Evaluate expressions, convert units, currency, and colors',
  icon: 'calculator',
  source: 'calculator',
  folderName: 'Tools',
  keywords: ['calc', 'calculator', 'math', 'convert', 'currency', 'color'],
  createRootStep: () => calculatorStep(),
}

function timezonesStep(): Step {
  return {
    id: 'timezones:input',
    label: 'Time Zone Converter',
    placeholder: 'e.g. 4pm bst to est, 16:30 to tokyo, pst to gmt...',
    isInputStep: true,
    onSelect: async () => ({ type: 'done' }),
    onCommitQuery: async (query): Promise<StepResult> => {
      const trimmed = query.trim()
      if (!trimmed) return { type: 'pop' }
      const result = tryTimeConversion(trimmed)
      if (!result) throw new Error(`Could not parse '${trimmed}' — try '4pm bst to est'`)
      await navigator.clipboard.writeText(result.copy)
      appEvents.toast?.(`${result.sublabel} — copied`, 'success')
      return { type: 'stay' }
    },
  }
}

export const timezonesCommand: Command = {
  id: 'builtin:timezones',
  label: 'Time Zone Converter',
  description: 'Convert times between zones (4pm bst to est)',
  icon: 'clock',
  source: 'builtin',
  folderName: 'Tools',
  keywords: ['time', 'timezone', 'zone', 'convert', 'clock', 'utc'],
  createRootStep: () => timezonesStep(),
}

// Registers the Tools-folder built-ins (the folder itself is assembled in App)
export const toolsProvider: CommandProvider = {
  id: 'tools',
  name: 'Tools',
  priority: 15,
  getCommands: (): Command[] => [calculatorCommand, timezonesCommand],
}

function commandItem(cmd: Command): PaletteItem {
  return {
    id: cmd.id,
    label: cmd.label,
    sublabel: cmd.description,
    icon: cmd.icon,
    iconPath: cmd.iconPath,
    source: cmd.source,
    actionLabel: cmd.actionLabel,
    searchText: [cmd.label, cmd.description, ...(cmd.keywords ?? [])].filter(Boolean).join(' '),
    data: cmd.data ?? cmd.id,
  }
}

// A folder command whose children are built-in commands rather than scripts.
// Selecting a child pushes its step or runs its action.
export function virtualFolderCommand(name: string, children: Command[]): Command {
  return {
    id: `folder:${name}`,
    label: name,
    icon: 'folder',
    isFolder: true,
    createRootStep: (): Step => ({
      id: `folder-step:${name}`,
      label: name,
      placeholder: `Search ${name}...`,
      load: async (): Promise<PaletteItem[]> => children.map(commandItem),
      onSelect: async (item, config: AppConfig): Promise<StepResult> => {
        const cmd = children.find(c => c.id === item.id)
        if (!cmd) return { type: 'done' }
        if (cmd.createRootStep) return { type: 'push', step: cmd.createRootStep(config) }
        if (cmd.action) {
          await cmd.action(config)
          return cmd.noClose ? { type: 'pop' } : { type: 'done' }
        }
        return { type: 'done' }
      },
    }),
  }
}

export function toolsFolderCommand(children: Command[]): Command {
  return virtualFolderCommand('Tools', children)
}
