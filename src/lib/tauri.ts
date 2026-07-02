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

export interface Quicklink {
  id: string
  name: string
  url: string
  icon: string | null
}

export const readQuicklinks = () =>
  invoke<Quicklink[]>('read_quicklinks')

export const writeQuicklinks = (quicklinks: Quicklink[]) =>
  invoke<void>('write_quicklinks', { quicklinks })

// Per-command user overrides (alias, pinned, hotkey), keyed by command id
export interface CommandOverride {
  alias?: string
  pinned?: boolean
  hotkey?: string
}

export const readOverrides = () =>
  invoke<Record<string, CommandOverride>>('read_overrides')

export const writeOverrides = (overrides: Record<string, CommandOverride>) =>
  invoke<void>('write_overrides', { overrides })

export const readSnippets = () =>
  invoke<Snippet[]>('read_snippets')

export const writeSnippets = (snippets: Snippet[]) =>
  invoke<void>('write_snippets', { snippets })

export const pasteToPrevious = (text: string) =>
  invoke<void>('paste_to_previous', { text })

export interface SystemStats {
  cpu: number
  mem_used: number
  mem_total: number
  mem_percent: number
  gpu: number | null
}

export const systemStats = () =>
  invoke<SystemStats>('system_stats')

export interface FileEntry {
  name: string
  path: string
  rel: string
  is_dir: boolean
}

// Folder open in the Explorer window focused before the palette was shown
export const explorerLocation = () =>
  invoke<string | null>('explorer_location')

export const listFilesRecursive = (path: string, max: number) =>
  invoke<FileEntry[]>('list_files_recursive', { path, max })

// Global file search: FTS5 index → Everything → walkdir fallback (Rust side)
export interface FileResult {
  name: string
  path: string
  icon: string | null
}

export const searchFiles = (query: string, paths: string[]) =>
  invoke<FileResult[]>('search_files', { query, paths })

export interface FileInfo {
  size: number
  modified: string | null
  is_dir: boolean
  thumbnail: string | null
}

export const fileInfo = (path: string) =>
  invoke<FileInfo>('file_info', { path })

// Shell icon for a path as a PNG data URL; cached per extension on both sides
export const pathIcon = (path: string) =>
  invoke<string | null>('path_icon', { path })

export interface ProcessInfo {
  pid: number
  name: string
  memory_bytes: number
  exe_path: string | null
}

export const listProcesses = () =>
  invoke<ProcessInfo[]>('list_processes')

export const killProcess = (pid: number) =>
  invoke<void>('kill_process', { pid })

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
