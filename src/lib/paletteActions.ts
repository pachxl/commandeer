// Ctrl+K action panel: secondary actions for the highlighted palette item.
//
// Extracted from Palette.tsx. buildItemActions is a pure builder given a
// context of the component's refs + feedback callbacks; the handlers read the
// refs lazily so they always see the latest config/commands/overrides at the
// time the action runs (matching the old in-component closure).

import { getCurrentWindow } from '@tauri-apps/api/window'
import type { Dispatch, MutableRefObject } from 'react'
import { setOverride, invalidateOverridesCache } from './overrides'
import { appEvents } from './appEvents'
import {
  IS_LINUX,
  IS_MAC,
  openPath,
  openUrl,
  pasteToPrevious,
  readQuicklinks,
  readNotes,
  revealPath,
  setCommandHotkey,
  writeClipboardText,
  writeQuicklinks,
  writeNotes,
  type Bookmark,
  type ClipboardItem,
  type Note,
  type Quicklink,
} from './tauri'
import type { Overrides } from './paletteRanking'
import type { ActionItem, AppConfig, Command, PaletteAction, PaletteItem } from '../types'
import type { ToastKind } from '../components/Toast'
import type { ConfirmOptions } from './confirm'

export interface ActionContext {
  dispatch: Dispatch<PaletteAction>
  // Read lazily inside handlers so they see the latest values at run time.
  configRef: MutableRefObject<AppConfig>
  commandsRef: MutableRefObject<Command[]>
  overridesRef: MutableRefObject<Overrides>
  // handleSelectRef: falls back to the normal selection path for step rows.
  // reloadStepRef: re-runs the current step's load after a mutating action.
  handleSelectRef: MutableRefObject<((item: PaletteItem) => Promise<void>) | null>
  reloadStepRef: MutableRefObject<() => void>
  resolveCommand: (id: string) => Command | undefined
  toast: (message: string, kind?: ToastKind) => void
  showHud: (message: string, icon?: string) => void
  requestConfirm: (options: ConfirmOptions) => Promise<boolean>
  refreshOverrides: () => Promise<void>
}

