import { beforeEach, describe, expect, it, vi } from 'vitest'
import { appEvents } from '../lib/appEvents'
import type { AppConfig, Command, PaletteItem, Step } from '../types'
import { clipboardProvider } from './clipboard'
import { loadNoteCommands } from './notes'
import { loadQuicklinkCommands } from './quicklinks'

const mocks = vi.hoisted(() => ({
  clearClipboardHistory: vi.fn(),
  openUrl: vi.fn(),
  pasteToPrevious: vi.fn(),
  readClipboardHistory: vi.fn(),
  readNotes: vi.fn(),
  readQuicklinks: vi.fn(),
  writeClipboardText: vi.fn(),
  writeNotes: vi.fn(),
  writeQuicklinks: vi.fn(),
}))

vi.mock('../lib/tauri', () => mocks)

const config = {} as AppConfig

function commandById(commands: Command[], id: string): Command {
  const command = commands.find(candidate => candidate.id === id)
  if (!command) throw new Error(`Missing command ${id}`)
  return command
}

function rootStep(command: Command): Step {
  if (!command.createRootStep) throw new Error(`Command ${command.id} has no root step`)
  return command.createRootStep(config)
}

function selectedItem(id: string): PaletteItem {
  return { id: `remove:${id}`, label: id, icon: 'trash', data: id }
}

describe('destructive provider actions', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    appEvents.confirm = vi.fn()
    appEvents.refreshCommands = vi.fn()
    appEvents.toast = vi.fn()
  })

  it('clears clipboard history only after destructive confirmation', async () => {
    if (!clipboardProvider.getCommands) throw new Error('Clipboard provider has no commands')
    const command = commandById(await clipboardProvider.getCommands(config), 'clipboard:clear')
    const confirm = vi.mocked(appEvents.confirm!)

    confirm.mockResolvedValueOnce(false)
    await command.action?.(config)
    expect(mocks.clearClipboardHistory).not.toHaveBeenCalled()

    confirm.mockResolvedValueOnce(true)
    await command.action?.(config)
    expect(confirm).toHaveBeenLastCalledWith({
      key: 'clear-clipboard-history',
      message: 'Clear all clipboard history?',
      detail: 'This clipboard history cannot be recovered.',
      confirmLabel: 'Clear',
      danger: true,
    })
    expect(mocks.clearClipboardHistory).toHaveBeenCalledOnce()
    expect(appEvents.toast).toHaveBeenCalledWith('Clipboard history cleared', 'success')
  })

  it('leaves the note-removal step untouched when confirmation is cancelled', async () => {
    mocks.readNotes.mockResolvedValue([{ id: 'note-1', title: 'Private note', content: 'Keep me' }])
    vi.mocked(appEvents.confirm!).mockResolvedValue(false)
    const step = rootStep(commandById(await loadNoteCommands(), 'note:remove'))

    const result = await step.onSelect(selectedItem('note-1'), config)

    expect(appEvents.confirm).toHaveBeenCalledWith({
      key: 'delete-note',
      message: 'Delete "Private note"?',
      detail: 'This note cannot be recovered.',
      confirmLabel: 'Delete',
      danger: true,
    })
    expect(result).toEqual({ type: 'stay' })
    expect(mocks.writeNotes).not.toHaveBeenCalled()
  })

  it('deletes a quick link only after confirmation', async () => {
    const quicklinks = [
      { id: 'quick-1', name: 'Search', url: 'https://example.com', icon: null },
      { id: 'quick-2', name: 'Keep', url: 'https://example.org', icon: null },
    ]
    mocks.readQuicklinks.mockResolvedValue(quicklinks)
    vi.mocked(appEvents.confirm!).mockResolvedValue(true)
    const step = rootStep(commandById(await loadQuicklinkCommands(), 'quicklink:remove'))

    const result = await step.onSelect(selectedItem('quick-1'), config)

    expect(appEvents.confirm).toHaveBeenCalledWith({
      key: 'delete-quicklink',
      message: 'Delete "Search"?',
      detail: 'This quick link cannot be recovered.',
      confirmLabel: 'Delete',
      danger: true,
    })
    expect(mocks.writeQuicklinks).toHaveBeenCalledWith([quicklinks[1]])
    expect(appEvents.refreshCommands).toHaveBeenCalledOnce()
    expect(appEvents.toast).toHaveBeenCalledWith('Quick link deleted', 'success')
    expect(result.type).toBe('replace')
  })
})
