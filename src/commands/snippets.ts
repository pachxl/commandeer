import type { Command, PaletteItem, Step, StepResult } from '../types'
import { appEvents } from '../lib/appEvents'
import { pasteToPrevious, readSnippets, writeSnippets, type Snippet } from '../lib/tauri'

function snippetCommand(snippet: Snippet): Command {
  return {
    id: `snippet:${snippet.id}`,
    label: snippet.keyword,
    description: snippet.text.slice(0, 80).replace(/\n/g, ' '),
    icon: 'snippet',
    source: 'snippet',
    keywords: [snippet.text],
    actionLabel: 'Paste to active app',
    searchOnly: true,
    data: snippet,
    action: async () => {
      await pasteToPrevious(snippet.text)
    },
  }
}

function addSnippetStep(): Step {
  return {
    id: 'snippet:add:keyword',
    label: 'Add Snippet',
    placeholder: 'Keyword (e.g. sig)...',
    isInputStep: true,
    onSelect: async () => ({ type: 'done' }),
    onCommitQuery: async (query): Promise<StepResult> => {
      const keyword = query.trim()
      if (!keyword) return { type: 'done' }
      return {
        type: 'push',
        step: {
          id: 'snippet:add:text',
          label: 'Snippet Text',
          placeholder: 'Paste or type the snippet text...',
          isInputStep: true,
          onSelect: async () => ({ type: 'done' }),
          onCommitQuery: async (text): Promise<StepResult> => {
            const trimmed = text.trim()
            if (!trimmed) return { type: 'done' }
            const all = await readSnippets()
            const next: Snippet = {
              id: crypto.randomUUID(),
              keyword,
              text: trimmed,
            }
            await writeSnippets([...all, next])
            appEvents.refreshCommands?.()
            return { type: 'done' }
          },
        },
      }
    },
  }
}

function removeSnippetStep(): Step {
  return {
    id: 'snippet:remove',
    label: 'Remove Snippet',
    placeholder: 'Select a snippet to remove...',
    load: async (): Promise<PaletteItem[]> => {
      const snippets = await readSnippets()
      return snippets.map(s => ({
        id: `snippet:remove:${s.id}`,
        label: s.keyword,
        sublabel: s.text.slice(0, 80).replace(/\n/g, ' '),
        icon: 'trash',
        data: s.id,
        actionLabel: 'Remove Snippet',
      }))
    },
    onSelect: async (item): Promise<StepResult> => {
      const all = await readSnippets()
      await writeSnippets(all.filter(s => s.id !== item.data))
      appEvents.refreshCommands?.()
      return { type: 'replace', step: removeSnippetStep() }
    },
  }
}

export async function loadSnippetCommands(): Promise<Command[]> {
  const snippets = await readSnippets()
  return [
    {
      id: 'snippet:add',
      label: 'Add Snippet',
      description: 'Save a new text snippet',
      icon: 'plus',
      keywords: ['snippet', 'add', 'expand'],
      actionLabel: 'Add Snippet',
      createRootStep: addSnippetStep,
    },
    ...(snippets.length > 0
      ? [{
          id: 'snippet:remove',
          label: 'Remove Snippet',
          description: 'Delete a saved snippet',
          icon: 'trash',
          keywords: ['snippet', 'remove', 'delete'],
          actionLabel: 'Open',
          searchOnly: true,
          createRootStep: removeSnippetStep,
        } satisfies Command]
      : []),
    ...snippets.map(snippetCommand),
  ]
}
