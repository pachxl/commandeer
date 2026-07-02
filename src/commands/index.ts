import type { AppConfig, Command, Step, PaletteItem } from '../types'
import { listScripts, runScript, type ScriptInfo } from '../lib/tauri'
import { searchFolderCommand } from './fileSearch'

export const builtinCommands: Command[] = [searchFolderCommand]

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
        icon: script.icon ?? 'folder',
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
