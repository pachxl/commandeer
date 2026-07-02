import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { openPath as tauriOpenPath, openUrl as tauriOpenUrl } from '@tauri-apps/plugin-opener'
import type { AppConfig } from '../types'

export interface ScriptInfo {
  name: string
  path: string
  ext: string
  icon: string | null
  folder: string | null
  is_folder: boolean
}

export const readConfig = () =>
  invoke<AppConfig>('read_config')

export const writeConfig = (config: AppConfig) =>
  invoke<void>('write_config', { config })

export const listScripts = (scriptsDir: string) =>
  invoke<ScriptInfo[]>('list_scripts', { scriptsDir })

export const runScript = (path: string) =>
  invoke<void>('run_script', { path })

export const setGameMode = (enabled: boolean) =>
  invoke<void>('set_game_mode', { enabled })

export interface ClaudeLimit {
  kind: string // 'session' | 'weekly_all' | 'weekly_scoped'
  percent: number
  severity: string // 'normal' | 'warning' | ...
  resets_at: string
  scope: { model?: { display_name?: string | null } | null } | null
}

export interface ClaudeUsageData {
  limits?: ClaudeLimit[]
}

export const claudeUsage = () =>
  invoke<ClaudeUsageData>('claude_usage')

export const openUrl = (url: string) =>
  tauriOpenUrl(url)

export const openPath = (path: string) =>
  tauriOpenPath(path)

export const dataDir = () =>
  invoke<string>('data_dir')

export interface Snippet {
  id: string
  keyword: string
  text: string
}

export const readSnippets = () =>
  invoke<Snippet[]>('read_snippets')

export const writeSnippets = (snippets: Snippet[]) =>
  invoke<void>('write_snippets', { snippets })

export const pasteToPrevious = (text: string) =>
  invoke<void>('paste_to_previous', { text })

export interface Theme {
  name: string
  variables: Record<string, string>
}

export const readThemes = () =>
  invoke<Theme[]>('read_themes')

export const setWindowTransparency = (transparency: number) => {
  const win = getCurrentWindow()
  return invoke<void>('set_window_transparency', { transparency, window: win })
}
