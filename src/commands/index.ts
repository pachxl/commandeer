import type { AppConfig, Command } from '../types'
import { listScripts, runScript } from '../lib/tauri'

export async function loadScriptCommands(config: AppConfig): Promise<Command[]> {
  const scripts = await listScripts(config.scripts_dir)
  return scripts.map(script => ({
    id: `script:${script.path}`,
    label: script.name,
    description: script.path,
    icon: script.icon ?? '',
    action: async () => {
      await runScript(script.path)
    },
  }))
}
