import type { AppConfig, Command, Step, PaletteItem } from '../types'
import { listScripts, runScript, openUrl, type ScriptInfo } from '../lib/tauri'

export const builtinCommands: Command[] = [
  {
    id: 'builtin:search',
    label: 'Search',
    icon: '🔍',
    description: 'Search the web',
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
  },
]

export async function loadScriptCommands(config: AppConfig): Promise<{ commands: Command[]; scripts: ScriptInfo[] }> {
  const scripts = await listScripts(config.scripts_dir)
  return { commands: scriptsToCommands(scripts), scripts }
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
        icon: script.icon ?? '📁',
        isFolder: true,
        createRootStep: (_cfg): Step => ({
          id: `folder-step:${folderName}`,
          label: folderName,
          placeholder: `Search ${folderName}...`,
          load: async (_cfg): Promise<PaletteItem[]> => folderScripts.map(s => ({
            id: `script:${s.path}`,
            label: s.name,
            icon: s.icon ?? '',
            data: s.path,
          })),
          onSelect: async (item, _cfg) => {
            await runScript(item.data as string)
            return { type: 'done' }
          },
        }),
      })
    } else {
      commands.push({
        id: `script:${script.path}`,
        label: script.name,
        icon: script.icon ?? '',
        folderName: script.folder ?? undefined,
        action: async () => {
          await runScript(script.path)
        },
      })
    }
  }

  return commands
}
