// Monitor input switching (Windows, DDC/CI VCP 0x60 — the channel ASUS
// DisplayWidget Center uses). Hardware-gated: commands only exist when a
// connected monitor advertises input select, so most users never see them.
import type { Command, CommandProvider, PaletteItem, Step, StepResult } from '../types'
import {
  cachedMonitorInputs,
  IS_WINDOWS,
  listMonitorInputs,
  setMonitorInput,
  type MonitorInput,
  type MonitorInputs,
} from '../lib/tauri'

export const MONITOR_INPUT_COMMAND_ID = 'monitor-input:switch'

const BASE_KEYWORDS = ['monitor', 'display', 'screen', 'input', 'source', 'switch', 'ddc', 'asus', 'displaywidget']

// Extra search terms so "dp" / "hdmi" / "usb-c" find the right input.
function inputKeywords(input: MonitorInput): string[] {
  const label = input.label.toLowerCase()
  const words = [label]
  if (label.startsWith('displayport')) words.push('dp', label.replace('displayport ', 'dp'))
  if (label.startsWith('hdmi')) words.push('hdmi', label.replace(' ', ''))
  return words
}

// Monitor display name; the Windows display number is appended only when two
// monitors share a model name.
export function monitorNames(monitors: MonitorInputs[]): Map<string, string> {
  const counts = new Map<string, number>()
  for (const m of monitors) counts.set(m.name, (counts.get(m.name) ?? 0) + 1)
  return new Map(monitors.map(m => [m.id, (counts.get(m.name) ?? 0) > 1 ? `${m.name} (${m.display})` : m.name]))
}

interface InputItemData {
  monitorId: string
  value: number
}

function switchStep(): Step {
  return {
    id: 'monitor-input:list',
    label: 'Switch Monitor Input',
    placeholder: 'Choose an input...',
    load: async (): Promise<PaletteItem[]> => {
      const monitors = await listMonitorInputs()
      const names = monitorNames(monitors)
      return monitors.flatMap(m =>
        m.inputs.map(input => ({
          id: `monitor-input:item:${m.id}:${input.value}`,
          label: input.label,
          sublabel: `${names.get(m.id)} · ${m.display}`,
          icon: 'monitor',
          actionLabel: 'Switch',
          keywords: [...inputKeywords(input), m.name],
          accessories: m.current === input.value ? [{ text: 'Current' }] : undefined,
          data: { monitorId: m.id, value: input.value } satisfies InputItemData,
        })),
      )
    },
    onSelect: async (item): Promise<StepResult> => {
      const { monitorId, value } = item.data as InputItemData
      await setMonitorInput(monitorId, value)
      return { type: 'done' }
    },
  }
}

export function monitorInputCommands(monitors: MonitorInputs[]): Command[] {
  if (monitors.length === 0) return []
  const names = monitorNames(monitors)
  const browse: Command = {
    id: MONITOR_INPUT_COMMAND_ID,
    label: 'Switch Monitor Input',
    description: 'Change a monitor’s input source (HDMI, DisplayPort, …) over DDC/CI',
    icon: 'monitor',
    source: 'system',
    folderName: 'System',
    keywords: [...BASE_KEYWORDS, 'hdmi', 'displayport', 'dp'],
    createRootStep: () => switchStep(),
  }
  // One direct command per monitor × input so "hdmi" from the root switches in
  // one Enter (and each can take its own global hotkey via Ctrl+K).
  const direct = monitors.flatMap(m =>
    m.inputs.map((input): Command => ({
      id: `monitor-input:${m.id}:${input.value}`,
      label: `Switch ${names.get(m.id)} to ${input.label}`,
      description: `${m.display} input source`,
      icon: 'monitor',
      source: 'system',
      folderName: 'System',
      searchOnly: true,
      keywords: [...BASE_KEYWORDS, ...inputKeywords(input)],
      actionLabel: 'Switch',
      action: async () => {
        await setMonitorInput(m.id, input.value)
      },
    })),
  )
  return [browse, ...direct]
}

export const monitorInputProvider: CommandProvider = {
  id: 'monitor-input',
  name: 'Monitor Input',
  priority: 40,
  getCommands: async (): Promise<Command[]> => {
    if (!IS_WINDOWS) return []
    return monitorInputCommands(await cachedMonitorInputs())
  },
}
