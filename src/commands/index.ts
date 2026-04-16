import type { AppConfig, Command, Step, PaletteItem } from '../types'
import { listScripts, runScript } from '../lib/tauri'

export async function loadScriptCommands(config: AppConfig): Promise<Command[]> {
  const scripts = await listScripts(config.scripts_dir)
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
        description: script.path,
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
