// Tools built-ins (Calculator, Time Zone Converter). They carry
// folderName: 'Tools' — the Palette's root browse skips them and the flat
// search finds them with 'Tools' as the sublabel.
import type { Command, CommandProvider, Step, StepResult } from '../types'
import { appEvents } from '../lib/appEvents'
import { tryTimeConversion } from '../lib/timezones'
import { evaluateCalcQuery } from './calculator'

function calculatorStep(): Step {
  return {
    id: 'calculator:input',
    label: 'Calculator',
    placeholder: 'Type an expression (e.g. 40+2, 100 usd to eur)...',
    isInputStep: true,
    livePreview: query => {
      const trimmed = query.trim()
      if (!trimmed) return null
      const r = evaluateCalcQuery(trimmed)
      return r ? { label: r.display, sublabel: r.sublabel, copy: r.copy } : null
    },
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
  description: 'Evaluate expressions, convert units and currency',
  icon: 'calculator',
  source: 'calculator',
  folderName: 'Tools',
  keywords: ['calc', 'calculator', 'math', 'convert', 'currency'],
  createRootStep: () => calculatorStep(),
}

function timezonesStep(): Step {
  return {
    id: 'timezones:input',
    label: 'Time Zone Converter',
    placeholder: 'e.g. 4pm bst to est, 16:30 to tokyo, pst to gmt...',
    isInputStep: true,
    livePreview: query => {
      const trimmed = query.trim()
      if (!trimmed) return null
      const r = tryTimeConversion(trimmed)
      return r ? { label: r.label, sublabel: r.sublabel, copy: r.copy } : null
    },
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

// Registers the Tools built-ins
export const toolsProvider: CommandProvider = {
  id: 'tools',
  name: 'Tools',
  priority: 15,
  getCommands: (): Command[] => [calculatorCommand, timezonesCommand],
}
