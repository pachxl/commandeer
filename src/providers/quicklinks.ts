import type { Command, CommandProvider, PaletteItem, Step, StepResult } from '../types'
import { appEvents } from '../lib/appEvents'
import { openUrl, readQuicklinks, writeQuicklinks, type Quicklink } from '../lib/tauri'

const QUERY_PLACEHOLDER = '{query}'

function quicklinkCommand(quicklink: Quicklink): Command {
  const hasPlaceholder = quicklink.url.includes(QUERY_PLACEHOLDER)
  const base = {
    id: `quicklink:${quicklink.id}`,
    label: quicklink.name,
    description: quicklink.url,
    icon: quicklink.icon ?? 'quicklink',
    source: 'quicklink' as const,
    folderName: 'Quick Links',
    keywords: [quicklink.name, quicklink.url],
    data: quicklink,
  }

  if (hasPlaceholder) {
    return {
      ...base,
      actionLabel: 'Search',
      createRootStep: () => ({
        id: `quicklink:input:${quicklink.id}`,
        label: quicklink.name,
        placeholder: `Type query for ${quicklink.name}...`,
        isInputStep: true,
        onSelect: async () => ({ type: 'done' }),
        onCommitQuery: async (query): Promise<StepResult> => {
          const trimmed = query.trim()
          if (!trimmed) return { type: 'pop' }
          const url = quicklink.url.replace(QUERY_PLACEHOLDER, encodeURIComponent(trimmed))
          await openUrl(url)
          return { type: 'done' }
        },
      }),
    }
  }

  return {
    ...base,
    actionLabel: 'Open',
    action: async () => {
      await openUrl(quicklink.url)
    },
  }
}

function addQuicklinkStep(): Step {
  return {
    id: 'quicklink:add:name',
    label: 'Add Quick Link',
    placeholder: 'Name (e.g. Search Google)...',
    isInputStep: true,
    onSelect: async () => ({ type: 'done' }),
    onCommitQuery: async (query): Promise<StepResult> => {
      const name = query.trim()
      if (!name) return { type: 'stay' }
      return {
        type: 'replace',
        step: {
          id: 'quicklink:add:url',
          label: 'Add Quick Link URL',
          placeholder: 'URL with optional {query} placeholder...',
          isInputStep: true,
          onSelect: async () => ({ type: 'done' }),
          onCommitQuery: async (urlQuery): Promise<StepResult> => {
            const url = urlQuery.trim()
            if (!url) return { type: 'stay' }
            const all = await readQuicklinks()
            const next: Quicklink = {
              id: crypto.randomUUID(),
              name,
              url,
              icon: null,
            }
            await writeQuicklinks([...all, next])
            appEvents.refreshCommands?.()
            appEvents.toast?.('Quick link saved', 'success')
            return { type: 'pop' }
          },
        },
      }
    },
  }
}

function removeQuicklinkStep(): Step {
  return {
    id: 'quicklink:remove',
    label: 'Remove Quick Link',
    placeholder: 'Select a quick link to remove...',
    load: async (): Promise<PaletteItem[]> => {
      const quicklinks = await readQuicklinks()
      return quicklinks.map(q => ({
        id: `quicklink:remove:${q.id}`,
        label: q.name,
        sublabel: q.url,
        icon: q.icon ?? 'quicklink',
        data: q.id,
        actionLabel: 'Remove Quick Link',
      }))
    },
    onSelect: async (item): Promise<StepResult> => {
      const all = await readQuicklinks()
      await writeQuicklinks(all.filter(q => q.id !== item.data))
      appEvents.refreshCommands?.()
      return { type: 'replace', step: removeQuicklinkStep() }
    },
  }
}

export async function loadQuicklinkCommands(): Promise<Command[]> {
  const quicklinks = await readQuicklinks()
  return [
    {
      id: 'quicklink:add',
      label: 'Add Quick Link',
      description: 'Save a URL or templated search',
      icon: 'plus',
      folderName: 'Quick Links',
      keywords: ['quicklink', 'bookmark', 'url', 'add'],
      actionLabel: 'Add Quick Link',
      createRootStep: addQuicklinkStep,
    },
    ...(quicklinks.length > 0
      ? [{
          id: 'quicklink:remove',
          label: 'Remove Quick Link',
          description: 'Delete a saved quick link',
          icon: 'trash',
          folderName: 'Quick Links',
          keywords: ['quicklink', 'remove', 'delete'],
          actionLabel: 'Open',
          createRootStep: removeQuicklinkStep,
        } satisfies Command]
      : []),
    ...quicklinks.map(quicklinkCommand),
  ]
}

export const quicklinksProvider: CommandProvider = {
  id: 'quicklinks',
  name: 'Quick Links',
  priority: 12,
  getCommands: loadQuicklinkCommands,
}
