import { describe, expect, it, vi } from 'vitest'

vi.mock('../lib/tauri', () => ({
  IS_WINDOWS: true,
  cachedMonitorInputs: vi.fn(),
  listMonitorInputs: vi.fn(),
  setMonitorInput: vi.fn(),
}))

import type { MonitorInputs } from '../lib/tauri'
import { MONITOR_INPUT_COMMAND_ID, monitorInputCommands, monitorNames } from './monitorInput'

const monitor = (id: string, name: string, position: string): MonitorInputs => ({
  id,
  name,
  position,
  current: null,
  inputs: [
    { value: 0x0f, label: 'DisplayPort 1' },
    { value: 0x11, label: 'HDMI 1' },
  ],
})

describe('monitor input commands', () => {
  it('stays hidden when no monitor supports input select', () => {
    expect(monitorInputCommands([])).toEqual([])
  })

  it('adds one browse command plus a search-only command per input', () => {
    const cmds = monitorInputCommands([monitor('a', 'XG27AQDMG', 'Main · Left')])
    expect(cmds.map(c => c.id)).toEqual([MONITOR_INPUT_COMMAND_ID, 'monitor-input:a:15', 'monitor-input:a:17'])
    expect(cmds[2].label).toBe('Switch XG27AQDMG to HDMI 1')
    expect(cmds.slice(1).every(c => c.searchOnly)).toBe(true)
    expect(cmds[1].keywords).toContain('dp')
    expect(cmds[1].keywords).toContain('left')
    expect(cmds[1].description).toBe('Main · Left monitor')
  })

  it('disambiguates identical models by desk position', () => {
    const names = monitorNames([monitor('a', 'VG279Q', 'Main · Left'), monitor('b', 'VG279Q', 'Right')])
    expect(names.get('b')).toBe('VG279Q (Right)')
    expect(monitorNames([monitor('a', 'VG279Q', 'Main · Left')]).get('a')).toBe('VG279Q')
  })
})
