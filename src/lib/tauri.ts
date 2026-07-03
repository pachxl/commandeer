import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
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

export interface ClipboardItem {
  id: string
  text: string
  copied_at: string
}

export const readClipboardHistory = () =>
  invoke<ClipboardItem[]>('read_clipboard_history')

export const clearClipboardHistory = () =>
  invoke<void>('clear_clipboard_history')

export const writeClipboardText = (text: string) =>
  invoke<void>('write_clipboard_text', { text })

// Daily-cached FX rates for calculator currency conversions. Refreshes at most
// once per day on the Rust side and serves a stale cache when offline.
export const getRates = () =>
  invoke<import('./math').CurrencyRates>('get_rates')

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

// Configurable global hotkeys (base + game mode) and per-command shortcuts
export const setGlobalHotkey = (hotkey: string, gameHotkey: string | null, gameMode: boolean) =>
  invoke<void>('set_global_hotkey', { update: { hotkey, game_hotkey: gameHotkey }, gameMode })

export const setCommandHotkey = (commandId: string, hotkey: string | null) =>
  invoke<void>('set_command_hotkey', { commandId, hotkey })

export const getCommandHotkey = (commandId: string) =>
  invoke<string | null>('get_command_hotkey', { commandId })

export const setAutostart = (enabled: boolean) =>
  invoke<void>('set_autostart', { enabled })

export const getAutostart = () =>
  invoke<boolean>('get_autostart')

// A registered per-command global shortcut (or a commandeer://command/<id>
// deep link) fired; payload is the command id
export const onCommandHotkey = (callback: (commandId: string) => void) =>
  listen<string>('command-hotkey', event => callback(event.payload))

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

// Installed applications (shell AppsFolder: win32 + UWP/Store), or Start-Menu
// shortcut paths when COM enumeration is unavailable
export interface AppInfo {
  name: string
  path: string
}

export const listApps = () =>
  invoke<AppInfo[]>('list_apps')

export const runApp = (path: string) =>
  invoke<void>('run_app', { path })

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

// System power/session actions, dispatched as direct Win32 calls (no shell)
export type SystemActionId =
  | 'lock'
  | 'sleep'
  | 'hibernate'
  | 'shutdown'
  | 'restart'
  | 'logout'
  | 'empty-trash'

export const systemAction = (action: SystemActionId) =>
  invoke<void>('system_action', { action })

// An active audio output device; id is the endpoint id the volume calls take
export interface AudioDevice {
  id: string
  name: string
  is_default: boolean
}

export const listAudioDevices = () =>
  invoke<AudioDevice[]>('list_audio_devices')

// Master volume of a device (omit for the default output), as a 0.0–1.0 scalar
export const getVolume = (device?: string) =>
  invoke<number>('get_volume', { device: device ?? null })

export const setVolume = (level: number, device?: string) =>
  invoke<void>('set_volume', { level, device: device ?? null })

// Atomically flips a device's mute and returns the new state
export const toggleMute = (device?: string) =>
  invoke<boolean>('toggle_mute', { device: device ?? null })

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
