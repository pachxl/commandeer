import type { Command, CommandProvider, PaletteItem, Step } from '../types'
import { clearClipboardHistory, pasteToPrevious, readClipboardHistory, type ClipboardItem } from '../lib/tauri'
import { fuzzyFilter } from '../lib/fuzzy'

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
    onSelect: async (item) => {
      const clipboardItem = item.data as ClipboardItem
      await pasteToPrevious(clipboardItem.text)
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
        await clearClipboardHistory()
      },
    },
  ],
  search: async (query: string): Promise<Command[]> => {
    const q = query.trim()
    if (q.length < 2) return []
    const items = await readClipboardHistory()
    // Rank the whole history before truncating, so the best fuzzy match still
    // surfaces even when it sits deep in the list.
    return fuzzyFilter(items, q, item => item.text)
      .slice(0, 5)
      .map(item => ({
        id: `clipboard:${item.id}`,
        label: item.text.slice(0, 80).replace(/\n/g, ' '),
        description: 'Paste to active app',
        icon: 'snippet',
        source: 'clipboard',
        data: item,
        accessories: [{ text: clipboardType(item.text) }],
        metadata: clipboardMetadata(item),
        action: async () => {
          await pasteToPrevious(item.text)
        },
      }))
  },
}
