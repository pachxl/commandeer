import { describe, expect, it } from 'vitest'
import { guideCommand, guideStep } from './guide'
import type { AppConfig } from '../types'

const config: AppConfig = { scripts_dir: '' }

describe('Commandeer guide', () => {
  it('is permanently searchable', () => {
    const command = guideCommand(config)
    expect(command.label).toBe('Commandeer Guide')
    expect(command.keywords).toContain('shortcuts')
    expect(command.createRootStep).toBeTypeOf('function')
  })

  it('teaches core modes and keeps reading rows open', async () => {
    const step = guideStep(config, true)
    const items = await step.load!(config)
    expect(items.some(item => item.accessories?.some(accessory => accessory.text === '@find'))).toBe(true)
    expect(items.some(item => item.accessories?.some(accessory => accessory.text === 'Ctrl K'))).toBe(true)
    expect(await step.onSelect(items[0], config)).toEqual({ type: 'stay' })
    expect(await step.onSelect(items[items.length - 1], config)).toEqual({ type: 'done' })
  })
})
