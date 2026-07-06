import type { AppConfig, Command, Step, PaletteItem } from '../types'
import { listScripts, openUrl, runScript, type ScriptInfo } from '../lib/tauri'
import { hasIcon } from '../components/Icon'

// Togglable in Settings (App reads the visibility flag when building the list)
export const webSearchCommand: Command = {
  id: 'builtin:search',
  label: 'Search',
  icon: 'search',
  description: 'Search the web',
  folderName: 'Tools',
  actionLabel: 'Open',
  createRootStep: (): Step => ({
    id: 'search-input',
    label: 'Search',
    placeholder: 'Type your search...',
    isInputStep: true,
    onSelect: async () => ({ type: 'done' }),
    onCommitQuery: async (query) => {
      const url = `https://www.google.com/search?q=${encodeURIComponent(query)}`
      await openUrl(url)
      return { type: 'done' }
    },
  }),
}

export async function loadScriptCommands(config: AppConfig): Promise<{ commands: Command[]; scripts: ScriptInfo[] }> {
  const scripts = await listScripts(config.scripts_dir)
  return { commands: scriptsToCommands(scripts), scripts }
}

// Resolve a script's display icon: a metadata-declared named icon wins if the
// name exists in the Icon library, else a sibling PNG, else the generic
// 'script' glyph so every script row carries an icon.
function scriptIcon(script: ScriptInfo): string {
  const named = script.metadata?.icon_name
  if (named && hasIcon(named)) return named
  return script.icon ?? 'script'
}

function scriptTitle(script: ScriptInfo): string {
  return script.metadata?.title ?? script.name
}

// Accessory badge for an explicitly-declared mode (inline/terminal/fullOutput/
// compact). Silent is the implicit default and gets no badge.
function scriptAccessories(script: ScriptInfo) {
  const mode = script.metadata?.mode
  return mode ? [{ text: mode }] : undefined
}

// An inline script (@vicinae.mode inline + refreshTime) produces a single
// live-refreshing row whose sublabel is the script's captured stdout. We mark
// it with liveOutputKey so the palette polls it and overrides the sublabel at
// render time (not in the ranked search text, to avoid re-ranking on refresh).
function inlineRefreshKey(script: ScriptInfo): string | undefined {
  if (script.metadata?.mode === 'inline' && script.metadata.refresh_seconds != null) {
    return script.path
  }
  return undefined
}

// Confirmation step for scripts that declare @vicinae.needsConfirmation true —
// a fuzzy-matched "res" shouldn't immediately run a destructive script.
function scriptConfirmStep(script: ScriptInfo): Step {
  const title = scriptTitle(script)
  return {
    id: `script:${script.path}:confirm`,
    label: title,
    placeholder: `Run "${title}"?`,
    load: async () => [
      { id: 'confirm', label: `Run ${title}`, sublabel: 'Press Enter to confirm', icon: 'script', actionLabel: 'Confirm' },
      { id: 'cancel', label: 'Cancel', icon: 'x', actionLabel: 'Cancel' },
    ],
    onSelect: async item => {
      if (item.id !== 'confirm') return { type: 'pop' }
      await runScript(script.path)
      return { type: 'done' }
    },
  }
}

export function scriptsToCommands(scripts: ScriptInfo[]): Command[] {
  const commands: Command[] = []

  for (const script of scripts) {
    if (script.is_folder) {
      const folderName = script.name
      const folderScripts = scripts.filter(s => s.folder === folderName)

      commands.push({
        id: `folder:${folderName}`,
        label: folderName,
        icon: script.icon ?? 'folder',
        source: 'script',
        isFolder: true,
        createRootStep: (_cfg): Step => ({
          id: `folder-step:${folderName}`,
          label: folderName,
          placeholder: `Search ${folderName}...`,
          load: async (_cfg): Promise<PaletteItem[]> => folderScripts.map(s => ({
            id: `script:${s.path}`,
            label: scriptTitle(s),
            icon: scriptIcon(s),
            sublabel: s.metadata?.description ?? undefined,
            keywords: s.metadata?.keywords,
            accessories: scriptAccessories(s),
            liveOutputKey: inlineRefreshKey(s),
            data: s.path,
          })),
          onSelect: async (item, _cfg) => {
            const s = folderScripts.find(fs => fs.path === (item.data as string))
            if (s?.metadata?.needs_confirmation) {
              return { type: 'push', step: scriptConfirmStep(s) }
            }
            await runScript(item.data as string)
            return { type: 'done' }
          },
        }),
      })
    } else {
      const liveKey = inlineRefreshKey(script)
      const base: Command = {
        id: `script:${script.path}`,
        label: scriptTitle(script),
        icon: scriptIcon(script),
        source: 'script',
        folderName: script.folder ?? undefined,
        keywords: script.metadata?.keywords,
        description: script.metadata?.description ?? undefined,
        accessories: scriptAccessories(script),
        liveOutputKey: liveKey,
        actionLabel: liveKey ? 'Refresh' : undefined,
        data: script.path,
      }
      if (script.metadata?.needs_confirmation) {
        commands.push({ ...base, createRootStep: () => scriptConfirmStep(script) })
      } else {
        commands.push({ ...base, action: async () => { await runScript(script.path) } })
      }
    }
  }

  return commands
}
