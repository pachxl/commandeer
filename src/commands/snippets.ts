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
    folderName: 'Snippets',
    keywords: [snippet.text],
    actionLabel: 'Paste to active app',
    data: snippet,
    action: async () => {
      const pasted = await pasteToPrevious(snippet.text)
      if (!pasted) appEvents.toast?.('Copied — press Ctrl+V to paste', 'success')
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
      if (!keyword) return { type: 'stay' }
      // Replace (not push) so the text step sits directly above the Snippets
      // folder: one pop after saving lands back on the snippets list
      return {
        type: 'replace',
        step: {
          id: 'snippet:add:text',
          label: 'Snippet Text',
          placeholder: 'Paste or type the snippet text...',
          isInputStep: true,
          onSelect: async () => ({ type: 'done' }),
          onCommitQuery: async (text): Promise<StepResult> => {
            const trimmed = text.trim()
            if (!trimmed) return { type: 'stay' }
            const all = await readSnippets()
            const next: Snippet = {
              id: crypto.randomUUID(),
              keyword,
              text: trimmed,
            }
            await writeSnippets([...all, next])
            appEvents.refreshCommands?.()
            appEvents.toast?.('Snippet saved', 'success')
            return { type: 'pop' }
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

// All snippet commands live in the Snippets virtual folder: Add, Remove, then
// the saved snippets themselves. The folder keeps them out of the root browse
// list; the flat search still finds every one.
export async function loadSnippetCommands(): Promise<Command[]> {
  const snippets = await readSnippets()
  return [
    {
      id: 'snippet:add',
      label: 'Add Snippet',
      description: 'Save a new text snippet',
      icon: 'plus',
      folderName: 'Snippets',
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
          folderName: 'Snippets',
          keywords: ['snippet', 'remove', 'delete'],
          actionLabel: 'Open',
          createRootStep: removeSnippetStep,
        } satisfies Command]
      : []),
    ...snippets.map(snippetCommand),
  ]
}
