import { describe, expect, it, vi } from 'vitest'
import type { AppConfig, Command, Step } from '../types'
import { commandDeepLinkStep } from './commandDeepLink'

const config: AppConfig = { scripts_dir: '' }

describe('external command deep-link navigation', () => {
  it('wraps a leaf action in a one-row step instead of executing it', async () => {
    const action = vi.fn(async () => {})
    const onActivated = vi.fn()
    const command: Command = { id: 'test:leaf', label: 'Leaf', icon: 'bolt', action }

    const step = commandDeepLinkStep(command, config, onActivated)!

    expect(action).not.toHaveBeenCalled()
    expect(await step.load!(config)).toEqual([expect.objectContaining({ id: command.id, label: command.label })])

    expect(await step.onSelect((await step.load!(config))[0], config)).toEqual({ type: 'done' })
    expect(action).toHaveBeenCalledOnce()
    expect(onActivated).toHaveBeenCalledWith(command.id)
  })

  it('opens an existing confirmation step without touching a direct action', () => {
    const action = vi.fn(async () => {})
    const confirmation: Step = {
      id: 'test:confirm',
      label: 'Confirm',
      placeholder: 'Are you sure?',
      onSelect: async () => ({ type: 'done' }),
    }
    const command: Command = {
      id: 'test:destructive',
      label: 'Destructive',
      icon: 'trash',
      action,
      createRootStep: () => confirmation,
    }

    expect(commandDeepLinkStep(command, config)).toBe(confirmation)
    expect(action).not.toHaveBeenCalled()
  })

  it('keeps no-close leaf commands open after explicit activation', async () => {
    const command: Command = {
      id: 'test:stay',
      label: 'Stay',
      icon: 'bolt',
      noClose: true,
      action: async () => {},
    }
    const step = commandDeepLinkStep(command, config)!

    expect(await step.onSelect((await step.load!(config))[0], config)).toEqual({ type: 'stay' })
  })
})
