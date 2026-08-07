import type { Command, CommandProvider, PaletteItem, Step } from '../types'
import { appEvents } from '../lib/appEvents'
import { clearClipboardHistory, pasteToPrevious, readClipboardHistory, type ClipboardItem } from '../lib/tauri'

function clipboardType(text: string): string {
  if (/^https?:\/\//i.test(text)) return 'link'
  if (/^[^\s@]+@[^\s@]+\.[^\s@]+$/i.test(text)) return 'email'
  return 'text'
}

function clipboardMetadata(item: ClipboardItem) {
  return [
    { label: 'Copied', value: new Date(item.copied_at).toLocaleString() },
    { label: 'Length', value: `${item.text.length} chars` },
    { label: 'Type', value: clipboardType(item.text) },
  ]
}

async function loadHistory(): Promise<PaletteItem[]> {
  const items = await readClipboardHistory()
  return items.map(item => ({
    id: `clipboard:${item.id}`,
    label: item.text.slice(0, 80).replace(/\n/g, ' '),
    sublabel: new Date(item.copied_at).toLocaleString(),
    icon: 'snippet',
    source: 'clipboard',
    data: item,
    accessories: [{ text: clipboardType(item.text) }],
    metadata: clipboardMetadata(item),
  }))
}

function clipboardHistoryStep(): Step {
  return {
    id: 'clipboard:history',
    label: 'Clipboard History',
    placeholder: 'Select an item to paste...',
    load: async () => loadHistory(),
    onSelect: async item => {
      const clipboardItem = item.data as ClipboardItem
      const pasted = await pasteToPrevious(clipboardItem.text)
      if (!pasted) appEvents.toast?.('Copied — press Ctrl+V to paste', 'success')
      return { type: 'done' }
    },
  }
}

export const clipboardProvider: CommandProvider = {
  id: 'clipboard',
  name: 'Clipboard',
  priority: 35,
  getCommands: (): Command[] => [
    {
      id: 'clipboard:history',
      label: 'Clipboard History',
      description: 'View recently copied items',
      icon: 'snippet',
      source: 'clipboard',
      folderName: 'Tools',
      aliases: ['clipboard', 'history'],
      keywords: ['clipboard', 'history', 'paste'],
      createRootStep: clipboardHistoryStep,
    },
    {
      id: 'clipboard:clear',
      label: 'Clear Clipboard History',
      description: 'Remove all clipboard history',
      icon: 'trash',
      source: 'clipboard',
      // Findable by search but kept out of the root browse list
      searchOnly: true,
      action: async () => {
        const confirmed = await appEvents.confirm?.({
          key: 'clear-clipboard-history',
          message: 'Clear all clipboard history?',
          detail: 'This clipboard history cannot be recovered.',
          confirmLabel: 'Clear',
          danger: true,
        })
        if (!confirmed) return
        await clearClipboardHistory()
        appEvents.toast?.('Clipboard history cleared', 'success')
      },
    },
  ],
  // No search(): clipboard contents must not surface in root search results —
  // history is only reachable through the Clipboard History step.
}
