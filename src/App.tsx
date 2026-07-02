import { useEffect, useRef, useState } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { loadScriptCommands, scriptsToCommands, webSearchCommand } from './commands'
import { loadSnippetCommands } from './commands/snippets'
import { settingsCommand } from './commands/settings'
import { loadProviderCommands } from './providers'
import { killProcessCommand } from './providers/processes'
import { toolsFolderCommand, virtualFolderCommand } from './providers/tools'
import { appEvents } from './lib/appEvents'
import { applyThemeByName } from './lib/themes'
import { onCommandHotkey, readConfig, setGameMode, setWindowTransparency, type ScriptInfo } from './lib/tauri'
import type { AppConfig, Command } from './types'
import Palette from './components/Palette'

// Fallback used only until the real config (with a platform-appropriate
// scripts_dir) is loaded from the backend.
const EMPTY_CONFIG: AppConfig = { scripts_dir: '' }
const GAME_MODE_KEY = 'commandeer:gamemode'
const CLAUDE_USAGE_KEY = 'commandeer:claude-usage-visible'
const WEB_SEARCH_KEY = 'commandeer:web-search-visible'
const SYSTEM_STATS_KEY = 'commandeer:system-stats-visible'
const SCRIPTS_CACHE_KEY = 'commandeer:scripts'

// Read directly from localStorage (not React state) so refresh() always sees
// the current value without stale-closure issues. Defaults to visible.
const isWebSearchVisible = () => localStorage.getItem(WEB_SEARCH_KEY) !== 'false'

function loadCachedScripts(): ScriptInfo[] {
  try {
    const raw = localStorage.getItem(SCRIPTS_CACHE_KEY)
    return raw ? JSON.parse(raw) as ScriptInfo[] : []
  } catch {
    return []
  }
}

export default function App() {
  const [config, setConfig] = useState<AppConfig>(EMPTY_CONFIG)
  // Single mutable config object shared with settings steps: they update it
  // in place (Object.assign) so writes stay visible without re-creating commands.
  const configRef = useRef<AppConfig>({ ...EMPTY_CONFIG })
  const [commands, setCommands] = useState<Command[]>(
    () => [
      ...scriptsToCommands(loadCachedScripts()),
      ...(isWebSearchVisible() ? [webSearchCommand] : []),
      killProcessCommand,
      settingsCommand(configRef.current),
    ]
  )
  const [gameModeEnabled, setGameModeEnabled] = useState(
    () => localStorage.getItem(GAME_MODE_KEY) === 'true'
  )
  const [claudeUsageVisible, setClaudeUsageVisible] = useState(
    () => localStorage.getItem(CLAUDE_USAGE_KEY) === 'true'
  )
  const [systemStatsVisible, setSystemStatsVisible] = useState(
    () => localStorage.getItem(SYSTEM_STATS_KEY) !== 'false'
  )
  const resetRef = useRef<(() => void) | null>(null)
  // Palette registers its per-command hotkey handler here; the Rust side fires
  // 'command-hotkey' events for registered shortcuts and deep links
  const commandHotkeyRef = useRef<((commandId: string) => void) | null>(null)

  async function refresh() {
    try {
      const { commands: cmds, scripts } = await loadScriptCommands(configRef.current)
      localStorage.setItem(SCRIPTS_CACHE_KEY, JSON.stringify(scripts))
      const snippetCmds = await loadSnippetCommands().catch(err => {
        console.error(err)
        return [] as Command[]
      })
      const providerCmds = await loadProviderCommands(configRef.current).catch(err => {
        console.error(err)
        return [] as Command[]
      })
      // Commands tagged with a folderName group under virtual folders (like
      // script folders): hidden from root browse, still in the flat search
      const webSearchCmds = isWebSearchVisible() ? [webSearchCommand] : []
      const toolsChildren = [...providerCmds, ...webSearchCmds].filter(c => c.folderName === 'Tools')
      setCommands([
        ...cmds,
        ...(toolsChildren.length > 0 ? [toolsFolderCommand(toolsChildren)] : []),
        ...(snippetCmds.length > 0 ? [virtualFolderCommand('Snippets', snippetCmds)] : []),
        ...snippetCmds,
        ...webSearchCmds,
        ...providerCmds,
        settingsCommand(configRef.current),
      ])
    } catch (err) {
      console.error(err)
    }
  }

  useEffect(() => {
    let disposed = false
    let unlisten: (() => void) | undefined
    let unlistenHotkey: (() => void) | undefined

    ;(async () => {
      try {
        const cfg = await readConfig()
        // Merge into the shared mutable object so settings steps and the
        // Palette always see the same, current config.
        Object.assign(configRef.current, cfg)
        if (!disposed) setConfig(configRef.current)
        applyThemeByName(cfg.theme).catch(console.error)
        if (cfg.transparency !== undefined) {
          setWindowTransparency(cfg.transparency).catch(console.error)
        }
      } catch (err) {
        console.error(err)
      }

      await refresh()
      setGameMode(gameModeEnabled).catch(console.error)

      unlistenHotkey = await onCommandHotkey(id => commandHotkeyRef.current?.(id))

      const win = getCurrentWindow()
      unlisten = await win.onFocusChanged(({ payload: focused }) => {
        if (focused) {
          refresh()
          // Re-assert the saved transparency every time the launcher is shown.
          // The window is reused across hide/show, but layered-window alpha can
          // be dropped by the OS (and a value set while hidden at startup never
          // sticks), so reapply to keep every open consistent.
          const transparency = configRef.current.transparency
          if (transparency !== undefined) {
            setWindowTransparency(transparency).catch(console.error)
          }
        } else {
          resetRef.current?.()
        }
      })
      if (disposed) { unlisten?.(); unlistenHotkey?.() }
    })()

    return () => { disposed = true; unlisten?.(); unlistenHotkey?.() }
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  async function toggleGameMode() {
    const next = !gameModeEnabled
    setGameModeEnabled(next)
    localStorage.setItem(GAME_MODE_KEY, String(next))
    await setGameMode(next)
  }

  function toggleClaudeUsage() {
    const next = !claudeUsageVisible
    setClaudeUsageVisible(next)
    localStorage.setItem(CLAUDE_USAGE_KEY, String(next))
  }

  function toggleWebSearch() {
    localStorage.setItem(WEB_SEARCH_KEY, String(!isWebSearchVisible()))
    void refresh()
  }

  function toggleSystemStats() {
    const next = !systemStatsVisible
    setSystemStatsVisible(next)
    localStorage.setItem(SYSTEM_STATS_KEY, String(next))
  }

  // Keep the bridge fresh each render so settings commands see current state
  appEvents.toggleGameMode = () => { void toggleGameMode() }
  appEvents.toggleClaudeUsage = toggleClaudeUsage
  appEvents.toggleWebSearch = toggleWebSearch
  appEvents.toggleSystemStats = toggleSystemStats
  appEvents.isGameMode = () => gameModeEnabled
  appEvents.isClaudeUsageVisible = () => claudeUsageVisible
  appEvents.isWebSearchVisible = isWebSearchVisible
  appEvents.isSystemStatsVisible = () => systemStatsVisible
  appEvents.refreshCommands = () => { void refresh() }

  return (
    <Palette
      config={config}
      commands={commands}
      onConfigChange={() => {}}
      resetRef={resetRef}
      commandHotkeyRef={commandHotkeyRef}
      onToggleGameMode={toggleGameMode}
      claudeUsageVisible={claudeUsageVisible}
      systemStatsVisible={systemStatsVisible}
    />
  )
}