export function buildItemActions(item: PaletteItem, ctx: ActionContext): ActionItem[] {
  const {
    dispatch,
    configRef,
    commandsRef,
    overridesRef,
    handleSelectRef,
    reloadStepRef,
    resolveCommand,
    toast,
    showHud,
    requestConfirm,
    refreshOverrides,
  } = ctx
  const actions: ActionItem[] = []
  const cmd = resolveCommand(item.id)

  const pushCopy = (label: string, value: string, shortcut?: string) => {
    actions.push({
      id: 'copy',
      label,
      shortcut,
      icon: 'copy',
      handler: async () => {
        await navigator.clipboard.writeText(value)
        showHud('Copied to clipboard', 'copy')
      },
    })
  }

  const runPrimary = (id: string, label: string) => {
    actions.push({
      id,
      label,
      shortcut: '↵',
      handler: async () => {
        if (cmd?.action) {
          await cmd.action(configRef.current)
          if (!cmd.noClose) await getCurrentWindow().hide()
        } else if (cmd?.createRootStep) {
          dispatch({ type: 'PUSH_STEP', step: cmd.createRootStep(configRef.current) })
        } else {
          // Step rows aren't commands — fall back to the normal selection
          // path so the step's onSelect runs
          await handleSelectRef.current?.(item)
        }
      },
    })
  }

  switch (item.source) {
    case 'file': {
      const filePath = item.data as string
      // Path parts for the Copy… submenu (POSIX + Windows separators)
      const base =
        filePath
          .replace(/[/\\]+$/, '')
          .split(/[/\\]/)
          .pop() ?? filePath
      const dir = filePath.slice(0, filePath.length - base.length).replace(/[/\\]+$/, '') || filePath
      const copyLeaf = (id: string, label: string, value: string): ActionItem => ({
        id,
        label,
        icon: 'copy',
        handler: async () => {
          await navigator.clipboard.writeText(value)
          showHud('Copied to clipboard', 'copy')
        },
      })
      actions.push({
        id: 'open',
        label: 'Open file',
        shortcut: '↵',
        handler: async () => {
          await openPath(filePath)
          await getCurrentWindow().hide()
        },
      })
      actions.push({
        id: 'reveal',
        label: IS_MAC ? 'Reveal in Finder' : IS_LINUX ? 'Reveal in File Manager' : 'Reveal in File Explorer',
        shortcut: 'R',
        icon: 'folder',
        handler: async () => {
          await revealPath(filePath)
          await getCurrentWindow().hide()
        },
      })
      actions.push({
        id: 'copy',
        label: 'Copy…',
        shortcut: 'C',
        icon: 'copy',
        submenu: [
          copyLeaf('copy-path', 'Copy Full Path', filePath),
          copyLeaf('copy-name', 'Copy File Name', base),
          copyLeaf('copy-dir', 'Copy Containing Folder', dir),
        ],
      })
      break
    }
    case 'clipboard': {
      const clip = item.data as ClipboardItem
      if (clip && typeof clip === 'object' && 'text' in clip) {
        actions.push({
          id: 'paste',
          label: 'Paste to active app',
          shortcut: '↵',
          handler: async () => {
            try {
              const pasted = await pasteToPrevious(clip.text)
              if (!pasted) showHud('Copied — press Ctrl+V to paste', 'copy')
            } catch (err) {
              toast('Failed to paste', 'error')
              throw err
            }
          },
        })
        actions.push({
          id: 'copy',
          label: 'Copy to clipboard',
          shortcut: 'C',
          icon: 'copy',
          handler: async () => {
            await writeClipboardText(clip.text)
            showHud('Copied to clipboard', 'copy')
          },
        })
      } else {
        runPrimary('open', 'Open')
      }
      break
    }
    case 'calculator':
      pushCopy('Copy result', item.label, 'C')
      break
    case 'script':
      runPrimary('run', 'Run script')
      break
    case 'system':
      runPrimary('run', 'Run command')
      break
    case 'quicklink': {
      const q = item.data as Quicklink
      runPrimary('open', 'Open link')
      pushCopy('Copy URL', q.url, 'C')
      actions.push({
        id: 'delete',
        label: 'Delete quick link',
        shortcut: '⌫',
        icon: 'trash',
        handler: async () => {
          const ok = await requestConfirm({
            key: 'delete-quicklink',
            message: `Delete "${q.name}"?`,
            detail: 'This quick link cannot be recovered.',
            confirmLabel: 'Delete',
            danger: true,
          })
          if (!ok) return
          const all = await readQuicklinks()
          await writeQuicklinks(all.filter(x => x.id !== q.id))
          appEvents.refreshCommands?.()
          reloadStepRef.current()
          toast('Quick link deleted', 'success')
        },
      })
      break
    }
    case 'note': {
      const n = item.data as Note
      runPrimary('copy', 'Copy note')
      actions.push({
        id: 'delete',
        label: 'Delete note',
        shortcut: '⌫',
        icon: 'trash',
        handler: async () => {
          const ok = await requestConfirm({
            key: 'delete-note',
            message: `Delete "${n.title}"?`,
            detail: 'This note cannot be recovered.',
            confirmLabel: 'Delete',
            danger: true,
          })
          if (!ok) return
          const all = await readNotes()
          await writeNotes(all.filter(x => x.id !== n.id))
          appEvents.refreshCommands?.()
          reloadStepRef.current()
          toast('Note deleted', 'success')
        },
      })
      break
    }
    case 'bookmark': {
      const b = item.data as Bookmark
      actions.push({
        id: 'open',
        label: 'Open in browser',
        shortcut: '↵',
        handler: async () => {
          await openUrl(b.url)
          await getCurrentWindow().hide()
        },
      })
      pushCopy('Copy URL', b.url, 'C')
      break
    }
    default:
      runPrimary('open', 'Open')
      pushCopy('Copy name', item.label, 'C')
  }

  // Alias, pin & hotkey actions for persistent root commands (not for
  // dynamic step/search rows, whose ids never appear in the root list)
  if (commandsRef.current.some(c => c.id === item.id)) {
    const ov = overridesRef.current[item.id]
    actions.push({
      id: 'pin',
      label: ov?.pinned ? 'Unpin' : 'Pin',
      icon: 'bookmark',
      handler: async () => {
        const pinned = !ov?.pinned
        await setOverride(item.id, { pinned })
        await refreshOverrides()
        toast(pinned ? 'Pinned — boosts search rank' : 'Unpinned', 'success')
      },
    })
    actions.push({
      id: 'show-at-root',
      label: ov?.showAtRoot ? 'Hide from Root' : 'Show in Root',
      icon: 'pin',
      handler: async () => {
        const showAtRoot = !ov?.showAtRoot
        await setOverride(item.id, { showAtRoot })
        await refreshOverrides()
        toast(showAtRoot ? 'Shown on main page' : 'Hidden from main page', 'success')
      },
    })
    actions.push({
      id: 'alias',
      label: ov?.alias ? `Change Alias (${ov.alias})` : 'Set Alias…',
      icon: 'edit',
      handler: async () => {
        dispatch({
          type: 'PUSH_STEP',
          step: {
            id: `overrides:alias:${item.id}`,
            label: `Alias: ${item.label}`,
            placeholder: 'Type an alias (leave empty to clear)…',
            isInputStep: true,
            onSelect: async () => ({ type: 'done' }),
            onCommitQuery: async query => {
              await setOverride(item.id, { alias: query.trim() || undefined })
              await refreshOverrides()
              return { type: 'pop' }
            },
          },
        })
      },
    })
    actions.push({
      id: 'hotkey',
      label: ov?.hotkey ? `Change Hotkey (${ov.hotkey})` : 'Set Global Hotkey…',
      icon: 'keyboard',
      handler: async () => {
        dispatch({
          type: 'PUSH_STEP',
          step: {
            id: `overrides:hotkey:${item.id}`,
            label: `Hotkey: ${item.label}`,
            placeholder: 'e.g. Ctrl+Alt+L (leave empty to clear)…',
            isInputStep: true,
            onSelect: async () => ({ type: 'done' }),
            onCommitQuery: async query => {
              await setCommandHotkey(item.id, query.trim() || null)
              // The backend wrote overrides.json directly — drop the cache
              // so the action label reflects the new hotkey immediately
              invalidateOverridesCache()
              await refreshOverrides()
              return { type: 'pop' }
            },
          },
        })
      },
    })
  }

  return actions
}
