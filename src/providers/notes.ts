import type { Command, CommandProvider, PaletteItem, Step, StepResult } from '../types'
import { appEvents } from '../lib/appEvents'
import { readNotes, writeClipboardText, writeNotes, type Note } from '../lib/tauri'

function noteCommand(note: Note): Command {
  const preview = note.content.slice(0, 80).replace(/\n/g, ' ')
  return {
    id: `note:${note.id}`,
    label: note.title,
    description: preview,
    icon: 'note',
    source: 'note' as const,
    folderName: 'Notes',
    keywords: [note.title, note.content],
    actionLabel: 'Copy to clipboard',
    data: note,
    // Notes are often written in markdown; show the full body formatted in the
    // detail pane while the row keeps a one-line plain-text preview.
    detailMarkdown: note.content,
    action: async () => {
      await writeClipboardText(note.content)
      appEvents.toast?.('Note copied', 'success')
    },
  }
}

function addNoteStep(): Step {
  return {
    id: 'note:add:title',
    label: 'Add Note',
    placeholder: 'Title...',
    isInputStep: true,
    onSelect: async () => ({ type: 'done' }),
    onCommitQuery: async (query): Promise<StepResult> => {
      const title = query.trim()
      if (!title) return { type: 'stay' }
      return {
        type: 'replace',
        step: {
          id: 'note:add:content',
          label: 'Note Text',
          placeholder: 'Paste or type the note text...',
          isInputStep: true,
          onSelect: async () => ({ type: 'done' }),
          onCommitQuery: async (text): Promise<StepResult> => {
            const content = text.trim()
            if (!content) return { type: 'stay' }
            const all = await readNotes()
            const next: Note = {
              id: crypto.randomUUID(),
              title,
              content,
            }
            await writeNotes([next, ...all])
            appEvents.refreshCommands?.()
            appEvents.toast?.('Note saved', 'success')
            return { type: 'pop' }
          },
        },
      }
    },
  }
}

function removeNoteStep(): Step {
  return {
    id: 'note:remove',
    label: 'Remove Note',
    placeholder: 'Select a note to remove...',
    load: async (): Promise<PaletteItem[]> => {
      const notes = await readNotes()
      return notes.map(n => ({
        id: `note:remove:${n.id}`,
        label: n.title,
        sublabel: n.content.slice(0, 80).replace(/\n/g, ' '),
        icon: 'trash',
        data: n.id,
        actionLabel: 'Remove Note',
      }))
    },
    onSelect: async (item): Promise<StepResult> => {
      const all = await readNotes()
      await writeNotes(all.filter(n => n.id !== item.data))
      appEvents.refreshCommands?.()
      return { type: 'replace', step: removeNoteStep() }
    },
  }
}

export async function loadNoteCommands(): Promise<Command[]> {
  const notes = await readNotes()
  return [
    {
      id: 'note:add',
      label: 'Add Note',
      description: 'Save a new text note',
      icon: 'plus',
      folderName: 'Notes',
      keywords: ['note', 'add', 'memo'],
      actionLabel: 'Add Note',
      createRootStep: addNoteStep,
    },
    ...(notes.length > 0
      ? [{
          id: 'note:remove',
          label: 'Remove Note',
          description: 'Delete a saved note',
          icon: 'trash',
          folderName: 'Notes',
          keywords: ['note', 'remove', 'delete'],
          actionLabel: 'Open',
          createRootStep: removeNoteStep,
        } satisfies Command]
      : []),
    ...notes.map(noteCommand),
  ]
}

export const notesProvider: CommandProvider = {
  id: 'notes',
  name: 'Notes',
  priority: 11,
  getCommands: loadNoteCommands,
}
