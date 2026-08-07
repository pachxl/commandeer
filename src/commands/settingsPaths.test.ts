import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { appEvents } from '../lib/appEvents'
import type { AppConfig, Step } from '../types'
import { settingsCommand } from './settings'

const mocks = vi.hoisted(() => ({
  writeConfig: vi.fn<(config: AppConfig) => Promise<void>>(),
  openPath: vi.fn<(path: string) => Promise<void>>(),
}))

vi.mock('../lib/tauri', () => ({
  dataDir: vi.fn().mockResolvedValue('C:\\data'),
  getAutostart: vi.fn().mockResolvedValue(false),
  getPermissionStatus: vi.fn(),
  openPath: mocks.openPath,
  openPermissionSettings: vi.fn(),
  setAutostart: vi.fn(),
  setGlobalHotkey: vi.fn(),
  setPerMonitorAltTab: vi.fn(),
  setScreenshotHotkey: vi.fn(),
  setWindowDrag: vi.fn(),
  setWindowTransparency: vi.fn(),
  startScreenshot: vi.fn(),
  writeConfig: mocks.writeConfig,
}))

async function settingsRoot(config: AppConfig): Promise<Step> {
  return settingsCommand(config).createRootStep!(config)
}

describe('path settings', () => {
  beforeEach(() => {
    mocks.writeConfig.mockReset().mockResolvedValue()
    mocks.openPath.mockReset().mockResolvedValue()
    appEvents.refreshCommands = vi.fn()
    appEvents.toast = vi.fn()
  })

  afterEach(() => {
    appEvents.refreshCommands = undefined
    appEvents.toast = undefined
  })

  it('persists a scripts directory and refreshes commands immediately', async () => {
    const config: AppConfig = { scripts_dir: 'C:\\Old' }
    const root = await settingsRoot(config)
    const item = (await root.load!(config)).find(candidate => candidate.id === 'settings:scripts-directory')!
    const result = await root.onSelect(item, config)
    expect(result.type).toBe('push')
    const form = result.type === 'push' ? result.step : undefined

    expect(await form!.onSubmit!({ scripts_dir: '  "C:\\New Scripts"  ' }, config)).toEqual({ type: 'done' })
    expect(mocks.writeConfig).toHaveBeenCalledWith(expect.objectContaining({ scripts_dir: 'C:\\New Scripts' }))
    expect(config.scripts_dir).toBe('C:\\New Scripts')
    expect(appEvents.refreshCommands).toHaveBeenCalledOnce()
  })

  it('saves normalized custom roots and exposes a default reset', async () => {
    const config: AppConfig = { scripts_dir: 'C:\\Scripts' }
    const root = await settingsRoot(config)
    const rootsItem = (await root.load!(config)).find(candidate => candidate.id === 'settings:search-roots')!
    const rootsResult = await root.onSelect(rootsItem, config)
    const rootsStep = rootsResult.type === 'push' ? rootsResult.step : undefined
    const editItem = (await rootsStep!.load!(config)).find(candidate => candidate.id === 'search-roots:edit')!
    const editResult = await rootsStep!.onSelect(editItem, config)
    const form = editResult.type === 'push' ? editResult.step : undefined

    expect(await form!.onSubmit!({ search_paths: 'C:\\Projects\nC:\\Projects\nD:\\Work' }, config)).toEqual({
      type: 'done',
    })
    expect(config.search_paths).toEqual(['C:\\Projects', 'D:\\Work'])

    const resetItem = (await rootsStep!.load!(config)).find(candidate => candidate.id === 'search-roots:reset')!
    expect(await rootsStep!.onSelect(resetItem, config)).toMatchObject({ type: 'replace' })
    expect(config.search_paths).toBeUndefined()
    expect(mocks.writeConfig).toHaveBeenLastCalledWith({ scripts_dir: 'C:\\Scripts' })
  })

  it('rejects relative and empty root lists without persisting them', async () => {
    const config: AppConfig = { scripts_dir: 'C:\\Scripts' }
    const root = await settingsRoot(config)
    const rootsItem = (await root.load!(config)).find(candidate => candidate.id === 'settings:search-roots')!
    const rootsResult = await root.onSelect(rootsItem, config)
    const rootsStep = rootsResult.type === 'push' ? rootsResult.step : undefined
    const editItem = (await rootsStep!.load!(config)).find(candidate => candidate.id === 'search-roots:edit')!
    const editResult = await rootsStep!.onSelect(editItem, config)
    const form = editResult.type === 'push' ? editResult.step : undefined

    expect(await form!.onSubmit!({ search_paths: 'relative\\folder' }, config)).toEqual({ type: 'stay' })
    expect(await form!.onSubmit!({ search_paths: '\n  \n' }, config)).toEqual({ type: 'stay' })
    expect(mocks.writeConfig).not.toHaveBeenCalled()
  })
})
