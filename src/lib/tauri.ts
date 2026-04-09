import { invoke } from '@tauri-apps/api/core'
import type { AppConfig } from '../types'

export interface ScriptInfo {
  name: string
  path: string
  ext: string
  icon: string | null
}

export const readConfig = () =>
  invoke<AppConfig>('read_config')

export const writeConfig = (config: AppConfig) =>
  invoke<void>('write_config', { config })

export const listScripts = (scriptsDir: string) =>
  invoke<ScriptInfo[]>('list_scripts', { scriptsDir })

export const runScript = (path: string) =>
  invoke<void>('run_script', { path })
